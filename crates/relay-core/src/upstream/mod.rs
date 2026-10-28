//! The outbound direction: contacts to Long Lines.
//!
//! [`queue`] keeps contacts on disk until the server answers, [`client`] speaks the v1 API and
//! [`token_store`] keeps the token in the OS credential store. A phase 2 `downstream` module
//! (spots from Long Lines into a local DX cluster) sits beside this one.

pub mod client;
pub mod queue;
pub mod token_store;

use serde::{Deserialize, Serialize};

use crate::n1mm::ContactKind;

/// The item operation (contract 6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Upsert,
    Delete,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Upsert => "upsert",
            Op::Delete => "delete",
        }
    }
}

/// A contact waiting to be sent, with every field already in its contract form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedContact {
    pub external_id: String,
    pub op: Op,
    /// Which packet it came from, so the result can be worded (an edit, a delete).
    pub kind: ContactKind,
    pub mycall: String,
    pub call: String,
    /// Always set for an upsert.
    pub frequency_hz: Option<u64>,
    pub band: Option<String>,
    pub mode: String,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub timestamp_utc: String,
    pub logger_app: Option<String>,
}

/// One item of `POST /contacts`, as sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WireItem {
    pub external_id: String,
    pub op: Op,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mycall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_hz: Option<u64>,
    /// Upserts always carry `band`, null when out of band; deletes leave it out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub timestamp_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger_app: Option<Option<String>>,
}

impl QueuedContact {
    /// The wire item: everything for an upsert, only the id, op and time for a delete.
    pub fn to_wire(&self) -> WireItem {
        match self.op {
            Op::Upsert => WireItem {
                external_id: self.external_id.clone(),
                op: Op::Upsert,
                mycall: Some(self.mycall.clone()),
                call: Some(self.call.clone()),
                frequency_hz: self.frequency_hz,
                band: Some(self.band.clone()),
                mode: Some(self.mode.clone()),
                timestamp_utc: self.timestamp_utc.clone(),
                logger_app: Some(self.logger_app.clone()),
            },
            Op::Delete => WireItem {
                external_id: self.external_id.clone(),
                op: Op::Delete,
                mycall: None,
                call: None,
                frequency_hz: None,
                band: None,
                mode: None,
                timestamp_utc: self.timestamp_utc.clone(),
                logger_app: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn contact(op: Op) -> QueuedContact {
        QueuedContact {
            external_id: "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b".into(),
            op,
            kind: ContactKind::Info,
            mycall: "KK4PWJ".into(),
            call: "W1AW".into(),
            frequency_hz: Some(14_025_000),
            band: Some("20m".into()),
            mode: "CW".into(),
            timestamp_utc: "2026-10-28T21:14:00Z".into(),
            logger_app: None,
        }
    }

    #[test]
    fn upsert_carries_every_field_with_nulls() {
        let wire = serde_json::to_value(contact(Op::Upsert).to_wire()).unwrap();
        assert_eq!(
            wire,
            json!({
                "external_id": "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b",
                "op": "upsert",
                "mycall": "KK4PWJ",
                "call": "W1AW",
                "frequency_hz": 14025000,
                "band": "20m",
                "mode": "CW",
                "timestamp_utc": "2026-10-28T21:14:00Z",
                "logger_app": null
            })
        );
        let mut out_of_band = contact(Op::Upsert);
        out_of_band.band = None;
        let wire = serde_json::to_value(out_of_band.to_wire()).unwrap();
        assert_eq!(wire["band"], json!(null));
    }

    #[test]
    fn delete_carries_only_id_op_and_time() {
        let wire = serde_json::to_value(contact(Op::Delete).to_wire()).unwrap();
        assert_eq!(
            wire,
            json!({
                "external_id": "4f0d1c2b3a5e4d6f8a9b0c1d2e3f4a5b",
                "op": "delete",
                "timestamp_utc": "2026-10-28T21:14:00Z"
            })
        );
    }
}
