//! Parser for N1MM Logger+ "external UDP broadcast" packets (and loggers that send the same XML).
//!
//! Tolerant by design: unknown, missing and extra elements are fine, element names are matched
//! without regard to case, and the logger's `band` element is ignored (the band comes from the
//! frequency). A packet that is not well-formed XML is a [`ParseError`], never a panic.
//!
//! Logger facts this follows (see `docs/loggers.md`): `rxfreq` / `txfreq` are in units of 10 Hz;
//! `timestamp` is `YYYY-MM-DD HH:MM:SS` in UTC; `ID` is the per-contact GUID; `IsOriginal` is
//! `False` on a contact another networked N1MM station is forwarding. The worked station's US
//! state and grid square come from [`crate::location`].

use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use serde::Serialize;

use crate::location;

/// The highest frequency the server accepts, in Hz.
pub const MAX_FREQUENCY_HZ: u64 = 300_000_000_000;

/// What kind of contact packet arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactKind {
    /// `contactinfo`: a new contact.
    Info,
    /// `contactreplace`: an edited contact.
    Replace,
    /// `contactdelete`: a deleted contact.
    Delete,
}

/// One contact as the logger sent it, trimmed. Callsigns and the mode are upper-cased.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedContact {
    pub kind: ContactKind,
    /// The logger's `app` value (`N1MM`, `NOT1MM`, ...).
    pub app: Option<String>,
    /// The station callsign as logged; empty when the logger sent none.
    pub mycall: String,
    /// The worked station; empty when missing.
    pub call: String,
    /// `rxfreq`, else `txfreq`, times 10. `None` when both are missing, zero or unreadable.
    pub frequency_hz: Option<u64>,
    /// Empty when missing.
    pub mode: String,
    /// `None` when missing or unreadable.
    pub timestamp: Option<DateTime<Utc>>,
    /// The logger's per-contact ID, trimmed; `None` when missing or empty.
    pub id: Option<String>,
    /// False only when the packet says `IsOriginal` false: a contact forwarded by another station.
    pub is_original: bool,
    /// The sending computer's name (N1MM `StationName`).
    pub station_name: Option<String>,
    /// The worked station's US state or DC postal code, when `state`, `section`, `qth` or
    /// `exchange1` is exactly one and no two of them name different states.
    pub state: Option<String>,
    /// The worked station's Maidenhead locator (`gridsquare`), upper-cased, when it is a valid
    /// 4, 6 or 8 character one.
    pub gridsquare: Option<String>,
}

/// A parsed packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    Contact(LoggedContact),
    /// Any other well-formed packet (`RadioInfo`, `spot`, ...), by its root element name.
    Other(String),
}

/// Why a packet could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("the packet is empty")]
    Empty,
    #[error("the packet has no root element")]
    NoRoot,
    #[error("the packet ends before its root element closes")]
    Truncated,
    #[error("the packet is not well-formed XML: {0}")]
    Xml(String),
}

/// Parses one datagram.
pub fn parse(bytes: &[u8]) -> Result<Packet, ParseError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let (root, fields) = read_fields(text)?;
    let kind = match root.to_ascii_lowercase().as_str() {
        "contactinfo" => ContactKind::Info,
        "contactreplace" => ContactKind::Replace,
        "contactdelete" => ContactKind::Delete,
        _ => return Ok(Packet::Other(root)),
    };
    Ok(Packet::Contact(contact_from(kind, &fields)))
}

