//! Integration tests: fake-logger UDP -> engine -> fake Long Lines (wiremock).

mod support;

use std::time::Duration;

use chrono::{TimeDelta, Utc};
use relay_core::bus::{AuthEvent, Event, SignOutReason};
use relay_core::fake_logger::{radioinfo_xml, send, FakeContact, MALFORMED};
use relay_core::state::{FeedStatus, UpstreamStatus};
use relay_core::status::RowStatus;
use relay_core::upstream::token_store::{MemoryTokenStore, Token, TokenStore};
use support::*;

fn contact(mycall: &str, call: &str) -> FakeContact {
    FakeContact::new(mycall, call, Utc::now() - TimeDelta::seconds(5))
}

fn status_of(engine: &relay_core::engine::Engine, i: usize) -> RowStatus {
    engine.snapshot().stream[i].status.clone()
}

#[tokio::test]
async fn sends_a_contact_as_a_spot() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let account = engine.snapshot().account.unwrap();
    assert_eq!(account.callsign, CALLSIGN);
    assert_eq!(account.email, EMAIL);

    let c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(
        rows(&snap),
        [(
            "W1AW \u{b7} 20 m CW".to_string(),
            "Sent as a spot".to_string()
        )]
    );
    assert_eq!(snap.feed.status, FeedStatus::Receiving);

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0]
        .user_agent
        .starts_with("LongLinesRelay/0.1.0-test ("));
    assert_eq!(requests[0].body["relay_version"], "0.1.0-test");
    let item = &requests[0].body["contacts"][0];
    assert_eq!(item["external_id"], c.id.clone().unwrap());
    assert_eq!(item["op"], "upsert");
    assert_eq!(item["mycall"], "KK4PWJ");
    assert_eq!(item["call"], "W1AW");
    assert_eq!(item["frequency_hz"], 14_025_000);
    assert_eq!(item["band"], "20m");
    assert_eq!(item["mode"], "CW");
    assert_eq!(
        item["timestamp_utc"],
        c.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    );
    assert_eq!(item["logger_app"], "N1MM");
    engine.shutdown().await;
}

#[tokio::test]
async fn a_contact_without_an_id_uses_the_hash() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let mut c = contact("KK4PWJ", "W1AW");
    c.id = None;
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_rows_final(&engine, 1).await;
    let items = server.items();
    let want = relay_core::external_id::fallback_hash(relay_core::external_id::HashFields {
        mycall: "KK4PWJ",
        call: "W1AW",
        band: Some("20m"),
        mode: "CW",
        timestamp_utc: &c.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    });
    assert_eq!(items[0]["external_id"], want);
    engine.shutdown().await;
}

#[tokio::test]
async fn offline_queue_sends_once_the_server_is_back() {
    // Hold a port, then let it go: nothing listens there until the server starts.
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap();
    drop(held);
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(
        &dir.path().join("q.sqlite3"),
        &format!("http://{addr}"),
        signed_in(),
    )
    .await;

    let c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_until(&engine, "a retry", |s| {
        matches!(s.upstream, UpstreamStatus::Retrying { .. })
    })
    .await;
    let snap = engine.snapshot();
    assert_eq!(snap.stream[0].status, RowStatus::Waiting);
    assert_eq!(snap.stream[0].status_text, "Waiting to send");
    // UDP keeps working while Long Lines is unreachable.
    send(port(&engine), radioinfo_xml().as_bytes()).unwrap();
    wait_until(&engine, "the radioinfo packet", |s| {
        s.feed.last_packet_at > snap.feed.last_packet_at
    })
    .await;

    let server = FakeLongLines::start_on(std::net::TcpListener::bind(addr).unwrap()).await;
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    assert_eq!(server.items().len(), 1);
    assert_eq!(server.is_live(c.id.as_deref().unwrap()), Some(true));
    engine.shutdown().await;
}

#[tokio::test]
async fn server_errors_retry_the_same_request() {
    let server = FakeLongLines::start().await;
    server.script(Scripted::Status(503));
    server.script(Scripted::Status(500));
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in &requests {
        assert_eq!(r.body["contacts"][0]["external_id"], c.id.clone().unwrap());
    }
    // Backoff: 100 ms, then 200 ms.
    assert!(requests[1].at - requests[0].at >= Duration::from_millis(90));
    assert!(requests[2].at - requests[1].at >= Duration::from_millis(190));
    engine.shutdown().await;
}

