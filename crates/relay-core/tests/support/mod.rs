//! A fake Long Lines for the integration tests: a wiremock server whose responder follows the
//! contract's `GET /me` and `POST /contacts` semantics, plus helpers to run an engine against it.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use relay_core::callsign::base_callsign;
use relay_core::engine::{Engine, EngineConfig};
use relay_core::listeners::ListenerStatus;
use relay_core::source_filter::SourceFilter;
use relay_core::state::Snapshot;
use relay_core::upstream::client::ClientConfig;
use relay_core::upstream::queue::Backoff;
use relay_core::upstream::token_store::{MemoryTokenStore, Token};
use serde_json::{json, Value};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TOKEN: &str = "ll_relay_abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst";
pub const CALLSIGN: &str = "KK4PWJ";
pub const EMAIL: &str = "operator@example.com";

/// A canned answer for the next `POST /contacts`, ahead of the normal handling.
#[derive(Clone, Debug)]
pub enum Scripted {
    Status(u16),
    RateLimited { retry_after_secs: u64 },
}

#[derive(Clone, Debug)]
pub struct ContactsRequest {
    pub at: Instant,
    pub body: Value,
    pub user_agent: String,
}

#[derive(Debug)]
struct Stored {
    content: Value,
    live: bool,
}

#[derive(Debug, Default)]
pub struct ServerState {
    pub revoked: bool,
    /// Answer every `POST /contacts` with this status while set.
    pub fail_contacts: Option<u16>,
    pub script: VecDeque<Scripted>,
    pub requests: Vec<ContactsRequest>,
    pub me_calls: usize,
    contacts: HashMap<String, Stored>,
}

struct Handler {
    state: Arc<Mutex<ServerState>>,
}

impl Respond for Handler {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut state = self.state.lock().unwrap();
        let authorized = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {TOKEN}"));
        match (request.method.as_str(), request.url.path()) {
            ("GET", "/me") => {
                state.me_calls += 1;
                if !authorized || state.revoked {
                    return ResponseTemplate::new(401).set_body_json(json!({"error": "bad_token"}));
                }
                ResponseTemplate::new(200)
                    .set_body_json(json!({"callsign": CALLSIGN, "email": EMAIL}))
            }
            ("POST", "/contacts") => {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let user_agent = request
                    .headers
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                state.requests.push(ContactsRequest {
                    at: Instant::now(),
                    body: body.clone(),
                    user_agent,
                });
                if !authorized || state.revoked {
                    return ResponseTemplate::new(401).set_body_json(json!({"error": "bad_token"}));
                }
                if let Some(status) = state.fail_contacts {
                    return ResponseTemplate::new(status);
                }
                match state.script.pop_front() {
                    Some(Scripted::Status(status)) => return ResponseTemplate::new(status),
                    Some(Scripted::RateLimited { retry_after_secs }) => {
                        return ResponseTemplate::new(429)
                            .insert_header("Retry-After", retry_after_secs.to_string().as_str())
                            .set_body_json(json!({"error": "rate_limited"}));
                    }
                    None => {}
                }
                let results: Vec<Value> = body["contacts"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|item| apply(&mut state, item))
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"results": results}))
            }
            _ => ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})),
        }
    }
}

fn apply(state: &mut ServerState, item: &Value) -> Value {
    let id = item["external_id"].as_str().unwrap_or_default().to_string();
    match item["op"].as_str() {
        Some("upsert") => {
            if base_callsign(item["mycall"].as_str().unwrap_or_default()) != base_callsign(CALLSIGN)
            {
                return json!({"external_id": id, "status": "rejected", "reason": "mycall_mismatch"});
            }
            let content = json!([
                item["mycall"],
                item["call"],
                item["frequency_hz"],
                item["band"],
                item["mode"],
                item["timestamp_utc"]
            ]);
            let status = match state.contacts.get(&id) {
                Some(s) if s.live && s.content == content => "unchanged",
                Some(s) if s.live => "updated",
                _ => "created",
            };
            state.contacts.insert(
                id.clone(),
                Stored {
                    content,
                    live: true,
                },
            );
            json!({"external_id": id, "status": status})
        }
        Some("delete") => {
            let status = match state.contacts.get_mut(&id) {
                Some(s) if s.live => {
                    s.live = false;
                    "retracted"
                }
                _ => "unknown",
            };
            json!({"external_id": id, "status": status})
        }
        _ => json!({"external_id": id, "status": "rejected", "reason": "invalid"}),
    }
}