/// Reads the root element's name and the text of its direct children, keyed by lower-case name.
fn read_fields(text: &str) -> Result<(String, HashMap<String, String>), ParseError> {
    let mut reader = Reader::from_str(text);
    let mut root: Option<String> = None;
    let mut fields = HashMap::new();
    let mut depth = 0usize;
    let mut current: Option<(String, String)> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| ParseError::Xml(e.to_string()))?;
        match event {
            XmlEvent::Start(e) => {
                let name = e.local_name().as_ref().to_string();
                depth += 1;
                match depth {
                    1 if root.is_none() => root = Some(name),
                    1 => return Err(ParseError::Xml("more than one root element".into())),
                    2 => current = Some((name.to_ascii_lowercase(), String::new())),
                    _ => {}
                }
            }
            XmlEvent::Empty(e) => {
                let name = e.local_name().as_ref().to_string();
                match depth {
                    0 if root.is_none() => return Ok((name, fields)),
                    0 => return Err(ParseError::Xml("more than one root element".into())),
                    1 => {
                        fields.entry(name.to_ascii_lowercase()).or_default();
                    }
                    _ => {}
                }
            }
            XmlEvent::End(_) => {
                if depth == 2 {
                    if let Some((name, value)) = current.take() {
                        fields.insert(name, value);
                    }
                }
                depth = depth.saturating_sub(1);
            }
            XmlEvent::Text(t) if depth == 2 => {
                if let Some((_, value)) = current.as_mut() {
                    value.push_str(&t.xml10_content());
                }
            }
            XmlEvent::CData(t) if depth == 2 => {
                if let Some((_, value)) = current.as_mut() {
                    value.push_str(&t.into_inner());
                }
            }
            XmlEvent::GeneralRef(r) if depth == 2 => {
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => Some(c.to_string()),
                    Ok(None) => quick_xml::escape::resolve_predefined_entity(&r).map(String::from),
                    Err(e) => return Err(ParseError::Xml(e.to_string())),
                };
                let Some(resolved) = resolved else {
                    return Err(ParseError::Xml(format!("unknown entity &{};", &*r)));
                };
                if let Some((_, value)) = current.as_mut() {
                    value.push_str(&resolved);
                }
            }
            XmlEvent::Eof => {
                return match (root, depth) {
                    (None, _) => Err(ParseError::NoRoot),
                    (Some(_), d) if d > 0 => Err(ParseError::Truncated),
                    (Some(r), _) => Ok((r, fields)),
                };
            }
            _ => {}
        }
    }
}

fn contact_from(kind: ContactKind, fields: &HashMap<String, String>) -> LoggedContact {
    let get = |name: &str| {
        fields
            .get(name)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let upper = |name: &str| get(name).map(|v| v.to_uppercase()).unwrap_or_default();
    LoggedContact {
        kind,
        app: get("app"),
        mycall: upper("mycall"),
        call: upper("call"),
        frequency_hz: frequency(get("rxfreq").as_deref())
            .or_else(|| frequency(get("txfreq").as_deref())),
        mode: upper("mode"),
        timestamp: get("timestamp").as_deref().and_then(parse_timestamp),
        id: get("id"),
        is_original: get("isoriginal").as_deref().is_none_or(|v| !is_false(v)),
        station_name: get("stationname"),
        state: location::us_state_of(
            STATE_FIELDS
                .iter()
                .filter_map(|name| fields.get(*name).map(String::as_str)),
        )
        .map(str::to_string),
        gridsquare: get("gridsquare").as_deref().and_then(location::gridsquare),
    }
}

/// The contact fields that can hold the worked station's state, lower-case. N1MM has no `state`
/// element; other loggers that send its format may. Which of `section`, `qth` and `exchange1`
/// N1MM fills with the received location depends on the contest (`section` in Sweepstakes and
/// Field Day), so each is read and only an exact state code counts.
const STATE_FIELDS: [&str; 4] = ["state", "section", "qth", "exchange1"];

/// A frequency in 10 Hz units, as N1MM and Not1MM send it, converted to Hz.
fn frequency(value: Option<&str>) -> Option<u64> {
    let value = value?;
    let units: u64 = match value.parse::<u64>() {
        Ok(u) => u,
        Err(_) => {
            let f: f64 = value.parse().ok()?;
            if !f.is_finite() || f < 0.0 {
                return None;
            }
            f.round() as u64
        }
    };
    let hz = units.checked_mul(10)?;
    (hz > 0 && hz <= MAX_FREQUENCY_HZ).then_some(hz)
}

/// `YYYY-MM-DD HH:MM:SS` (UTC). A `T` separator and a fraction of a second are tolerated; the
/// fraction is dropped because the contract's timestamps are whole seconds.
pub fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim().trim_end_matches('Z');
    let value = value.split('.').next().unwrap_or(value);
    [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
    ]
    .iter()
    .find_map(|f| NaiveDateTime::parse_from_str(value, f).ok())
    .map(|naive| naive.and_utc())
}

