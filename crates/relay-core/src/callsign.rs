//! The callsign check (contract section 7).
//!
//! [`base_callsign`] mirrors the server's SQL `base_callsign()`
//! (`supabase/migrations/20261012000040_callsign_rollups.sql` in the server repo) step for step;
//! both sides must agree or the server rejects what the relay sends.

/// Parts dropped as portable or operating suffixes.
const SUFFIXES: &[&str] = &["P", "M", "MM", "AM", "QRP", "QRPP", "R"];

/// Reduces a callsign to its base call: `W4/KK4PWJ/P` becomes `KK4PWJ`.
///
/// 1. Trim spaces and upper-case.
/// 2. Drop everything from the first `-` (an SSID).
/// 3. Split on `/`.
/// 4. Discard empty parts, the suffixes above and single digits.
/// 5. Take the longest part; on a tie prefer one with a letter followed by a digit, then the
///    later part.
/// 6. With no part left, the result is the string from step 2.
pub fn base_callsign(call: &str) -> String {
    // SQL's trim() removes spaces only.
    let upper = call.trim_matches(' ').to_uppercase();
    let call = upper.split('-').next().unwrap_or_default();
    let mut best: Option<((usize, bool, usize), &str)> = None;
    for (i, part) in call.split('/').enumerate() {
        if part.is_empty() || SUFFIXES.contains(&part) || is_single_digit(part) {
            continue;
        }
        let key = (part.chars().count(), has_letter_then_digit(part), i);
        if best.is_none_or(|(k, _)| key > k) {
            best = Some((key, part));
        }
    }
    best.map_or_else(|| call.to_string(), |(_, part)| part.to_string())
}

/// True when `mycall` is the same station as the account callsign (equal base calls). An empty
/// call is never the same station.
pub fn same_station(mycall: &str, account_callsign: &str) -> bool {
    let base = base_callsign(mycall);
    !base.is_empty() && base == base_callsign(account_callsign)
}

fn is_single_digit(part: &str) -> bool {
    part.len() == 1 && part.as_bytes()[0].is_ascii_digit()
}

fn has_letter_then_digit(part: &str) -> bool {
    part.as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_table() {
        let cases = [
            ("KK4PWJ", "KK4PWJ"),
            ("kk4pwj ", "KK4PWJ"),
            ("W4/KK4PWJ", "KK4PWJ"),
            ("KK4PWJ/P", "KK4PWJ"),
            ("KK4PWJ/M", "KK4PWJ"),
            ("KK4PWJ/MM", "KK4PWJ"),
            ("KK4PWJ/AM", "KK4PWJ"),
            ("KK4PWJ/QRP", "KK4PWJ"),
            ("KK4PWJ/QRPP", "KK4PWJ"),
            ("KK4PWJ/R", "KK4PWJ"),
            ("KK4PWJ/4", "KK4PWJ"),
            ("EA8/DL1ABC/P", "DL1ABC"),
            ("VP2V/W1AW", "W1AW"),
            ("W1AW-7", "W1AW"),
        ];
        for (input, want) in cases {
            assert_eq!(base_callsign(input), want, "base_callsign({input:?})");
        }
    }

    #[test]
    fn edge_cases_match_the_sql() {
        // No part survives: the SSID-stripped string comes back.
        assert_eq!(base_callsign("P"), "P");
        assert_eq!(base_callsign("/"), "/");
        assert_eq!(base_callsign(""), "");
        assert_eq!(base_callsign("4"), "4");
        // Equal length: the part with a letter then a digit wins over a bare prefix.
        assert_eq!(base_callsign("W1AW/ABCD"), "W1AW");
        assert_eq!(base_callsign("ABCD/W1AW"), "W1AW");
        // Equal length, both callsign-like: the later part.
        assert_eq!(base_callsign("K1AB/W1AW"), "W1AW");
        // Longest wins outright.
        assert_eq!(base_callsign("KH6/KK4PWJ"), "KK4PWJ");
        // Only spaces are trimmed, as SQL's trim() does; a dash ends the call.
        assert_eq!(base_callsign("  w1aw-15  "), "W1AW");
        assert_eq!(base_callsign("W1AW//P"), "W1AW");
    }

    /// Every row is the server's SQL `base_callsign()` output, run on the local stack
    /// (`select c, public.base_callsign(c) from unnest(array[...]) as c`) on 2026-10-28.
    #[test]
    fn matches_the_sql_on_tricky_calls() {
        let sql = [
            ("", ""),
            (" kk4pwj-7 ", "KK4PWJ"),
            ("-7", ""),
            ("/", "/"),
            ("3D2/W1AW", "W1AW"),
            ("4", "4"),
            ("9A/DL1ABC/QRP", "DL1ABC"),
            ("AB/CD", "CD"),
            ("ABCD/W1AW", "W1AW"),
            ("DL/G4ABC/MM", "G4ABC"),
            ("EA8/DL1ABC/P", "DL1ABC"),
            ("G4ABC/A", "G4ABC"),
            ("G4ABC/A/P", "G4ABC"),
            ("HB0/DL1ABC/P", "DL1ABC"),
            ("K/W1AW", "W1AW"),
            ("K1AB/W1AW", "W1AW"),
            ("KH6/KK4PWJ", "KK4PWJ"),
            ("KK4PWJ", "KK4PWJ"),
            ("kk4pwj ", "KK4PWJ"),
            ("KK4PWJ-7", "KK4PWJ"),
            ("KK4PWJ/4", "KK4PWJ"),
            ("KK4PWJ/AM", "KK4PWJ"),
            ("KK4PWJ/M", "KK4PWJ"),
            ("KK4PWJ/MM", "KK4PWJ"),
            ("KK4PWJ/P", "KK4PWJ"),
            ("KK4PWJ/P-7", "KK4PWJ"),
            ("KK4PWJ/QRP", "KK4PWJ"),
            ("kk4pwj/qrpp", "KK4PWJ"),
            ("KK4PWJ/QRPP", "KK4PWJ"),
            ("KK4PWJ/R", "KK4PWJ"),
            ("P", "P"),
            ("TA1/G4ABC", "G4ABC"),
            ("VK9/W1AW/P", "W1AW"),
            ("VP2E/KK4PWJ/M", "KK4PWJ"),
            ("VP2V/W1AW", "W1AW"),
            ("W1AW-7", "W1AW"),
            ("W1AW//P", "W1AW"),
            ("W1AW/3D2", "W1AW"),
            ("W1AW/7/P", "W1AW"),
            ("W1AW/ABCD", "W1AW"),
            ("W1AW/KH6", "W1AW"),
            ("W4/KK4PWJ", "KK4PWJ"),
        ];
        for (input, want) in sql {
            assert_eq!(base_callsign(input), want, "base_callsign({input:?})");
        }
    }

    #[test]
    fn same_station_compares_base_calls() {
        assert!(same_station("KK4PWJ/P", "KK4PWJ"));
        assert!(same_station("W4/KK4PWJ", "kk4pwj"));
        assert!(same_station("KK4PWJ", "KK4PWJ/M"));
        assert!(!same_station("K4XYZ", "KK4PWJ"));
        assert!(!same_station("", "KK4PWJ"));
        assert!(!same_station("", ""));
        assert!(!same_station("KK4PWJ", ""));
    }
}
