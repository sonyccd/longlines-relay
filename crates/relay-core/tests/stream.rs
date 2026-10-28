//! The spots stream (contract section 9) against a fake Long Lines: `GET /me` from wiremock, the
//! stream from a raw HTTP server that can hold a response open.

mod support;

use std::time::Duration;

use relay_core::engine::Engine;
use relay_core::state::{ClusterStatus, ClusterUpstream, UpstreamStatus};
use relay_core::upstream::token_store::TokenStore;
use serde_json::json;
use support::*;

struct Setup {
    server: FakeLongLines,
    stream: FakeStream,
    engine: Engine,
    tokens: std::sync::Arc<relay_core::upstream::token_store::MemoryTokenStore>,
    _dir: tempfile::TempDir,
}

/// A signed-in engine whose `GET /me` lists `scopes` and points at a fake stream.
async fn setup(scopes: &[&str], cluster_on: bool) -> Setup {
    let server = FakeLongLines::start().await;
    let stream = FakeStream::start().await;
    server.serve_v2(&stream, scopes);
    let dir = tempfile::tempdir().unwrap();
    let tokens = signed_in();
    let engine = start_engine_with(
        &dir.path().join("q.sqlite3"),
        &server.uri(),
        std::sync::Arc::clone(&tokens),
        |config| {
            config.cluster_enabled = cluster_on;
            config.keepalive_timeout = Duration::from_millis(600);
            config.stream_stable_after = Duration::from_millis(200);
        },
    )
    .await;
    wait_signed_in(&engine).await;
    Setup {
        server,
        stream,
        engine,
        tokens,
        _dir: dir,
    }
}

#[tokio::test]
async fn serves_spots_from_an_open_stream() {
    let s = setup(&BOTH_SCOPES, true).await;
    s.stream.wait("the stream to open", |st| st.open == 1).await;
    wait_until(&s.engine, "hello", |snap| {
        snap.cluster.upstream == ClusterUpstream::Connected
    })
    .await;

    let head = s.stream.requests()[0].to_ascii_lowercase();
    assert!(head.starts_with("get /v2/spots http/1.1"), "{head}");
    assert!(head.contains(&format!("authorization: bearer {TOKEN}")));
    assert!(head.contains("accept: text/event-stream"));
    assert!(head.contains("user-agent: longlinesrelay/0.1.0-test ("));
    assert!(!head.contains("last-event-id"));

    s.stream.send(": keepalive\n\n");
    s.stream.send(spot_event(&example_spot()));
    let snap = wait_until(&s.engine, "the spot", |snap| snap.sent.len() == 1).await;
    let row = &snap.sent[0];
    assert_eq!(row.spot_id, Some(48_213_377));
    assert_eq!(row.title, "K4ABC \u{b7} 14062.0 kHz CW");
    assert_eq!(row.detail, "POTA K-0039 \u{b7} from POTA states, CW");
    assert_eq!(row.time_utc, "21:14 UTC");
    assert_eq!(snap.cluster.status, ClusterStatus::Running);
    assert_eq!(snap.cluster.sent_today, 1);

    // A repeat of the same id within ten minutes is dropped; unknown events are ignored.
    s.stream.send(spot_event(&example_spot()));
    s.stream.send("event: news\ndata: {}\n\n");
    let mut other = example_spot();
    other["id"] = json!(2);
    other["callsign"] = json!("W9XYZ");
    s.stream.send(spot_event(&other));
    let snap = wait_until(&s.engine, "the second spot", |snap| snap.sent.len() == 2).await;
    assert_eq!(snap.sent[0].callsign, "W9XYZ");
    assert_eq!(s.stream.requests().len(), 1, "one stream, never two");
    s.engine.shutdown().await;
}