fn is_false(value: &str) -> bool {
    value.eq_ignore_ascii_case("false") || value == "0"
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn frequency_is_in_ten_hertz_units() {
        assert_eq!(frequency(Some("352519")), Some(3_525_190));
        assert_eq!(frequency(Some("1402500")), Some(14_025_000));
        assert_eq!(frequency(Some("0")), None);
        assert_eq!(frequency(Some("abc")), None);
        assert_eq!(frequency(Some("-5")), None);
        assert_eq!(frequency(Some("1402500.4")), Some(14_025_000));
        assert_eq!(frequency(None), None);
        assert_eq!(frequency(Some("30000000001")), None);
    }

    #[test]
    fn timestamps_are_utc() {
        let want = Utc.with_ymd_and_hms(2020, 1, 17, 16, 43, 38).unwrap();
        assert_eq!(parse_timestamp("2020-01-17 16:43:38"), Some(want));
        assert_eq!(parse_timestamp("2020-01-17T16:43:38Z"), Some(want));
        assert_eq!(parse_timestamp("2020-01-17 16:43:38.25"), Some(want));
        assert_eq!(parse_timestamp("17/01/2020 16:43"), None);
        assert_eq!(parse_timestamp(""), None);
    }

    #[test]
    fn is_original_values() {
        let parse_flag = |v: &str| {
            let xml =
                format!("<contactinfo><call>W1AW</call><IsOriginal>{v}</IsOriginal></contactinfo>");
            match parse(xml.as_bytes()).unwrap() {
                Packet::Contact(c) => c.is_original,
                Packet::Other(_) => panic!("not a contact"),
            }
        };
        assert!(parse_flag("True"));
        assert!(parse_flag("true"));
        assert!(parse_flag("1"));
        assert!(parse_flag(""));
        assert!(!parse_flag("False"));
        assert!(!parse_flag("FALSE"));
        assert!(!parse_flag("0"));
    }

    #[test]
    fn rxfreq_falls_back_to_txfreq() {
        let xml = b"<contactinfo><rxfreq>0</rxfreq><txfreq>702500</txfreq></contactinfo>";
        let Packet::Contact(c) = parse(xml).unwrap() else {
            panic!()
        };
        assert_eq!(c.frequency_hz, Some(7_025_000));
    }

    #[test]
    fn entities_and_cdata_are_decoded() {
        let xml = b"<contactinfo><call>W1&amp;AW</call><mode><![CDATA[cw]]></mode><app>&#78;1MM</app></contactinfo>";
        let Packet::Contact(c) = parse(xml).unwrap() else {
            panic!()
        };
        assert_eq!(c.call, "W1&AW");
        assert_eq!(c.mode, "CW");
        assert_eq!(c.app.as_deref(), Some("N1MM"));
    }

    #[test]
    fn errors_never_panic() {
        assert_eq!(parse(b""), Err(ParseError::Empty));
        assert_eq!(parse(b"   \n"), Err(ParseError::Empty));
        assert_eq!(parse(b"just text"), Err(ParseError::NoRoot));
        assert_eq!(
            parse(b"<contactinfo><call>W1AW</call>"),
            Err(ParseError::Truncated)
        );
        assert!(matches!(
            parse(b"<contactinfo><call>W1AW</mode></contactinfo>"),
            Err(ParseError::Xml(_))
        ));
        assert!(matches!(parse(b"<a></a><b></b>"), Err(ParseError::Xml(_))));
        assert!(matches!(
            parse(b"<contactinfo><call>&bogus;</call></contactinfo>"),
            Err(ParseError::Xml(_))
        ));
        assert!(parse(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn self_closing_root_is_another_packet() {
        assert_eq!(
            parse(b"<RadioInfo/>"),
            Ok(Packet::Other("RadioInfo".into()))
        );
    }
}
