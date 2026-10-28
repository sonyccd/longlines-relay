//! Parser tests over the packet fixtures (see `fixtures/README.md` for where each came from).

use chrono::{TimeZone, Utc};
use relay_core::bands::band_for_hz;
use relay_core::n1mm::{parse, ContactKind, LoggedContact, Packet, ParseError};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn contact(name: &str) -> LoggedContact {
    match parse(&fixture(name)) {
        Ok(Packet::Contact(c)) => c,
        other => panic!("{name}: expected a contact, got {other:?}"),
    }
}

#[test]
fn n1mm_contactinfo() {
    let c = contact("n1mm-contactinfo.xml");
    assert_eq!(c.kind, ContactKind::Info);
    assert_eq!(c.app.as_deref(), Some("N1MM"));
    assert_eq!(c.mycall, "W2XYZ");
    assert_eq!(c.call, "W1AW");
    // 352519 in 10 Hz units is 3525.19 kHz.
    assert_eq!(c.frequency_hz, Some(3_525_190));
    assert_eq!(band_for_hz(c.frequency_hz.unwrap()), Some("80m"));
    assert_eq!(c.mode, "CW");
    assert_eq!(
        c.timestamp,
        Some(Utc.with_ymd_and_hms(2020, 1, 17, 16, 43, 38).unwrap())
    );
    assert_eq!(c.id.as_deref(), Some("f9ffac4fcd3e479ca86e137df1338531"));
    assert!(c.is_original);
    assert_eq!(c.station_name.as_deref(), Some("CONTEST-PC"));
}

#[test]
fn n1mm_forwarded_contact_is_not_original() {
    let c = contact("n1mm-contactinfo-forwarded.xml");
    assert_eq!(c.kind, ContactKind::Info);
    assert!(!c.is_original);
}

#[test]
fn n1mm_contactreplace() {
    let c = contact("n1mm-contactreplace.xml");
    assert_eq!(c.kind, ContactKind::Replace);
    assert_eq!(c.id.as_deref(), Some("f9ffac4fcd3e479ca86e137df1338531"));
    assert_eq!(c.frequency_hz, Some(3_526_190));
    assert_eq!(c.call, "W1AW");
    assert!(c.is_original);
}

#[test]
fn n1mm_contactdelete() {
    let c = contact("n1mm-contactdelete.xml");
    assert_eq!(c.kind, ContactKind::Delete);
    assert_eq!(c.id.as_deref(), Some("f9ffac4fcd3e479ca86e137df1338531"));
    assert_eq!(c.call, "W1AW");
    assert_eq!(
        c.timestamp,
        Some(Utc.with_ymd_and_hms(2020, 1, 17, 16, 43, 38).unwrap())
    );
    // A delete carries no station call, frequency or mode.
    assert_eq!(c.mycall, "");
    assert_eq!(c.frequency_hz, None);
    assert_eq!(c.mode, "");
    assert!(c.is_original);
}

#[test]
fn n1mm_radioinfo_is_ignored() {
    assert_eq!(
        parse(&fixture("n1mm-radioinfo.xml")),
        Ok(Packet::Other("RadioInfo".into()))
    );
}

#[test]
fn dxlog_n1mm_format() {
    let c = contact("dxlog-n1mm-contactinfo.xml");
    assert_eq!(c.kind, ContactKind::Info);
    assert_eq!(c.app.as_deref(), Some("N1MM"));
    assert_eq!(c.mycall, "KK4PWJ");
    // The comma-decimal band element is ignored; the band comes from the frequency.
    assert_eq!(c.frequency_hz, Some(14_025_000));
    assert_eq!(band_for_hz(c.frequency_hz.unwrap()), Some("20m"));
    assert_eq!(c.id.as_deref(), Some("3b7c2e0d9a5f4c1e8d6b2a7f0e9c4d13"));
}

#[test]
fn not1mm() {
    let c = contact("not1mm-contactinfo.xml");
    assert_eq!(c.kind, ContactKind::Info);
    assert_eq!(c.app.as_deref(), Some("NOT1MM"));
    // Not1MM puts the operator in mycall.
    assert_eq!(c.mycall, "KK4PWJ");
    assert_eq!(c.call, "W1AW");
    assert_eq!(c.frequency_hz, Some(14_025_000));
    assert_eq!(
        c.timestamp,
        Some(Utc.with_ymd_and_hms(2026, 10, 28, 21, 14, 0).unwrap())
    );
    assert_eq!(c.id.as_deref(), Some("6d0f8e2a91b34c7d8e5f0a1b2c3d4e5f"));
    assert!(c.is_original);
}

#[test]
fn malformed_packets_are_errors() {
    assert_eq!(
        parse(&fixture("malformed-truncated.xml")),
        Err(ParseError::Truncated)
    );
    assert!(matches!(
        parse(&fixture("malformed-unclosed.xml")),
        Err(ParseError::Xml(_))
    ));
}