#[tokio::test]
async fn restart_mid_queue_loses_nothing() {
    let server = FakeLongLines::start().await;
    server.set_fail_contacts(Some(503));
    let dir = tempfile::tempdir().unwrap();
    let queue = dir.path().join("q.sqlite3");
    let first = contact("KK4PWJ", "W1AW");
    let second = contact("KK4PWJ", "N4AB");

    let engine = start_engine(&queue, &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    send(port(&engine), first.info_xml().as_bytes()).unwrap();
    send(port(&engine), second.info_xml().as_bytes()).unwrap();
    wait_until(&engine, "two waiting rows and a failed send", |s| {
        s.stream.len() == 2 && matches!(s.upstream, UpstreamStatus::Retrying { .. })
    })
    .await;
    engine.shutdown().await;

    server.set_fail_contacts(None);
    let engine = start_engine(&queue, &server.uri(), signed_in()).await;
    // The queued contacts come back as waiting rows, newest first, then go out.
    let restored = engine.snapshot();
    assert_eq!(restored.stream.len(), 2);
    assert_eq!(restored.stream[0].call, "N4AB");
    let snap = wait_rows_final(&engine, 2).await;
    assert!(snap.stream.iter().all(|r| r.status == RowStatus::Sent));
    assert_eq!(server.is_live(first.id.as_deref().unwrap()), Some(true));
    assert_eq!(server.is_live(second.id.as_deref().unwrap()), Some(true));
    // Sent in the order logged.
    let last = server.requests().last().unwrap().body.clone();
    assert_eq!(last["contacts"][0]["call"], "W1AW");
    assert_eq!(last["contacts"][1]["call"], "N4AB");
    engine.shutdown().await;
}

#[tokio::test]
async fn replace_updates_the_spot() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let mut c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_rows_final(&engine, 1).await;
    c.freq_10hz += 100;
    send(port(&engine), c.replace_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 2).await;
    assert_eq!(snap.stream[0].status, RowStatus::Updated);
    assert_eq!(
        snap.stream[0].status_text,
        "Edited in logger. Spot updated."
    );
    assert_eq!(snap.stream[1].status, RowStatus::Sent);
    let id = c.id.as_deref().unwrap();
    assert_eq!(server.is_live(id), Some(true));
    assert_eq!(server.stored_frequency(id), Some(14_026_000));
    engine.shutdown().await;
}

#[tokio::test]
async fn delete_removes_the_spot() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_rows_final(&engine, 1).await;
    send(port(&engine), c.delete_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 2).await;
    assert_eq!(snap.stream[0].status, RowStatus::Removed);
    assert_eq!(
        snap.stream[0].status_text,
        "Deleted in logger. Spot removed."
    );
    assert_eq!(server.is_live(c.id.as_deref().unwrap()), Some(false));
    let items = server.items();
    assert_eq!(items[1]["op"], "delete");
    assert!(items[1].get("mycall").is_none());
    engine.shutdown().await;
}

#[tokio::test]
async fn an_n1mm_edit_shows_one_updated_row() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let mut c = contact("KK4PWJ", "W1AW");
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_rows_final(&engine, 1).await;
    // N1MM sends an edit as contactdelete then contactreplace.
    send(port(&engine), c.delete_xml().as_bytes()).unwrap();
    c.freq_10hz += 100;
    send(port(&engine), c.replace_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 2).await;
    assert_eq!(
        snap.stream[0].kind,
        Some(relay_core::n1mm::ContactKind::Replace)
    );
    assert_eq!(snap.stream[0].status, RowStatus::Updated);
    assert_eq!(snap.stream[1].status, RowStatus::Sent);
    let id = c.id.as_deref().unwrap();
    assert_eq!(server.is_live(id), Some(true));
    assert_eq!(server.stored_frequency(id), Some(14_026_000));
    engine.shutdown().await;
}

