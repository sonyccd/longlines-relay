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
    // Its gridsquare, exchange1, section and qth elements are empty.
    assert_eq!(c.state, None);
    assert_eq!(c.gridsquare, None);
}

#[test]
fn n1mm_sweepstakes_section_is_the_state() {
    let c = contact("n1mm-contactinfo-sweepstakes.xml");
    assert_eq!(c.call, "K4ABC");
    assert_eq!(c.state.as_deref(), Some("NC"));
    assert_eq!(c.gridsquare, None);
}

#[test]
fn n1mm_vhf_contest_grid() {
    let c = contact("n1mm-contactinfo-vhf.xml");
    assert_eq!(c.call, "N4AB");
    assert_eq!(c.frequency_hz, Some(50_125_000));
    // Upper-cased; the subsquare letters arrive in lower case.
    assert_eq!(c.gridsquare.as_deref(), Some("FM05PU"));
    assert_eq!(c.state, None);
}

#[test]
fn n1mm_qso_party_exchange_is_the_state() {
    let c = contact("n1mm-contactinfo-qso-party.xml");
    assert_eq!(c.call, "W4DOG");
    assert_eq!(c.state.as_deref(), Some("VA"));
}

/// The sweepstakes fixture with one element's value swapped.
fn sweepstakes_with(element: &str, value: &str) -> LoggedContact {
    let xml = String::from_utf8(fixture("n1mm-contactinfo-sweepstakes.xml")).unwrap();
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let start = xml.find(&open).unwrap() + open.len();
    let end = xml[start..].find(&close).unwrap() + start;
    let xml = format!("{}{value}{}", &xml[..start], &xml[end..]);
    match parse(xml.as_bytes()) {
        Ok(Packet::Contact(c)) => c,
        other => panic!("expected a contact, got {other:?}"),
    }
}

#[test]
fn only_an_exact_state_code_counts() {
    // An ARRL section that is not a state's postal code (part of New York) is never guessed at.
    assert_eq!(sweepstakes_with("section", "ENY").state, None);
    assert_eq!(
        sweepstakes_with("section", "nc").state.as_deref(),
        Some("NC")
    );
    assert_eq!(
        sweepstakes_with("section", "DC").state.as_deref(),
        Some("DC")
    );
    assert_eq!(sweepstakes_with("section", "ON").state, None);
    assert_eq!(sweepstakes_with("section", "PR").state, None);
    // A county in the exchange beside a section is not a second state.
    let c = sweepstakes_with("exchange1", "WAKE");
    assert_eq!(c.state.as_deref(), Some("NC"));
    // Two different states in two fields: neither is sent.
    assert_eq!(sweepstakes_with("qth", "VA").state, None);
    assert_eq!(sweepstakes_with("qth", "NC").state.as_deref(), Some("NC"));
}

#[test]
fn a_state_element_is_read_when_a_logger_sends_one() {
    let xml = b"<contactinfo><call>K4ABC</call><State>ga</State></contactinfo>";
    let Ok(Packet::Contact(c)) = parse(xml) else {
        panic!()
    };
    assert_eq!(c.state.as_deref(), Some("GA"));
}

#[test]
fn an_unusable_grid_is_left_out() {
    assert_eq!(
        sweepstakes_with("gridsquare", "FM05").gridsquare.as_deref(),
        Some("FM05")
    );
    assert_eq!(
        sweepstakes_with("gridsquare", "FM05pu47")
            .gridsquare
            .as_deref(),
        Some("FM05PU47")
    );
    for bad in ["FM0", "FM05p", "ZZ99", "FM05PU47XX", "NC"] {
        assert_eq!(
            sweepstakes_with("gridsquare", bad).gridsquare,
            None,
            "{bad}"
        );
    }
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
