//! Pins the JSON the window reads. The page's tests load the same file
//! (`src/test/fixtures/snapshot.json`), so a change to the shape breaks both sides.

use chrono::{TimeZone, Utc};
use relay_core::listeners::ListenerStatus;
use relay_core::n1mm::ContactKind;
use relay_core::state::{Account, RelayState, StreamRow};
use relay_core::status::RowStatus;
use relay_core::upstream::auth::PairingView;
use serde_json::Value;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../src/test/fixtures/snapshot.json"
);

fn snapshot() -> Value {
    let at = |h, m, s| Utc.with_ymd_and_hms(2026, 10, 28, h, m, s).unwrap();
    let state = RelayState::new();
    state.set_listener(ListenerStatus::Listening { port: 12060 }, at(14, 0, 0));
    state.set_account(Some(Account {
        callsign: "KK4PWJ".into(),
        email: "op@example.com".into(),
    }));
    state.push_row(StreamRow::for_contact(
        ContactKind::Info,
        "K8QRS",
        Some(21_025_000),
        "CW",
        Some(at(14, 9, 0)),
        at(14, 9, 3),
        RowStatus::Mismatch {
            mycall: "K4XYZ".into(),
        },
    ));
    state.push_row(StreamRow::unreadable(at(14, 18, 0)));
    state.push_row(StreamRow::for_contact(
        ContactKind::Info,
        "W4ABC",
        Some(7_025_000),
        "CW",
        Some(at(14, 22, 0)),
        at(14, 22, 2),
        RowStatus::Sent,
    ));
    state.record_packet(at(14, 22, 2));
    state.set_pairing(PairingView::Idle);
    serde_json::to_value(state.snapshot(0)).unwrap()
}

#[test]
fn the_window_reads_this_shape() {
    let actual = snapshot();
    if std::env::var_os("UPDATE_UI_FIXTURE").is_some() {
        let text = serde_json::to_string_pretty(&actual).unwrap() + "\n";
        std::fs::write(FIXTURE, text).unwrap();
    }
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(FIXTURE)
            .expect("the fixture exists (UPDATE_UI_FIXTURE=1 writes it)"),
    )
    .unwrap();
    assert_eq!(
        actual, expected,
        "set UPDATE_UI_FIXTURE=1 to rewrite it, then fix the page"
    );
}

#[test]
fn pairing_views_have_the_shapes_the_window_reads() {
    let waiting = PairingView::Waiting {
        user_code: "KQ7M-X4PD".into(),
        verification_uri: "https://longlines.io/relay".into(),
        verification_uri_complete: "https://longlines.io/relay/approve?code=KQ7M-X4PD".into(),
        expires_at: Utc.with_ymd_and_hms(2026, 10, 28, 14, 30, 0).unwrap(),
    };
    assert_eq!(
        serde_json::to_value(waiting).unwrap(),
        serde_json::json!({
            "state": "waiting",
            "user_code": "KQ7M-X4PD",
            "verification_uri": "https://longlines.io/relay",
            "verification_uri_complete": "https://longlines.io/relay/approve?code=KQ7M-X4PD",
            "expires_at": "2026-10-28T14:30:00Z",
        })
    );
}
