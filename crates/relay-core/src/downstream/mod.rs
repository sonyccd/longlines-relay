//! The inbound direction: spots from Long Lines for the local DX cluster.
//!
//! [`stream`] holds the spots stream open (contract section 9) and hands each spot to the engine,
//! which serves it to logging software through [`crate::cluster`]. [`sse`] reads the stream's
//! Server-Sent Events framing.

pub mod sse;
pub mod stream;

use serde::{Deserialize, Serialize};

/// The spotter a spot without one is served under (contract 9.4 and 10.1).
pub const NO_SPOTTER: &str = "LONGLINES";

/// One `spot` event (contract 9.4). Unknown fields are ignored; the ones the client only shows
/// default to empty so a spot that lacks them still reaches the logger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub id: u64,
    #[serde(default)]
    pub spotter: String,
    pub frequency_khz: f64,
    pub callsign: String,
    #[serde(default)]
    pub comment: String,
    /// `HHMM`, UTC.
    #[serde(default)]
    pub time_utc: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_label: String,
    #[serde(default)]
    pub connection: String,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    #[serde(default)]
    pub spot_time: String,
}

impl Spot {
    /// Reads a `spot` event's `data`. `None` for anything the cluster could not serve: not JSON,
    /// no id, no callsign, or a frequency that is not a positive number.
    pub fn from_data(data: &str) -> Option<Spot> {
        let spot: Spot = match serde_json::from_str(data) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "couldn't read a spot from Long Lines");
                return None;
            }
        };
        let usable = spot.frequency_khz.is_finite()
            && spot.frequency_khz > 0.0
            && !spot.callsign.trim().is_empty();
        if !usable {
            tracing::warn!(
                id = spot.id,
                "a spot from Long Lines had no usable frequency or call"
            );
            return None;
        }
        Some(spot)
    }
}

/// The `hello` event (contract 9.3).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Hello {
    #[serde(default)]
    pub relay_id: String,
    #[serde(default)]
    pub device_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"{"id":48213377,"spotter":"W4DOG","frequency_khz":14062.0,"callsign":"K4ABC","comment":"CW POTA K-0039","time_utc":"2114","mode":"CW","reference":"POTA K-0039","source":"pota","source_label":"POTA","connection":"POTA states","spot_time":"2026-10-28T21:14:07Z"}"#;

    #[test]
    fn reads_the_contract_example() {
        let spot = Spot::from_data(EXAMPLE).unwrap();
        assert_eq!(spot.id, 48_213_377);
        assert_eq!(spot.spotter, "W4DOG");
        assert_eq!(spot.frequency_khz, 14062.0);
        assert_eq!(spot.mode.as_deref(), Some("CW"));
        assert_eq!(spot.reference.as_deref(), Some("POTA K-0039"));
        assert_eq!(spot.connection, "POTA states");
    }

    #[test]
    fn tolerates_unknown_and_missing_display_fields() {
        let spot = Spot::from_data(
            r#"{"id":1,"frequency_khz":7032.5,"callsign":"W9XYZ","comment":"CW","time_utc":"2111","mode":null,"new_field":[1]}"#,
        )
        .unwrap();
        assert_eq!(spot.spotter, "");
        assert_eq!(spot.mode, None);
        assert_eq!(spot.connection, "");
    }

    #[test]
    fn refuses_what_cannot_be_served() {
        assert!(Spot::from_data("not json").is_none());
        assert!(Spot::from_data(r#"{"frequency_khz":1.0,"callsign":"W1AW"}"#).is_none());
        assert!(Spot::from_data(r#"{"id":1,"frequency_khz":0,"callsign":"W1AW"}"#).is_none());
        assert!(Spot::from_data(r#"{"id":1,"frequency_khz":-3.5,"callsign":"W1AW"}"#).is_none());
        assert!(Spot::from_data(r#"{"id":1,"frequency_khz":14000.0,"callsign":" "}"#).is_none());
    }
}
