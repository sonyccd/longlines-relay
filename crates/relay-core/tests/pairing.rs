//! Sign-in by device pairing against a fake Long Lines (contract section 4).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use relay_core::engine::{Engine, EngineConfig};
use relay_core::source_filter::SourceFilter;
use relay_core::state::{Snapshot, UpstreamStatus};
use relay_core::upstream::auth::{PairingFailure, PairingView};
use relay_core::upstream::client::ClientConfig;
use relay_core::upstream::token_store::{MemoryTokenStore, Token, TokenStore};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const TOKEN: &str = "ll_relay_abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst";

/// What `POST /device/token` answers, in order; the last answer repeats.
#[derive(Clone, Debug)]
enum Answer {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Approved,
    Status(u16),
}

struct TokenResponder {
    answers: Mutex<Vec<Answer>>,
    polls: Arc<Mutex<Vec<Instant>>>,
}

impl Respond for TokenResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        assert_eq!(body["device_code"], "dc-secret");
        self.polls.lock().unwrap().push(Instant::now());
        let mut answers = self.answers.lock().unwrap();
        let answer = if answers.len() > 1 {
            answers.remove(0)
        } else {
            answers[0].clone()
        };
        let error = |code: &str| ResponseTemplate::new(400).set_body_json(json!({"error": code}));
        match answer {
            Answer::Pending => error("authorization_pending"),
            Answer::SlowDown => error("slow_down"),
            Answer::Denied => error("access_denied"),
            Answer::Expired => error("expired_token"),
            Answer::Approved => ResponseTemplate::new(200).set_body_json(json!({
                "access_token": TOKEN,
                "token_type": "bearer",
                "scopes": ["contacts:write"],
            })),
            Answer::Status(status) => ResponseTemplate::new(status),
        }
    }
}

struct Fake {
    server: MockServer,
    polls: Arc<Mutex<Vec<Instant>>>,
    starts: Arc<AtomicUsize>,
}

struct StartResponder {
    status: u16,
    expires_in: u64,
    interval: u64,
    starts: Arc<AtomicUsize>,
}

impl Respond for StartResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        assert_eq!(body["client_name"], "Long Lines Relay");
        assert_eq!(body["device_name"], "SHACK-PC");
        if self.status != 200 {
            return ResponseTemplate::new(self.status);
        }
        ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dc-secret",
            "user_code": "KQ7M-X4PD",
            "verification_uri": "https://longlines.io/relay",
            "verification_uri_complete": "https://longlines.io/relay/approve?code=KQ7M-X4PD",
            "expires_in": self.expires_in,
            "interval": self.interval,
        }))
    }
}

async fn fake(start_status: u16, expires_in: u64, answers: Vec<Answer>) -> Fake {
    let server = MockServer::start().await;
    let polls = Arc::new(Mutex::new(Vec::new()));
    let starts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/device/start"))
        .respond_with(StartResponder {
            status: start_status,
            expires_in,
            interval: 5,
            starts: Arc::clone(&starts),
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/device/token"))
        .respond_with(TokenResponder {
            answers: Mutex::new(answers),
            polls: Arc::clone(&polls),
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"callsign": "KK4PWJ", "email": "op@example.com"})),
        )
        .mount(&server)
        .await;
    Fake {
        server,
        polls,
        starts,
    }
}

/// One protocol second is 10 ms here, so `interval: 5` is 50 ms.
const SECOND: Duration = Duration::from_millis(10);

async fn engine(dir: &tempfile::TempDir, base_url: &str, tokens: Arc<MemoryTokenStore>) -> Engine {
    let mut config = EngineConfig::new(
        0,
        dir.path().join("q.sqlite3"),
        ClientConfig::new(base_url, "0.1.0-test"),
        tokens,
    );
    config.source_filter = Arc::new(SourceFilter::with_addresses([]));
    config.idle_tick = Duration::from_millis(50);
    config.pairing_second = SECOND;
    Engine::start(config).await.expect("engine starts")
}

