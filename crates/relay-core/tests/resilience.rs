//! Integration tests for what goes wrong around the engine: a misbehaving server, a damaged
//! queue, a held port and a credential store that fails.

mod support;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use relay_core::engine::{Engine, EngineConfig};
use relay_core::fake_logger::{send, FakeContact};
use relay_core::listeners::ListenerStatus;
use relay_core::source_filter::SourceFilter;
use relay_core::state::{FeedStatus, UpstreamStatus};
use relay_core::status::RowStatus;
use relay_core::upstream::client::{ClientConfig, MAX_RETRY_AFTER};
use relay_core::upstream::queue::{Backoff, Queue};
use relay_core::upstream::token_store::{MemoryTokenStore, Token, TokenStore, TokenStoreError};
use support::*;

/// A token store whose reads and deletes can be made to fail, like a locked keyring. Unlike
/// `CachingTokenStore` it keeps the token when a delete fails, so the engine has to cope alone.
#[derive(Debug, Default)]
struct FlakyStore {
    inner: MemoryTokenStore,
    fail_load: AtomicBool,
    fail_delete: AtomicBool,
}

impl FlakyStore {
    fn with_token() -> Arc<Self> {
        let store = Self::default();
        store.inner.save(&Token::new(TOKEN)).unwrap();
        Arc::new(store)
    }
}

impl TokenStore for FlakyStore {
    fn load(&self) -> Result<Option<Token>, TokenStoreError> {
        if self.fail_load.load(Ordering::SeqCst) {
            return Err(TokenStoreError("locked".into()));
        }
        self.inner.load()
    }
    fn save(&self, token: &Token) -> Result<(), TokenStoreError> {
        self.inner.save(token)
    }
    fn delete(&self) -> Result<(), TokenStoreError> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(TokenStoreError("locked".into()));
        }
        self.inner.delete()
    }
}

fn config(dir: &Path, base_url: &str, tokens: Arc<dyn TokenStore>) -> EngineConfig {
    let mut config = EngineConfig::new(
        0,
        dir.join("q.sqlite3"),
        ClientConfig::new(base_url, "0.1.0-test"),
        tokens,
    );
    config.source_filter = Arc::new(SourceFilter::new());
    config.backoff = Backoff {
        base: Duration::from_millis(100),
        max: Duration::from_millis(400),
    };
    config.idle_tick = Duration::from_millis(50);
    config.port_retry = Duration::from_millis(50);
    config
}

fn contact(call: &str) -> FakeContact {
    FakeContact::new(CALLSIGN, call, Utc::now() - TimeDelta::seconds(5))
}

/// A port nothing holds right now.
fn free_port() -> u16 {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    probe.local_addr().unwrap().port()
}

#[tokio::test]
async fn a_huge_retry_after_is_capped_instead_of_stopping_the_sender() {
    let server = FakeLongLines::start().await;
    server.script(Scripted::RateLimited {
        retry_after_secs: 100_000_000_000_000,
    });
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    send(port(&engine), contact("W1AW").info_xml().as_bytes()).unwrap();
    let snap = wait_until(&engine, "rate limited", |s| {
        matches!(s.upstream, UpstreamStatus::RateLimited { .. })
    })
    .await;
    let UpstreamStatus::RateLimited { until } = snap.upstream else {
        unreachable!()
    };
    let cap = TimeDelta::from_std(MAX_RETRY_AFTER).unwrap() + TimeDelta::seconds(5);
    assert!(until <= Utc::now() + cap, "waits until {until}");
    assert_eq!(snap.stream[0].status, RowStatus::Waiting);
    engine.shutdown().await;
}

#[tokio::test]
async fn an_unreadable_queue_row_does_not_stop_the_engine() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("q.sqlite3");
    drop(Queue::open(&path, Backoff::DEFAULT).unwrap());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "insert into outbox (external_id, op, body, created_at, next_at) values ('x', 'upsert', '{\"from\": \"a newer relay\"}', 0, 0)",
            [],
        )
        .unwrap();

    let engine = start_engine(&path, &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    assert!(engine.snapshot().stream.is_empty());
    send(port(&engine), contact("W1AW").info_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    engine.shutdown().await;
}

#[tokio::test]
async fn a_port_freed_by_another_program_is_picked_up() {
    let holder = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let held = holder.local_addr().unwrap().port();
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = config(dir.path(), &server.uri(), signed_in());
    config.port = held;
    let engine = Engine::start(config).await.unwrap();
    assert_eq!(engine.snapshot().feed.status, FeedStatus::PortInUse);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(engine.snapshot().feed.status, FeedStatus::PortInUse);

    drop(holder);
    let snap = wait_until(&engine, "the port", |s| {
        s.feed.listener == ListenerStatus::Listening { port: held }
    })
    .await;
    assert_ne!(snap.feed.status, FeedStatus::PortInUse);
    wait_signed_in(&engine).await;
    send(held, contact("W1AW").info_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    engine.shutdown().await;
}

#[tokio::test]
async fn changing_ports_and_back_never_reports_the_relay_own_socket() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(config(dir.path(), &server.uri(), signed_in()))
        .await
        .unwrap();
    let first = port(&engine);
    for _ in 0..10 {
        assert_eq!(
            engine.set_port(first).await,
            ListenerStatus::Listening { port: first }
        );
    }
    let other = free_port();
    for _ in 0..10 {
        assert_eq!(
            engine.set_port(other).await,
            ListenerStatus::Listening { port: other }
        );
        assert_eq!(
            engine.set_port(first).await,
            ListenerStatus::Listening { port: first }
        );
    }
    engine.shutdown().await;
}

#[tokio::test]
async fn a_token_that_wont_delete_is_never_used_again() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = FlakyStore::with_token();
    store.fail_delete.store(true, Ordering::SeqCst);
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    wait_signed_in(&engine).await;

    assert!(engine.sign_out().is_err());
    let snap = engine.snapshot();
    assert!(snap.sign_out_incomplete);
    assert_eq!(snap.upstream, UpstreamStatus::SignedOut);
    assert_eq!(snap.account, None);
    send(port(&engine), contact("W1AW").info_xml().as_bytes()).unwrap();
    wait_until(&engine, "a row", |s| s.stream.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.requests().is_empty(), "sent with a signed-out token");
    assert_eq!(engine.snapshot().upstream, UpstreamStatus::SignedOut);
    engine.shutdown().await;

    // The next launch remembers the sign-out although the token is still stored.
    assert!(store.load().unwrap().is_some());
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    let snap = engine.snapshot();
    assert!(snap.sign_out_incomplete);
    assert_eq!(snap.upstream, UpstreamStatus::SignedOut);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.requests().is_empty());
    assert_eq!(engine.snapshot().account, None);

    // Once the credential store works again the token is deleted.
    store.fail_delete.store(false, Ordering::SeqCst);
    wait_until(&engine, "the delete", |s| !s.sign_out_incomplete).await;
    assert_eq!(store.load().unwrap(), None);
    assert!(!dir.path().join("sign-out-pending").exists());
    engine.shutdown().await;
}

