//! Where the worked station is, when the logger says: its US state and its grid square
//! (contract 6.1, `state` and `gridsquare`).
//!
//! Both are optional and only ever come from the packet. The relay never looks a callsign up,
//! never guesses a state from a longer value (an ARRL section such as `ENY`, a county, a
//! sentence) and never sends a value it has not checked. Neither enters the `external_id` hash.

/// The postal codes of the 50 states and the District of Columbia, the values `state` may take.
pub const US_STATE_CODES: [&str; 51] = [
    "AK", "AL", "AR", "AZ", "CA", "CO", "CT", "DC", "DE", "FL", "GA", "HI", "IA", "ID", "IL", "IN",
    "KS", "KY", "LA", "MA", "MD", "ME", "MI", "MN", "MO", "MS", "MT", "NC", "ND", "NE", "NH", "NJ",
    "NM", "NV", "NY", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VA", "VT", "WA",
    "WI", "WV", "WY",
];

/// The value, trimmed and upper-cased, when it is exactly one of [`US_STATE_CODES`].
pub fn us_state(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_uppercase();
    US_STATE_CODES.iter().copied().find(|code| *code == value)
}

/// The state the logger gave in any of `values`: the one code they hold, or `None` when none of
/// them is exactly a state code or two of them name different states.
pub fn us_state_of<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'static str> {
    let mut found: Option<&'static str> = None;
    for code in values.into_iter().filter_map(us_state) {
        match found {
            None => found = Some(code),
            Some(seen) if seen == code => {}
            Some(_) => return None,
        }
    }
    found
}

/// A Maidenhead locator of 4, 6 or 8 characters, trimmed and upper-cased: a field (two letters
/// `A` to `R`), a square (two digits), then optionally a subsquare (two letters `A` to `X`) and
/// an extended square (two digits). Anything else, including 10 characters or more, is `None`.
pub fn gridsquare(value: &str) -> Option<String> {
    let grid = value.trim().to_ascii_uppercase();
    let b = grid.as_bytes();
    let pair = |i: usize, ok: fn(u8) -> bool| ok(b[i]) && ok(b[i + 1]);
    let field = |c: u8| (b'A'..=b'R').contains(&c);
    let subsquare = |c: u8| (b'A'..=b'X').contains(&c);
    let digit = |c: u8| c.is_ascii_digit();
    let valid = matches!(b.len(), 4 | 6 | 8)
        && pair(0, field)
        && pair(2, digit)
        && (b.len() < 6 || pair(4, subsquare))
        && (b.len() < 8 || pair(6, digit));
    valid.then_some(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_codes_are_the_fifty_states_and_dc() {
        assert_eq!(US_STATE_CODES.len(), 51);
        let mut sorted = US_STATE_CODES;
        sorted.sort_unstable();
        assert_eq!(sorted, US_STATE_CODES, "kept sorted, so no duplicate hides");
        assert!(US_STATE_CODES.windows(2).all(|w| w[0] != w[1]));
        assert!(US_STATE_CODES
            .iter()
            .all(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase())));
    }

    #[test]
    fn a_state_is_exactly_a_code() {
        assert_eq!(us_state("NC"), Some("NC"));
        assert_eq!(us_state(" nc "), Some("NC"));
        assert_eq!(us_state("DC"), Some("DC"));
        // Territories, Canadian provinces and the DX marker are not states.
        for not_a_state in ["PR", "VI", "GU", "ON", "BC", "QC", "DX", "", "N", "NCX"] {
            assert_eq!(us_state(not_a_state), None, "{not_a_state}");
        }
        // ARRL sections longer than two letters are never guessed at.
        for section in ["ENY", "WPA", "SFL", "NTX", "MDC", "SCV"] {
            assert_eq!(us_state(section), None, "{section}");
        }
        assert_eq!(us_state("North Carolina"), None);
        assert_eq!(us_state("N C"), None);
    }

    #[test]
    fn one_state_from_several_fields() {
        assert_eq!(us_state_of(["", "NC", ""]), Some("NC"));
        assert_eq!(us_state_of(["NC", "nc"]), Some("NC"));
        assert_eq!(us_state_of(["2A", "NC", "WAKE"]), Some("NC"));
        assert_eq!(us_state_of(["NC", "VA"]), None, "two states: send neither");
        assert_eq!(us_state_of(["", "ENY", "599"]), None);
        assert_eq!(us_state_of(std::iter::empty()), None);
    }

    #[test]
    fn grid_squares() {
        assert_eq!(gridsquare("FM05").as_deref(), Some("FM05"));
        assert_eq!(gridsquare("fm05pu").as_deref(), Some("FM05PU"));
        assert_eq!(gridsquare(" FM05pu47 ").as_deref(), Some("FM05PU47"));
        assert_eq!(gridsquare("AA00aa00").as_deref(), Some("AA00AA00"));
        assert_eq!(gridsquare("RR99XX99").as_deref(), Some("RR99XX99"));
        for bad in [
            "",
            "FM",
            "FM0",
            "FM05P",
            "FM05pu4",
            "FM05pu47ab",
            "SM05",
            "FS05",
            "FM5A",
            "FM05py",
            "FM05puAB",
            "0M05",
            "FM 05",
            "FM05-pu",
            "FM\u{0}5",
        ] {
            assert_eq!(gridsquare(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn non_ascii_never_panics() {
        assert_eq!(gridsquare("\u{1F4FB}\u{1F4FB}"), None);
        assert_eq!(gridsquare("F\u{e9}05"), None);
        assert_eq!(us_state("\u{e9}"), None);
    }
}
