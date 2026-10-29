//! End to end against a real Long Lines `relay-api` (for example the server repo's local
//! Supabase stack), so the client and the server are checked against each other, not only
//! against the contract. Ignored by default; it needs a paired token:
//!
//! ```sh
//! LONGLINES_RELAY_E2E_BASE=http://127.0.0.1:54321/functions/v1/relay-api/v2 \
//! LONGLINES_RELAY_E2E_TOKEN=ll_relay_... \
//! cargo test -p relay-core --test e2e -- --ignored --nocapture
//! ```
//!
//! It sends fake-logger packets through the engine: a contact and its edit (`contactreplace`
//! with another frequency), a contact and its delete, one logged under another callsign and one
//! logged 40 minutes ago. It prints the external IDs and calls so the database can be checked:
//! the edited contact has one live spot and one retracted, the deleted one only a retracted spot,
//! and the last two never reach the server.

mod support;

use std::time::{Duration, Instant};

use chrono::{TimeDelta, Utc};
use relay_core::engine::Engine;
use relay_core::fake_logger::{send, FakeContact};
use relay_core::n1mm::ContactKind;
use relay_core::state::StreamRow;
use relay_core::status::RowStatus;
use relay_core::upstream::token_store::{MemoryTokenStore, Token};
use std::sync::Arc;

use support::{port, start_engine};

const BASE_ENV: &str = "LONGLINES_RELAY_E2E_BASE";
const TOKEN_ENV: &str = "LONGLINES_RELAY_E2E_TOKEN";
/// The first request can start the Edge Function cold.
const WAIT: Duration = Duration::from_secs(60);

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test"))
}

/// Waits for a row of `kind` with `external_id` whose status is final, and returns it.
async fn final_row(engine: &Engine, external_id: &str, kind: ContactKind) -> StreamRow {
    let deadline = Instant::now() + WAIT;
    loop {
        let found = engine.snapshot().stream.into_iter().find(|r| {
            r.kind == Some(kind)
                && r.external_id.as_deref() == Some(external_id)
                && r.status.is_final()
        });
        if let Some(row) = found {
            return row;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {kind:?} {external_id}; rows: {:#?}",
            engine.snapshot().stream
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Waits for a final row for `call` that never got an external ID (it was never queued).
async fn final_unqueued_row(engine: &Engine, call: &str) -> StreamRow {
    let deadline = Instant::now() + WAIT;
    loop {
        let found = engine
            .snapshot()
            .stream
            .into_iter()
            .find(|r| r.call == call && r.status.is_final());
        if let Some(row) = found {
            return row;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {call}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn packet(port: u16, xml: &str) {
    send(port, xml.as_bytes()).expect("send a packet");
}

#[tokio::test]
#[ignore = "needs a running relay-api and a paired token (LONGLINES_RELAY_E2E_BASE / _TOKEN)"]
async fn contacts_reach_a_real_server() {
    let base = env(BASE_ENV);
    let token = env(TOKEN_ENV);
    let dir = tempfile::tempdir().unwrap();
    let tokens = Arc::new(MemoryTokenStore::with_token(Token::new(token)));
    let engine = start_engine(&dir.path().join("queue.sqlite3"), &base, tokens).await;

    // GET /me first: the station callsign check needs the account.
    let deadline = Instant::now() + WAIT;
    let account = loop {
        if let Some(account) = engine.snapshot().account {
            break account;
        }
        assert!(Instant::now() < deadline, "GET /me never answered");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let mycall = format!("{}/P", account.callsign);
    let port = port(&engine);
    let now = Utc::now() - TimeDelta::minutes(1);
    // Whole seconds, as the packet carries them.
    let now = now - TimeDelta::nanoseconds(i64::from(now.timestamp_subsec_nanos()));

    // A: logged, then edited to another frequency (an N1MM contactreplace with the same ID).
    let mut edited = FakeContact::new(&mycall, "W1AW", now);
    let edited_id = edited.id.clone().unwrap();
    packet(port, &edited.info_xml());
    let row = final_row(&engine, &edited_id, ContactKind::Info).await;
    assert_eq!(row.status, RowStatus::Sent, "{row:#?}");
    edited.freq_10hz = 1_403_000;
    packet(port, &edited.replace_xml());
    let row = final_row(&engine, &edited_id, ContactKind::Replace).await;
    assert_eq!(row.status, RowStatus::Updated, "{row:#?}");

    // B: logged, then deleted.
    let deleted = FakeContact::new(&mycall, "N4AB", now);
    let deleted_id = deleted.id.clone().unwrap();
    packet(port, &deleted.info_xml());
    let row = final_row(&engine, &deleted_id, ContactKind::Info).await;
    assert_eq!(row.status, RowStatus::Sent, "{row:#?}");
    packet(port, &deleted.delete_xml());
    let row = final_row(&engine, &deleted_id, ContactKind::Delete).await;
    assert_eq!(row.status, RowStatus::Removed, "{row:#?}");

    // C: logged under another station's callsign. Never queued, never sent.
    let other = FakeContact::new("K4XYZ", "VE3ABC", now);
    packet(port, &other.info_xml());
    let row = final_unqueued_row(&engine, "VE3ABC").await;
    assert_eq!(
        row.status,
        RowStatus::Mismatch {
            mycall: "K4XYZ".into()
        }
    );

    // D: logged 40 minutes ago. Never queued, never sent.
    let stale = FakeContact::new(&mycall, "G4ABC", now - TimeDelta::minutes(40));
    packet(port, &stale.info_xml());
    let row = final_unqueued_row(&engine, "G4ABC").await;
    assert_eq!(row.status, RowStatus::TooOld);

    println!("edited  W1AW   external_id {edited_id}");
    println!("deleted N4AB   external_id {deleted_id}");
    println!("mismatch VE3ABC (mycall K4XYZ) external_id {:?}", other.id);
    println!("stale   G4ABC  external_id {:?}", stale.id);
    engine.shutdown().await;
}
