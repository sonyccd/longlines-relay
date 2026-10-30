//! Builds and sends N1MM-format packets, for the `fake-logger` dev tool and the integration
//! tests. The packets follow the N1MM Logger+ appendix's field list.

use std::io;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use quick_xml::escape::escape;
use sha2::{Digest, Sha256};

/// A truncated `contactinfo`: the parser must report it, not crash.
pub const MALFORMED: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<contactinfo>\n\t<app>N1MM</app>\n\t<call>W1AW</call>\n\t<mode>CW";

/// One contact as a logger would send it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeContact {
    pub app: String,
    pub mycall: String,
    pub call: String,
    /// `rxfreq` / `txfreq` in 10 Hz units, as N1MM sends them (1402500 is 14.025 MHz).
    pub freq_10hz: u64,
    pub mode: String,
    pub timestamp: DateTime<Utc>,
    /// `None` leaves the `ID` element out.
    pub id: Option<String>,
    pub is_original: bool,
    pub station_name: String,
    /// The worked station's locator and received location fields; empty by default, as on a
    /// contact with no exchange.
    pub gridsquare: String,
    pub exchange1: String,
    pub section: String,
    pub qth: String,
}

impl FakeContact {
    /// A 20 m CW contact from N1MM with a fresh ID.
    pub fn new(mycall: &str, call: &str, timestamp: DateTime<Utc>) -> Self {
        Self {
            app: "N1MM".into(),
            mycall: mycall.into(),
            call: call.into(),
            freq_10hz: 1_402_500,
            mode: "CW".into(),
            timestamp,
            id: Some(new_id()),
            is_original: true,
            station_name: "SHACK-PC".into(),
            gridsquare: String::new(),
            exchange1: String::new(),
            section: String::new(),
            qth: String::new(),
        }
    }

    /// `contactinfo`.
    pub fn info_xml(&self) -> String {
        self.contact_xml("contactinfo")
    }

    /// `contactreplace`, as N1MM sends after an edit.
    pub fn replace_xml(&self) -> String {
        self.contact_xml("contactreplace")
    }

    /// `contactdelete`: N1MM sends only these few fields.
    pub fn delete_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<contactdelete>\n");
        field(&mut xml, "app", &self.app);
        field(&mut xml, "timestamp", &self.timestamp_text());
        field(&mut xml, "call", &self.call);
        field(&mut xml, "contestnr", "1");
        field(&mut xml, "StationName", &self.station_name);
        if let Some(id) = &self.id {
            field(&mut xml, "ID", id);
        }
        xml.push_str("</contactdelete>\n");
        xml
    }

    fn timestamp_text(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn contact_xml(&self, root: &str) -> String {
        let freq = self.freq_10hz.to_string();
        // N1MM's band element is a decimal MHz string; the relay ignores it.
        let band_mhz = (self.freq_10hz / 10_000) as f64 / 10.0;
        let mut xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<{root}>\n");
        field(&mut xml, "app", &self.app);
        field(&mut xml, "contestname", "DXSERIAL");
        field(&mut xml, "contestnr", "1");
        field(&mut xml, "timestamp", &self.timestamp_text());
        field(&mut xml, "mycall", &self.mycall);
        field(&mut xml, "band", &band_mhz.to_string());
        field(&mut xml, "rxfreq", &freq);
        field(&mut xml, "txfreq", &freq);
        field(&mut xml, "operator", "");
        field(&mut xml, "mode", &self.mode);
        field(&mut xml, "call", &self.call);
        field(&mut xml, "snt", "599");
        field(&mut xml, "rcv", "599");
        field(&mut xml, "gridsquare", &self.gridsquare);
        field(&mut xml, "exchange1", &self.exchange1);
        field(&mut xml, "section", &self.section);
        field(&mut xml, "comment", "");
        field(&mut xml, "qth", &self.qth);
        field(&mut xml, "radionr", "1");
        field(
            &mut xml,
            "IsOriginal",
            if self.is_original { "True" } else { "False" },
        );
        field(&mut xml, "IsRunQSO", "0");
        field(&mut xml, "StationName", &self.station_name);
        if let Some(id) = &self.id {
            field(&mut xml, "ID", id);
        }
        field(&mut xml, "IsClaimedQso", "1");
        xml.push_str(&format!("</{root}>\n"));
        xml
    }
}

/// An N1MM `RadioInfo` packet, which the relay counts as activity and otherwise ignores.
pub fn radioinfo_xml() -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<RadioInfo>\n");
    field(&mut xml, "app", "N1MM");
    field(&mut xml, "StationName", "SHACK-PC");
    field(&mut xml, "RadioNr", "1");
    field(&mut xml, "Freq", "1402500");
    field(&mut xml, "TXFreq", "1402500");
    field(&mut xml, "Mode", "CW");
    xml.push_str("</RadioInfo>\n");
    xml
}

fn field(xml: &mut String, name: &str, value: &str) {
    if value.is_empty() {
        xml.push_str(&format!("\t<{name}></{name}>\n"));
    } else {
        xml.push_str(&format!("\t<{name}>{}</{name}>\n", escape(value)));
    }
}

/// A 32-hex-character ID like N1MM's GUIDs, unique within this process.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let digest = Sha256::digest(format!("{nanos}:{n}:{}", std::process::id()).as_bytes());
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Sends one datagram to `127.0.0.1:<port>`.
pub fn send(port: u16, payload: &[u8]) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    socket.send_to(payload, (Ipv4Addr::LOCALHOST, port))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::n1mm::{parse, ContactKind, Packet};
    use chrono::TimeZone;

    fn contact() -> FakeContact {
        let ts = Utc.with_ymd_and_hms(2026, 10, 28, 21, 14, 0).unwrap();
        let mut c = FakeContact::new("KK4PWJ", "W1AW", ts);
        c.id = Some("f9ffac4fcd3e479ca86e137df1338531".into());
        c
    }

    #[test]
    fn packets_parse_back() {
        let c = contact();
        let Packet::Contact(info) = parse(c.info_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(info.kind, ContactKind::Info);
        assert_eq!(info.mycall, "KK4PWJ");
        assert_eq!(info.frequency_hz, Some(14_025_000));
        assert_eq!(info.timestamp, Some(c.timestamp));
        assert_eq!(info.id.as_deref(), Some("f9ffac4fcd3e479ca86e137df1338531"));
        let Packet::Contact(rep) = parse(c.replace_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(rep.kind, ContactKind::Replace);
        let Packet::Contact(del) = parse(c.delete_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(del.kind, ContactKind::Delete);
        assert_eq!(del.mycall, "");
        assert_eq!(
            parse(radioinfo_xml().as_bytes()),
            Ok(Packet::Other("RadioInfo".into()))
        );
        assert!(parse(MALFORMED.as_bytes()).is_err());
    }

    #[test]
    fn location_fields_parse_back() {
        let mut c = contact();
        let Packet::Contact(info) = parse(c.info_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!((info.state, info.gridsquare), (None, None));
        c.gridsquare = "FM05pu".into();
        c.section = "NC".into();
        let Packet::Contact(info) = parse(c.info_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(info.state.as_deref(), Some("NC"));
        assert_eq!(info.gridsquare.as_deref(), Some("FM05PU"));
    }

    #[test]
    fn values_are_escaped() {
        let mut c = contact();
        c.call = "W1<AW>&".into();
        let Packet::Contact(info) = parse(c.info_xml().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(info.call, "W1<AW>&");
    }

    #[test]
    fn ids_are_unique_hex() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }
}