#[tokio::test]
async fn signing_in_again_replaces_a_token_that_wouldnt_delete() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = FlakyStore::with_token();
    store.fail_delete.store(true, Ordering::SeqCst);
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    wait_signed_in(&engine).await;
    assert!(engine.sign_out().is_err());
    assert!(dir.path().join("sign-out-pending").exists());

    engine.sign_in(Token::new(TOKEN)).await.unwrap();
    let snap = engine.snapshot();
    assert!(!snap.sign_out_incomplete);
    assert!(!dir.path().join("sign-out-pending").exists());
    send(port(&engine), contact("W1AW").info_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    engine.shutdown().await;
}

#[tokio::test]
async fn a_locked_credential_store_is_not_signed_out() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = FlakyStore::with_token();
    store.fail_load.store(true, Ordering::SeqCst);
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().upstream,
        UpstreamStatus::CredentialStoreUnavailable
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        engine.snapshot().upstream,
        UpstreamStatus::CredentialStoreUnavailable
    );

    store.fail_load.store(false, Ordering::SeqCst);
    wait_signed_in(&engine).await;
    assert_eq!(engine.snapshot().upstream, UpstreamStatus::Connected);
    engine.shutdown().await;
}

#[tokio::test]
async fn the_spots_stream_never_opens_with_a_token_that_wont_delete() {
    let server = FakeLongLines::start().await;
    let stream = FakeStream::start().await;
    server.serve_v2(&stream, &BOTH_SCOPES);
    let dir = tempfile::tempdir().unwrap();
    let store = FlakyStore::with_token();
    store.fail_delete.store(true, Ordering::SeqCst);
    let mut config = config(dir.path(), &server.uri(), store.clone());
    config.cluster_enabled = true;
    let engine = Engine::start(config).await.unwrap();
    wait_signed_in(&engine).await;
    stream.wait("the stream to open", |st| st.open == 1).await;

    assert!(engine.sign_out().is_err());
    stream.wait("the stream to close", |st| st.open == 0).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        stream.requests().len(),
        1,
        "reopened with a signed-out token"
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn an_account_answer_that_arrives_after_sign_out_is_ignored() {
    let server = FakeLongLines::start().await;
    server.set_me_delay(Duration::from_millis(600));
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    // A contact waits in the queue while the sender's `GET /me` is out.
    send(port(&engine), contact("W1AW").info_xml().as_bytes()).unwrap();
    wait_until(&engine, "a row", |s| s.stream.len() == 1).await;
    while server.state.lock().unwrap().me_calls == 0 {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(engine.snapshot().account, None);

    engine.sign_out().unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let snap = engine.snapshot();
    assert_eq!(
        snap.account, None,
        "the stale answer brought the account back"
    );
    assert_eq!(snap.upstream, UpstreamStatus::SignedOut);
    assert!(server.requests().is_empty(), "sent with a signed-out token");
    engine.shutdown().await;
}

#[tokio::test]
async fn a_sign_out_marker_that_couldnt_be_written_is_written_later() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let store = FlakyStore::with_token();
    store.fail_delete.store(true, Ordering::SeqCst);
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    wait_signed_in(&engine).await;
    // A directory in the marker's place makes writing it fail.
    let marker = dir.path().join("sign-out-pending");
    std::fs::create_dir(&marker).unwrap();

    assert!(engine.sign_out().is_err());
    assert!(engine.snapshot().sign_out_incomplete);
    assert!(marker.is_dir());
    std::fs::remove_dir(&marker).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !marker.is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "the marker was never written"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    engine.shutdown().await;

    // So the next launch still knows the stored token was signed out.
    let engine = Engine::start(config(dir.path(), &server.uri(), store.clone()))
        .await
        .unwrap();
    assert!(engine.snapshot().sign_out_incomplete);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.requests().is_empty());
    assert_eq!(engine.snapshot().account, None);
    engine.shutdown().await;
}
