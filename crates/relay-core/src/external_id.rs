//! The contact's idempotency key (contract section 6.2).

use sha2::{Digest, Sha256};

/// The longest `external_id` the server accepts.
pub const MAX_EXTERNAL_ID_LEN: usize = 256;

/// The fields hashed when the logger sends no contact ID, exactly as they are sent.
#[derive(Clone, Copy, Debug)]
pub struct HashFields<'a> {
    pub mycall: &'a str,
    pub call: &'a str,
    pub band: Option<&'a str>,
    pub mode: &'a str,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub timestamp_utc: &'a str,
}

/// The logger's ID, trimmed, when it has one the server accepts; otherwise the fallback hash.
pub fn external_id(logger_id: Option<&str>, fields: HashFields<'_>) -> String {
    match logger_id.map(str::trim) {
        Some(id) if !id.is_empty() && id.chars().count() <= MAX_EXTERNAL_ID_LEN => id.to_string(),
        _ => fallback_hash(fields),
    }
}

/// Lower-case hex SHA-256 of `mycall|call|band|mode|timestamp_utc`, with an empty band for none.
pub fn fallback_hash(fields: HashFields<'_>) -> String {
    let joined = format!(
        "{}|{}|{}|{}|{}",
        fields.mycall,
        fields.call,
        fields.band.unwrap_or(""),
        fields.mode,
        fields.timestamp_utc
    );
    let digest = Sha256::digest(joined.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields<'a>(band: Option<&'a str>) -> HashFields<'a> {
        HashFields {
            mycall: "KK4PWJ",
            call: "W1AW",
            band,
            mode: "CW",
            timestamp_utc: "2026-10-28T21:14:00Z",
        }
    }

    #[test]
    fn contract_example_hash() {
        assert_eq!(
            fallback_hash(fields(Some("20m"))),
            "8c1a593b282999aa3e5714c50d593b3d9b7049c568d78cff4b15f711866f22cb"
        );
    }

    #[test]
    fn contract_rejection_example_hash() {
        let f = HashFields {
            mycall: "K4XYZ",
            call: "VE3ABC",
            band: Some("15m"),
            mode: "FT8",
            timestamp_utc: "2026-10-28T21:15:45Z",
        };
        assert_eq!(
            fallback_hash(f),
            "f79e495bd37010e153bd2da07fd4eb22a248faa643887dffdd1d33e918e1c91c"
        );
    }

    #[test]
    fn null_band_hashes_as_empty() {
        let none = fallback_hash(fields(None));
        let empty = fallback_hash(fields(Some("")));
        assert_eq!(none, empty);
        assert_ne!(none, fallback_hash(fields(Some("20m"))));
    }

    #[test]
    fn logger_id_wins_when_usable() {
        let f = fields(Some("20m"));
        assert_eq!(
            external_id(Some("  f9ffac4fcd3e479ca86e137df1338531 "), f),
            "f9ffac4fcd3e479ca86e137df1338531"
        );
        assert_eq!(external_id(Some("   "), f), fallback_hash(f));
        assert_eq!(external_id(None, f), fallback_hash(f));
        let long = "x".repeat(MAX_EXTERNAL_ID_LEN + 1);
        assert_eq!(external_id(Some(&long), f), fallback_hash(f));
        let max = "x".repeat(MAX_EXTERNAL_ID_LEN);
        assert_eq!(external_id(Some(&max), f), max);
    }
}