#[tokio::test]
async fn opens_only_while_the_switch_is_on() {
    let s = setup(&BOTH_SCOPES, false).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(s.stream.requests().is_empty());
    assert_eq!(s.engine.snapshot().cluster.status, ClusterStatus::Off);
    assert_eq!(s.engine.snapshot().cluster.upstream, ClusterUpstream::Off);

    let view = s.engine.set_cluster_enabled(true);
    assert!(view.enabled);
    s.stream.wait("the stream to open", |st| st.open == 1).await;

    s.engine.set_cluster_enabled(false);
    s.stream
        .wait("the stream to close", |st| st.open == 0 && st.closed == 1)
        .await;
    wait_until(&s.engine, "upstream off", |snap| {
        snap.cluster.upstream == ClusterUpstream::Off
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.stream.requests().len(), 1);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn closes_on_sign_out() {
    let s = setup(&BOTH_SCOPES, true).await;
    s.stream.wait("the stream to open", |st| st.open == 1).await;
    s.engine.sign_out().unwrap();
    s.stream
        .wait("the stream to close", |st| st.closed == 1)
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.stream.requests().len(), 1);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn reconnects_after_the_stream_ends_or_goes_quiet() {
    let s = setup(&BOTH_SCOPES, false).await;
    s.stream.reply(StreamReply::Close {
        body: hello_event() + &spot_event(&example_spot()),
    });
    s.stream.reply(StreamReply::Status {
        code: 503,
        error: "unavailable",
        retry_after: None,
    });
    s.stream.reply(StreamReply::Plain);
    // Then a stream that says hello and goes silent: closed after the keepalive timeout.
    s.stream.reply(StreamReply::Open {
        initial: hello_event(),
    });
    s.engine.set_cluster_enabled(true);
    wait_until(&s.engine, "the spot", |snap| snap.sent.len() == 1).await;
    s.stream
        .wait("the quiet stream to be dropped", |st| {
            st.closed == 1 && st.requests.len() >= 5
        })
        .await;
    // The ended stream's spot comes again on the next one: still served once.
    assert_eq!(s.engine.snapshot().sent.len(), 1);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn waits_out_retry_after() {
    let s = setup(&BOTH_SCOPES, false).await;
    s.stream.reply(StreamReply::Status {
        code: 429,
        error: "rate_limited",
        retry_after: Some(1),
    });
    let started = std::time::Instant::now();
    s.engine.set_cluster_enabled(true);
    wait_until(&s.engine, "retrying", |snap| {
        matches!(snap.cluster.upstream, ClusterUpstream::Retrying { .. })
    })
    .await;
    s.stream
        .wait("the second request", |st| st.requests.len() == 2)
        .await;
    assert!(started.elapsed() >= Duration::from_millis(950));
    s.engine.shutdown().await;
}

#[tokio::test]
async fn a_token_without_the_scope_never_reconnects() {
    let s = setup(&BOTH_SCOPES, false).await;
    s.stream.reply(StreamReply::Status {
        code: 403,
        error: "insufficient_scope",
        retry_after: None,
    });
    s.engine.set_cluster_enabled(true);
    let snap = wait_until(&s.engine, "scope missing", |snap| {
        snap.cluster.status == ClusterStatus::ScopeMissing
    })
    .await;
    assert_eq!(snap.cluster.label, "Sign in again");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(s.stream.requests().len(), 1);
    // Still signed in: only the stream needs a new pairing.
    assert!(s.tokens.load().unwrap().is_some());
    assert_eq!(s.engine.snapshot().upstream, UpstreamStatus::Connected);
    // A later `GET /me` that lists the scope does not open it again for this token.
    s.engine.refresh_account();
    wait_until(&s.engine, "the account again", |_| {
        s.server.state.lock().unwrap().me_calls >= 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.stream.requests().len(), 1);
    assert_eq!(
        s.engine.snapshot().cluster.status,
        ClusterStatus::ScopeMissing
    );

    // Signing out clears it.
    s.engine.sign_out().unwrap();
    wait_until(&s.engine, "cleared", |snap| {
        snap.cluster.status == ClusterStatus::Listening
    })
    .await;
    s.engine.shutdown().await;
}

#[tokio::test]
async fn a_v1_token_never_opens_the_stream() {
    let s = setup(&["contacts:write"], true).await;
    wait_until(&s.engine, "scope missing", |snap| {
        snap.cluster.status == ClusterStatus::ScopeMissing
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(s.stream.requests().is_empty());
    s.engine.shutdown().await;
}

#[tokio::test]
async fn a_refused_token_signs_out() {
    let s = setup(&BOTH_SCOPES, false).await;
    s.stream.reply(StreamReply::Status {
        code: 401,
        error: "bad_token",
        retry_after: None,
    });
    s.engine.set_cluster_enabled(true);
    wait_until(&s.engine, "signed out", |snap| {
        snap.upstream == UpstreamStatus::SignedOut
    })
    .await;
    assert!(s.tokens.load().unwrap().is_none());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.stream.requests().len(), 1);
    drop(s.server);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn never_sends_the_token_to_a_plain_http_host() {
    let s = setup(&BOTH_SCOPES, false).await;
    {
        let mut state = s.server.state.lock().unwrap();
        state.spots_url = Some("http://spots.example.com/v2/spots".into());
    }
    s.engine.refresh_account();
    wait_until(&s.engine, "the account again", |_| {
        s.server.state.lock().unwrap().me_calls >= 2
    })
    .await;
    s.engine.set_cluster_enabled(true);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(s.engine.snapshot().cluster.upstream, ClusterUpstream::Off);
    s.engine.shutdown().await;
}
