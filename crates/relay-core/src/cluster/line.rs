//! The spot line (contract section 10): one spot as DXSpider lays it out, so logging software
//! that reads a DX cluster reads it unchanged.
//!
//! ```text
//!          1         2         3         4         5         6         7
//! 123456789012345678901234567890123456789012345678901234567890123456789012345
//! DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z
//! ```
//!
//! The relay owns the served line; the server renders the same layout only for the web app's
//! preview, and both are pinned to the contract's examples.

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::downstream::{Spot, NO_SPOTTER};

/// Every line the cluster sends ends with this.
pub const LINE_END: &str = "\r\n";
/// The comment column's width; a longer comment is cut.
pub const COMMENT_WIDTH: usize = 30;
/// The callsign column's width; a longer callsign takes what it needs.
pub const CALLSIGN_WIDTH: usize = 12;
/// Columns 7 to 24: the spotter, its colon, spaces and the frequency.
pub const SPOTTER_FREQUENCY_WIDTH: usize = 18;

/// What one line is made of, already in the line's alphabet once [`format_line`] cleans it.
#[derive(Clone, Debug, PartialEq)]
pub struct LineParts {
    pub spotter: String,
    pub frequency_khz: f64,
    pub callsign: String,
    pub comment: String,
    /// `HHMM`.
    pub time_utc: String,
}

/// `spotter` and `callsign`: anything outside U+0021 to U+007E becomes `?`.
fn printable_word(s: &str) -> String {
    s.chars()
        .map(|c| {
            if ('\u{21}'..='\u{7e}').contains(&c) {
                c
            } else {
                '?'
            }
        })
        .collect()
}

/// `comment`: anything outside U+0020 to U+007E becomes `?`, then it is cut to 30.
fn printable_comment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if (' '..='\u{7e}').contains(&c) {
                c
            } else {
                '?'
            }
        })
        .take(COMMENT_WIDTH)
        .collect()
}

/// The line for `parts`, CR LF included (contract 10.1).
pub fn format_line(parts: &LineParts) -> String {
    let spotter = match printable_word(&parts.spotter) {
        s if s.is_empty() => NO_SPOTTER.to_string(),
        s => s,
    };
    let head = format!("{spotter}:");
    let frequency = format!("{:.1}", parts.frequency_khz);
    let gap = SPOTTER_FREQUENCY_WIDTH
        .saturating_sub(head.len() + frequency.len())
        .max(1);
    let callsign = printable_word(&parts.callsign);
    let comment = printable_comment(&parts.comment);
    format!(
        "DX de {head}{:gap$}{frequency}  {callsign:<cw$} {comment:<mw$} {}Z{LINE_END}",
        "",
        parts.time_utc,
        cw = CALLSIGN_WIDTH,
        mw = COMMENT_WIDTH,
    )
}

/// `HHMM` from the spot: its `time_utc` when that is four digits, else `spot_time`, else `now`.
fn spot_hhmm(spot: &Spot, now: DateTime<Utc>) -> String {
    let t = &spot.time_utc;
    if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
        return t.clone();
    }
    NaiveDateTime::parse_from_str(&spot.spot_time, "%Y-%m-%dT%H:%M:%SZ")
        .map(|t| t.and_utc())
        .unwrap_or(now)
        .format("%H%M")
        .to_string()
}

/// The line the cluster serves for `spot`.
pub fn format_spot(spot: &Spot, now: DateTime<Utc>) -> String {
    format_line(&LineParts {
        spotter: spot.spotter.clone(),
        frequency_khz: spot.frequency_khz,
        callsign: spot.callsign.clone(),
        comment: spot.comment.clone(),
        time_utc: spot_hhmm(spot, now),
    })
}