async fn wait_for(engine: &Engine, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = engine.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; last snapshot: {snapshot:#?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn approval_signs_in_and_stores_the_token() {
    let fake = fake(
        200,
        600,
        vec![Answer::Pending, Answer::Pending, Answer::Approved],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(MemoryTokenStore::new());
    let engine = engine(&dir, &fake.server.uri(), Arc::clone(&tokens)).await;
    assert_eq!(engine.snapshot().pairing, PairingView::Idle);

    let view = engine.start_sign_in("SHACK-PC").await.unwrap();
    let PairingView::Waiting {
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_at,
    } = view
    else {
        panic!("not waiting: {view:?}");
    };
    assert_eq!(user_code, "KQ7M-X4PD");
    assert_eq!(verification_uri, "https://longlines.io/relay");
    assert!(verification_uri_complete.ends_with("?code=KQ7M-X4PD"));
    assert!(expires_at > chrono::Utc::now());
    assert!(matches!(
        engine.snapshot().pairing,
        PairingView::Waiting { .. }
    ));

    let snap = wait_for(&engine, "the account", |s| s.account.is_some()).await;
    assert_eq!(snap.pairing, PairingView::Idle);
    assert_eq!(snap.account.unwrap().callsign, "KK4PWJ");
    assert_eq!(snap.upstream, UpstreamStatus::Connected);
    assert_eq!(tokens.load().unwrap(), Some(Token::new(TOKEN)));

    // Polls are at least `interval` apart.
    let polls = fake.polls.lock().unwrap().clone();
    assert_eq!(polls.len(), 3);
    for pair in polls.windows(2) {
        assert!(pair[1] - pair[0] >= SECOND * 5, "{:?}", pair[1] - pair[0]);
    }
    engine.shutdown().await;
}

#[tokio::test]
async fn slow_down_adds_five_seconds() {
    let fake = fake(
        200,
        600,
        vec![Answer::SlowDown, Answer::Pending, Answer::Approved],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    wait_for(&engine, "the account", |s| s.account.is_some()).await;
    let polls = fake.polls.lock().unwrap().clone();
    assert_eq!(polls.len(), 3);
    assert!(polls[1] - polls[0] >= SECOND * 10);
    assert!(polls[2] - polls[1] >= SECOND * 10);
    engine.shutdown().await;
}

#[tokio::test]
async fn denied_goes_back_to_signed_out() {
    let fake = fake(200, 600, vec![Answer::Pending, Answer::Denied]).await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(MemoryTokenStore::new());
    let engine = engine(&dir, &fake.server.uri(), Arc::clone(&tokens)).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    let snap = wait_for(&engine, "denied", |s| {
        matches!(s.pairing, PairingView::Failed { .. })
    })
    .await;
    assert_eq!(
        snap.pairing,
        PairingView::Failed {
            failure: PairingFailure::Denied
        }
    );
    assert_eq!(snap.upstream, UpstreamStatus::SignedOut);
    assert_eq!(tokens.load().unwrap(), None);
    engine.shutdown().await;
}

#[tokio::test]
async fn the_server_saying_expired_stops_polling() {
    let fake = fake(200, 600, vec![Answer::Expired]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    let snap = wait_for(&engine, "expired", |s| {
        matches!(s.pairing, PairingView::Failed { .. })
    })
    .await;
    assert_eq!(
        snap.pairing,
        PairingView::Failed {
            failure: PairingFailure::Expired
        }
    );
    tokio::time::sleep(SECOND * 12).await;
    assert_eq!(fake.polls.lock().unwrap().len(), 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn the_code_expires_on_the_clients_clock() {
    // 12 protocol seconds with a 5 s interval: two polls, then expired without a third.
    let fake = fake(200, 12, vec![Answer::Pending]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    let started = Instant::now();
    engine.start_sign_in("SHACK-PC").await.unwrap();
    let snap = wait_for(&engine, "expired", |s| {
        matches!(s.pairing, PairingView::Failed { .. })
    })
    .await;
    assert!(started.elapsed() >= SECOND * 12);
    assert_eq!(
        snap.pairing,
        PairingView::Failed {
            failure: PairingFailure::Expired
        }
    );
    assert_eq!(fake.polls.lock().unwrap().len(), 2);
    engine.shutdown().await;
}

#[tokio::test]
async fn server_errors_while_polling_keep_polling() {
    let fake = fake(
        200,
        600,
        vec![Answer::Status(503), Answer::Status(500), Answer::Approved],
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    wait_for(&engine, "the account", |s| s.account.is_some()).await;
    assert_eq!(fake.polls.lock().unwrap().len(), 3);
    engine.shutdown().await;
}

#[tokio::test]
async fn cancel_stops_polling() {
    let fake = fake(200, 600, vec![Answer::Pending]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    engine.cancel_sign_in();
    assert_eq!(engine.snapshot().pairing, PairingView::Idle);
    tokio::time::sleep(SECOND * 12).await;
    assert!(fake.polls.lock().unwrap().is_empty());
    assert_eq!(engine.snapshot().pairing, PairingView::Idle);
    engine.shutdown().await;
}

#[tokio::test]
async fn starting_again_replaces_the_first_attempt() {
    let fake = fake(200, 600, vec![Answer::Pending]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    engine.start_sign_in("SHACK-PC").await.unwrap();
    engine.start_sign_in("SHACK-PC").await.unwrap();
    assert_eq!(fake.starts.load(Ordering::SeqCst), 2);
    tokio::time::sleep(SECOND * 7).await;
    // Only the second attempt polls: one poll after one interval, not two.
    assert_eq!(fake.polls.lock().unwrap().len(), 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn an_unreachable_server_fails_to_start() {
    let fake = fake(503, 600, vec![Answer::Pending]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    assert_eq!(
        engine.start_sign_in("SHACK-PC").await,
        Err(PairingFailure::Unreachable)
    );
    assert_eq!(
        engine.snapshot().pairing,
        PairingView::Failed {
            failure: PairingFailure::Unreachable
        }
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn a_refused_start_is_refused() {
    let fake = fake(400, 600, vec![Answer::Pending]).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir, &fake.server.uri(), Arc::new(MemoryTokenStore::new())).await;
    assert_eq!(
        engine.start_sign_in("SHACK-PC").await,
        Err(PairingFailure::Refused)
    );
    engine.shutdown().await;
}