#[tokio::test]
async fn a_refused_token_signs_out() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = signed_in();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), tokens.clone()).await;
    let mut events = engine.subscribe();
    wait_signed_in(&engine).await;
    server.revoke();
    send(
        port(&engine),
        contact("KK4PWJ", "W1AW").info_xml().as_bytes(),
    )
    .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await {
                Ok(Event::Auth(e @ AuthEvent::SignedOut { .. })) => return e,
                Ok(_) => {}
                Err(e) => panic!("bus closed: {e}"),
            }
        }
    })
    .await
    .expect("a signed-out event");
    assert_eq!(
        event,
        AuthEvent::SignedOut {
            reason: SignOutReason::Revoked
        }
    );
    assert_eq!(tokens.load().unwrap(), None);
    let snap = engine.snapshot();
    assert_eq!(snap.account, None);
    assert_eq!(snap.upstream, UpstreamStatus::SignedOut);
    // The contact stays queued for the next sign-in.
    assert_eq!(snap.stream[0].status, RowStatus::Waiting);
    engine.shutdown().await;
}

#[tokio::test]
async fn rate_limit_waits_for_retry_after() {
    let server = FakeLongLines::start().await;
    server.script(Scripted::RateLimited {
        retry_after_secs: 1,
    });
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    send(
        port(&engine),
        contact("KK4PWJ", "W1AW").info_xml().as_bytes(),
    )
    .unwrap();
    wait_until(&engine, "rate limited", |s| {
        matches!(s.upstream, UpstreamStatus::RateLimited { .. })
    })
    .await;
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let gap = requests[1].at - requests[0].at;
    assert!(gap >= Duration::from_millis(900), "resent after {gap:?}");
    assert_eq!(requests[0].body, requests[1].body);
    engine.shutdown().await;
}

#[tokio::test]
async fn test_mode_never_sends() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    engine.set_test_mode(true);
    assert_eq!(engine.snapshot().feed.status, FeedStatus::TestMode);
    let mut c = contact("KK4PWJ", "W1AW");
    let p = port(&engine);
    send(p, c.info_xml().as_bytes()).unwrap();
    c.freq_10hz += 100;
    send(p, c.replace_xml().as_bytes()).unwrap();
    send(p, contact("KK4PWJ", "N4AB").delete_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 3).await;
    for row in &snap.stream {
        assert_eq!(row.status, RowStatus::TestMode);
        assert_eq!(row.status_text, "Not sent. Test mode is on.");
    }
    // Checks still apply in test mode, so it shows what would happen.
    send(p, contact("K4XYZ", "W1AW").info_xml().as_bytes()).unwrap();
    wait_rows_final(&engine, 4).await;
    assert_eq!(
        status_of(&engine, 0),
        RowStatus::Mismatch {
            mycall: "K4XYZ".into()
        }
    );

    engine.set_test_mode(false);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(server.requests().is_empty(), "test-mode contacts were sent");
    engine.shutdown().await;
}

#[tokio::test]
async fn callsign_mismatch_is_ignored_until_the_call_matches() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    send(
        port(&engine),
        contact("K4XYZ", "W4ABC").info_xml().as_bytes(),
    )
    .unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status_text, "Ignored. Logged as K4XYZ.");
    assert_eq!(snap.feed.status, FeedStatus::IgnoringContacts);
    assert_eq!(snap.feed.label, "Ignoring contacts");
    assert_eq!(snap.feed.mismatch.as_deref(), Some("K4XYZ"));

    send(
        port(&engine),
        contact("W4/KK4PWJ/P", "N2DEF").info_xml().as_bytes(),
    )
    .unwrap();
    let snap = wait_rows_final(&engine, 2).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    assert_eq!(snap.feed.status, FeedStatus::Receiving);
    assert_eq!(snap.feed.mismatch, None);
    let items = server.items();
    assert_eq!(items.len(), 1);
    // The call goes out exactly as logged.
    assert_eq!(items[0]["mycall"], "W4/KK4PWJ/P");
    engine.shutdown().await;
}