/// Reads a line in this layout back into its parts: what the relay serves, and what the real
/// DXSpider capture holds. Anything after `HHMMZ` (DXSpider's BEL characters, a line end) is
/// ignored, and the comment loses the trailing spaces its padding added. `None` for a line that
/// does not fit the layout.
pub fn parse_line(line: &str) -> Option<LineParts> {
    let end = line.rfind('Z')?;
    let line = &line[..=end];
    let time_utc = line.get(line.len().checked_sub(5)?..line.len() - 1)?;
    if !time_utc.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let rest = line.strip_prefix("DX de ")?;
    let (spotter, rest) = rest.split_once(':')?;
    let rest = rest.trim_start_matches(' ');
    let (frequency, rest) = rest.split_once(' ')?;
    let frequency_khz: f64 = frequency.parse().ok()?;
    let callsign_at = line.len() - rest.trim_start_matches(' ').len();
    let (callsign, _) = line[callsign_at..].split_once(' ')?;
    let comment_at = callsign_at + callsign.len().max(CALLSIGN_WIDTH) + 1;
    let comment_end = line.len().checked_sub(6)?;
    let comment = line.get(comment_at..comment_end)?.trim_end_matches(' ');
    Some(LineParts {
        spotter: spotter.to_string(),
        frequency_khz,
        callsign: callsign.to_string(),
        comment: comment.to_string(),
        time_utc: time_utc.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn parts(spotter: &str, khz: f64, call: &str, comment: &str, time: &str) -> LineParts {
        LineParts {
            spotter: spotter.into(),
            frequency_khz: khz,
            callsign: call.into(),
            comment: comment.into(),
            time_utc: time.into(),
        }
    }

    /// Contract 10.3, "From Long Lines spots", byte for byte.
    #[test]
    fn the_contract_examples() {
        let cases = [
            (
                parts("W4DOG", 14062.0, "K4ABC", "CW POTA K-0039", "2114"),
                "DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z",
            ),
            (
                parts(
                    "KK4PWJ",
                    7185.5,
                    "W1AW",
                    "LSB SOTA W4C/CM-001 SOTAwatch",
                    "2110",
                ),
                "DX de KK4PWJ:     7185.5  W1AW         LSB SOTA W4C/CM-001 SOTAwatch  2110Z",
            ),
            (
                parts("W3LPL-#", 3530.0, "G4ABC", "CW RBN", "2120"),
                "DX de W3LPL-#:    3530.0  G4ABC        CW RBN                         2120Z",
            ),
            (
                parts(
                    "N4AB",
                    14062.0,
                    "K4ABC",
                    "CW POTA K-0039 Shining Rock Wi",
                    "2114",
                ),
                "DX de N4AB:      14062.0  K4ABC        CW POTA K-0039 Shining Rock Wi 2114Z",
            ),
            (
                parts("", 7032.5, "W9XYZ", "CW POTA K-4571", "2111"),
                "DX de LONGLINES:  7032.5  W9XYZ        CW POTA K-4571                 2111Z",
            ),
            (
                parts(
                    "KK4PWJ",
                    14050.0,
                    "LLTEST",
                    "CW Long Lines Relay test spot",
                    "2102",
                ),
                "DX de KK4PWJ:    14050.0  LLTEST       CW Long Lines Relay test spot  2102Z",
            ),
            (
                parts("KK4PWJ", 14285.0, "VP2E/KK4PWJ/M", "USB", "2120"),
                "DX de KK4PWJ:    14285.0  VP2E/KK4PWJ/M USB                            2120Z",
            ),
            (
                parts("UR2680YL", 1_296_200.0, "G4ABC", "CW", "2120"),
                "DX de UR2680YL: 1296200.0  G4ABC        CW                             2120Z",
            ),
        ];
        for (p, want) in cases {
            let got = format_line(&p);
            assert_eq!(got, format!("{want}\r\n"));
            // And back: the parts split out again (the empty spotter comes back as LONGLINES).
            let back = parse_line(&got).unwrap();
            assert_eq!(back.callsign, p.callsign);
            assert_eq!(back.comment, p.comment);
            assert_eq!(back.frequency_khz, p.frequency_khz);
        }
    }

    #[test]
    fn standard_lines_are_75_columns() {
        let line = format_line(&parts("W4DOG", 14062.0, "K4ABC", "CW", "2114"));
        assert_eq!(line.len(), 77);
        assert!(line.ends_with("2114Z\r\n"));
        assert_eq!(&line[26..31], "K4ABC");
        assert_eq!(&line[39..41], "CW");
    }

    #[test]
    fn cuts_and_cleans() {
        let line = format_line(&parts(
            "W4D\u{d6}G",
            14062.04,
            "K4\tABC",
            "CW caf\u{e9}\u{7}\u{1F4FB} and a comment far longer than thirty",
            "2114",
        ));
        assert_eq!(
            line,
            "DX de W4D?G:     14062.0  K4?ABC       CW caf??? and a comment far lo 2114Z\r\n"
        );
        assert!(line.is_ascii());
    }

    #[test]
    fn one_decimal_place() {
        let f = |khz| {
            parse_line(&format_line(&parts("A", khz, "B", "", "0000")))
                .unwrap()
                .frequency_khz
        };
        assert_eq!(f(14062.0), 14062.0);
        assert_eq!(f(7032.5), 7032.5);
        assert!(format_line(&parts("A", 14062.0, "B", "", "0000")).contains(" 14062.0  "));
    }

    #[test]
    fn spot_times() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 21, 2, 30).unwrap();
        let mut spot = Spot::from_data(
            r#"{"id":1,"spotter":"W4DOG","frequency_khz":14062.0,"callsign":"K4ABC","comment":"CW POTA K-0039","time_utc":"2114","spot_time":"2026-10-28T21:14:07Z"}"#,
        )
        .unwrap();
        let line = format_spot(&spot, now);
        assert_eq!(
            line,
            "DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z\r\n"
        );
        spot.time_utc = "21:14".into();
        spot.spot_time = "2026-10-28T20:59:59Z".into();
        assert!(format_spot(&spot, now).ends_with(" 2059Z\r\n"));
        spot.spot_time = String::new();
        assert!(format_spot(&spot, now).ends_with(" 2102Z\r\n"));
    }

    #[test]
    fn refuses_lines_that_do_not_fit() {
        assert!(parse_line("").is_none());
        assert!(parse_line("WWV de VE7CC <18>:   SFI=97, A=7, K=1").is_none());
        assert!(parse_line("DX de W4DOG: abc  K4ABC        CW 2114Z").is_none());
        assert!(parse_line("DX de W4DOG:     14062.0  K4ABC        CW xxxxZ").is_none());
    }
}