/// The fake server.
pub struct FakeLongLines {
    pub server: MockServer,
    pub state: Arc<Mutex<ServerState>>,
}

impl FakeLongLines {
    pub async fn start() -> Self {
        Self::mount(MockServer::start().await).await
    }

    /// Starts on a listener the test already holds (to come up on a known port).
    pub async fn start_on(listener: std::net::TcpListener) -> Self {
        Self::mount(MockServer::builder().listener(listener).start().await).await
    }

    async fn mount(server: MockServer) -> Self {
        let state = Arc::new(Mutex::new(ServerState::default()));
        Mock::given(any())
            .respond_with(Handler {
                state: Arc::clone(&state),
            })
            .mount(&server)
            .await;
        Self { server, state }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    pub fn requests(&self) -> Vec<ContactsRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Every item sent, in order.
    pub fn items(&self) -> Vec<Value> {
        self.requests()
            .iter()
            .flat_map(|r| r.body["contacts"].as_array().cloned().unwrap_or_default())
            .collect()
    }

    pub fn is_live(&self, external_id: &str) -> Option<bool> {
        self.state
            .lock()
            .unwrap()
            .contacts
            .get(external_id)
            .map(|s| s.live)
    }

    pub fn stored_frequency(&self, external_id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap()
            .contacts
            .get(external_id)
            .and_then(|s| s.content[2].as_u64())
    }

    pub fn script(&self, step: Scripted) {
        self.state.lock().unwrap().script.push_back(step);
    }

    pub fn set_fail_contacts(&self, status: Option<u16>) {
        self.state.lock().unwrap().fail_contacts = status;
    }

    pub fn revoke(&self) {
        self.state.lock().unwrap().revoked = true;
    }
}

/// A token store holding the test token.
pub fn signed_in() -> Arc<MemoryTokenStore> {
    Arc::new(MemoryTokenStore::with_token(Token::new(TOKEN)))
}

/// An engine on an ephemeral port with fast backoff, against `base_url`.
pub async fn start_engine(
    queue_path: &Path,
    base_url: &str,
    tokens: Arc<MemoryTokenStore>,
) -> Engine {
    let mut config = EngineConfig::new(
        0,
        queue_path.to_path_buf(),
        ClientConfig::new(base_url, "0.1.0-test"),
        tokens,
    );
    config.source_filter = Arc::new(SourceFilter::with_addresses([]));
    config.backoff = Backoff {
        base: Duration::from_millis(100),
        max: Duration::from_millis(400),
    };
    config.idle_tick = Duration::from_millis(50);
    Engine::start(config).await.expect("engine starts")
}

/// The bound UDP port.
pub fn port(engine: &Engine) -> u16 {
    match engine.snapshot().feed.listener {
        ListenerStatus::Listening { port } => port,
        other => panic!("not listening: {other:?}"),
    }
}

/// Polls the snapshot until `done` holds, or fails after 10 seconds.
pub async fn wait_until(engine: &Engine, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = engine.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}; last snapshot: {snapshot:#?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// `(title, status line)` for every row, newest first.
pub fn rows(snapshot: &Snapshot) -> Vec<(String, String)> {
    snapshot
        .stream
        .iter()
        .map(|r| (r.title.clone(), r.status_text.clone()))
        .collect()
}

/// Waits until the account is known.
pub async fn wait_signed_in(engine: &Engine) {
    wait_until(engine, "the account", |s| s.account.is_some()).await;
}

/// Waits until every row's status is final and there are `count` rows.
pub async fn wait_rows_final(engine: &Engine, count: usize) -> Snapshot {
    wait_until(engine, &format!("{count} final rows"), |s| {
        s.stream.len() == count && s.stream.iter().all(|r| r.status.is_final())
    })
    .await
}