#[tokio::test]
async fn packets_that_are_never_sent() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    assert_eq!(engine.snapshot().feed.status, FeedStatus::Waiting);
    let p = port(&engine);

    send(p, radioinfo_xml().as_bytes()).unwrap();
    wait_until(&engine, "receiving", |s| {
        s.feed.status == FeedStatus::Receiving
    })
    .await;
    assert!(
        engine.snapshot().stream.is_empty(),
        "radioinfo makes no row"
    );

    let old = FakeContact::new("KK4PWJ", "W1AW", Utc::now() - TimeDelta::minutes(45));
    send(p, old.info_xml().as_bytes()).unwrap();
    let ahead = FakeContact::new("KK4PWJ", "W1AW", Utc::now() + TimeDelta::minutes(10));
    send(p, ahead.info_xml().as_bytes()).unwrap();
    let mut forwarded = contact("KK4PWJ", "W1AW");
    forwarded.is_original = false;
    send(p, forwarded.info_xml().as_bytes()).unwrap();
    send(p, contact("", "W1AW").info_xml().as_bytes()).unwrap();
    send(p, MALFORMED.as_bytes()).unwrap();
    let mut no_id_delete = contact("KK4PWJ", "W1AW");
    no_id_delete.id = None;
    send(p, no_id_delete.delete_xml().as_bytes()).unwrap();

    let snap = wait_rows_final(&engine, 6).await;
    let texts: Vec<String> = snap
        .stream
        .iter()
        .rev()
        .map(|r| r.status_text.clone())
        .collect();
    assert_eq!(
        texts,
        [
            "Not sent. Logged more than 30 minutes ago.",
            "Not sent. Logged more than 5 minutes ahead of this computer's clock.",
            "Ignored. Forwarded by another N1MM station.",
            "Ignored. No station callsign in the logger.",
            "Couldn't read packet",
            "Couldn't send: The logger didn't say which contact was deleted.",
        ]
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.requests().is_empty());
    engine.shutdown().await;
}

#[tokio::test]
async fn an_old_delete_is_still_sent() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let old = FakeContact::new("KK4PWJ", "W1AW", Utc::now() - TimeDelta::hours(2));
    send(port(&engine), old.delete_xml().as_bytes()).unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Removed);
    assert_eq!(server.items()[0]["op"], "delete");
    engine.shutdown().await;
}

#[tokio::test]
async fn sign_in_and_sign_out() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = std::sync::Arc::new(MemoryTokenStore::new());
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), tokens.clone()).await;
    assert_eq!(engine.snapshot().upstream, UpstreamStatus::SignedOut);

    // Signed out, contacts wait (the account is unknown, so no callsign check yet).
    send(
        port(&engine),
        contact("KK4PWJ", "W1AW").info_xml().as_bytes(),
    )
    .unwrap();
    wait_until(&engine, "a waiting row", |s| s.stream.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(server.requests().is_empty());

    let account = engine.sign_in(Token::new(TOKEN)).await.unwrap();
    assert_eq!(account.callsign, CALLSIGN);
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);

    let mut events = engine.subscribe();
    engine.sign_out().unwrap();
    assert_eq!(tokens.load().unwrap(), None);
    assert_eq!(engine.snapshot().account, None);
    let mut saw = false;
    while let Ok(event) = events.try_recv() {
        if event
            == Event::Auth(AuthEvent::SignedOut {
                reason: SignOutReason::User,
            })
        {
            saw = true;
        }
    }
    assert!(saw, "no signed-out event");
    engine.shutdown().await;
}

#[tokio::test]
async fn queued_contacts_from_another_station_are_not_sent_after_sign_in() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let tokens = std::sync::Arc::new(MemoryTokenStore::new());
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), tokens).await;
    send(
        port(&engine),
        contact("K4XYZ", "W1AW").info_xml().as_bytes(),
    )
    .unwrap();
    wait_until(&engine, "a waiting row", |s| s.stream.len() == 1).await;
    engine.sign_in(Token::new(TOKEN)).await.unwrap();
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(
        snap.stream[0].status,
        RowStatus::Mismatch {
            mycall: "K4XYZ".into()
        }
    );
    assert!(server.requests().is_empty());
    engine.shutdown().await;
}

#[tokio::test]
async fn a_held_port_shows_port_in_use_until_changed() {
    let holder = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let held = holder.local_addr().unwrap().port();
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = relay_core::engine::EngineConfig::new(
        held,
        dir.path().join("q.sqlite3"),
        relay_core::upstream::client::ClientConfig::new(server.uri(), "0.1.0-test"),
        signed_in(),
    );
    config.source_filter =
        std::sync::Arc::new(relay_core::source_filter::SourceFilter::with_addresses([]));
    let engine = relay_core::engine::Engine::start(config).await.unwrap();
    let feed = engine.snapshot().feed;
    assert_eq!(feed.status, FeedStatus::PortInUse);
    assert_eq!(feed.label, "Port in use");
    assert_eq!(feed.port, held);
    let status = engine.set_port(0);
    assert!(matches!(
        status,
        relay_core::listeners::ListenerStatus::Listening { .. }
    ));
    assert_eq!(engine.snapshot().feed.status, FeedStatus::Waiting);
    engine.shutdown().await;
}

#[tokio::test]
async fn fake_logger_binary_drives_the_engine() {
    let server = FakeLongLines::start().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    wait_signed_in(&engine).await;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fake-logger"))
        .args([
            "--port",
            &port(&engine).to_string(),
            "--mycall",
            "KK4PWJ",
            "--call",
            "W1AW",
            "--sequence",
            "info,replace,delete",
            "--delay-ms",
            "200",
            "--malformed",
        ])
        .output()
        .expect("fake-logger runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snap = wait_rows_final(&engine, 4).await;
    let texts: Vec<String> = snap
        .stream
        .iter()
        .rev()
        .map(|r| r.status_text.clone())
        .collect();
    assert_eq!(
        texts,
        [
            "Sent as a spot",
            "Edited in logger. Spot updated.",
            "Deleted in logger. Spot removed.",
            "Couldn't read packet",
        ]
    );
    engine.shutdown().await;
}

/// A `GET /me` that answers after `me_delay`, and a `POST /contacts` that stores everything.
async fn slow_me_server(me_delay: Duration) -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"callsign": CALLSIGN, "email": EMAIL}))
                .set_delay(me_delay),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/contacts"))
        .respond_with(|request: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let results: Vec<_> = body["contacts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| serde_json::json!({"external_id": c["external_id"], "status": "created"}))
                .collect();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": results}))
        })
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn an_upsert_that_ages_out_during_get_me_is_not_sent() {
    // Logged 29:58 ago: fresh when it arrives, past 30 minutes once the slow GET /me answers.
    let server = slow_me_server(Duration::from_secs(4)).await;
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), signed_in()).await;
    let logged = Utc::now() - TimeDelta::minutes(30) + TimeDelta::seconds(2);
    let c = FakeContact::new("KK4PWJ", "W1AW", logged);
    send(port(&engine), c.info_xml().as_bytes()).unwrap();
    wait_until(&engine, "the contact queued", |s| {
        s.stream.first().map(|r| &r.status) == Some(&RowStatus::Waiting)
    })
    .await;
    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::TooOld);
    let sent = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/contacts")
        .count();
    assert_eq!(sent, 0, "an upsert past 30 minutes must never be sent");
    engine.shutdown().await;
}

#[tokio::test]
async fn a_late_401_for_an_old_token_keeps_the_new_one() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};
    const OLD: &str = "ll_relay_oldoldoldoldoldoldoldoldoldoldoldoldoldoldoldoldold";
    const NEW: &str = "ll_relay_newnewnewnewnewnewnewnewnewnewnewnewnewnewnewnewnew";
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"callsign": CALLSIGN, "email": EMAIL})),
        )
        .mount(&server)
        .await;
    // The old token was revoked in the web app; the 401 is slow to come back.
    Mock::given(method("POST"))
        .and(path("/contacts"))
        .and(header("authorization", format!("Bearer {OLD}").as_str()))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(serde_json::json!({"error": "bad_token"}))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/contacts"))
        .and(header("authorization", format!("Bearer {NEW}").as_str()))
        .respond_with(|request: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let id = body["contacts"][0]["external_id"].clone();
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"results": [{"external_id": id, "status": "created"}]}),
            )
        })
        .mount(&server)
        .await;

    let tokens = std::sync::Arc::new(MemoryTokenStore::with_token(Token::new(OLD)));
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine(&dir.path().join("q.sqlite3"), &server.uri(), tokens.clone()).await;
    wait_signed_in(&engine).await;
    send(
        port(&engine),
        contact("KK4PWJ", "W1AW").info_xml().as_bytes(),
    )
    .unwrap();
    // The old token's request is out; the user signs in again before its 401 lands.
    tokio::time::sleep(Duration::from_millis(500)).await;
    engine.sign_in(Token::new(NEW)).await.unwrap();

    let snap = wait_rows_final(&engine, 1).await;
    assert_eq!(snap.stream[0].status, RowStatus::Sent);
    assert_eq!(tokens.load().unwrap(), Some(Token::new(NEW)));
    assert_eq!(engine.snapshot().upstream, UpstreamStatus::Connected);
    engine.shutdown().await;
}
