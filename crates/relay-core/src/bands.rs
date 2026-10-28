//! The band table from the contract's Appendix A (the server's `_shared/bands.ts`).
//!
//! The band is always derived from the frequency, never read from the logger's `band` element:
//! N1MM sends it as a decimal string (`3.5`) and DXLog.net uses the computer's decimal separator.

/// One row of the band table, inclusive on both ends, in kHz.
struct BandRange {
    from_khz: u64,
    to_khz: u64,
    band: &'static str,
}

/// Rows in the contract's order. The first match wins, so QO-100 frequencies get `3cm_qo100`.
const BANDS: &[BandRange] = &[
    BandRange {
        from_khz: 135,
        to_khz: 138,
        band: "2200m",
    },
    BandRange {
        from_khz: 472,
        to_khz: 479,
        band: "600m",
    },
    BandRange {
        from_khz: 1_800,
        to_khz: 2_000,
        band: "160m",
    },
    BandRange {
        from_khz: 3_500,
        to_khz: 4_000,
        band: "80m",
    },
    BandRange {
        from_khz: 5_000,
        to_khz: 5_500,
        band: "60m",
    },
    BandRange {
        from_khz: 7_000,
        to_khz: 7_300,
        band: "40m",
    },
    BandRange {
        from_khz: 10_000,
        to_khz: 10_200,
        band: "30m",
    },
    BandRange {
        from_khz: 14_000,
        to_khz: 14_500,
        band: "20m",
    },
    BandRange {
        from_khz: 18_000,
        to_khz: 18_200,
        band: "17m",
    },
    BandRange {
        from_khz: 21_000,
        to_khz: 21_500,
        band: "15m",
    },
    BandRange {
        from_khz: 24_800,
        to_khz: 25_000,
        band: "12m",
    },
    BandRange {
        from_khz: 26_000,
        to_khz: 27_999,
        band: "11m",
    },
    BandRange {
        from_khz: 28_000,
        to_khz: 30_000,
        band: "10m",
    },
    BandRange {
        from_khz: 40_000,
        to_khz: 41_000,
        band: "8m",
    },
    BandRange {
        from_khz: 50_000,
        to_khz: 54_000,
        band: "6m",
    },
    BandRange {
        from_khz: 70_000,
        to_khz: 71_000,
        band: "4m",
    },
    BandRange {
        from_khz: 144_000,
        to_khz: 148_000,
        band: "2m",
    },
    BandRange {
        from_khz: 219_000,
        to_khz: 225_000,
        band: "1.25m",
    },
    BandRange {
        from_khz: 430_000,
        to_khz: 440_000,
        band: "70cm",
    },
    BandRange {
        from_khz: 1_200_000,
        to_khz: 1_400_000,
        band: "23cm",
    },
    BandRange {
        from_khz: 2_300_000,
        to_khz: 2_450_000,
        band: "13cm",
    },
    BandRange {
        from_khz: 3_300_000,
        to_khz: 3_500_000,
        band: "9cm",
    },
    BandRange {
        from_khz: 5_400_000,
        to_khz: 5_900_000,
        band: "6cm",
    },
    BandRange {
        from_khz: 10_489_550,
        to_khz: 10_490_000,
        band: "3cm_qo100",
    },
    BandRange {
        from_khz: 10_000_000,
        to_khz: 10_500_000,
        band: "3cm",
    },
];

/// The contract's `band` for a frequency in Hz, or `None` when it is in no row.
///
/// The contract compares `frequency_hz / 1000` as a real number against whole-kHz bounds, which
/// is the same as comparing Hz against the bounds times 1000.
pub fn band_for_hz(frequency_hz: u64) -> Option<&'static str> {
    BANDS
        .iter()
        .find(|b| frequency_hz >= b.from_khz * 1000 && frequency_hz <= b.to_khz * 1000)
        .map(|b| b.band)
}

/// The band as the UI shows it: a space before the unit, as the mockups write "40 m".
pub fn band_label(band: &str) -> String {
    if band == "3cm_qo100" {
        return "3 cm QO-100".to_string();
    }
    match band.find(|c: char| c.is_ascii_alphabetic()) {
        Some(i) if i > 0 => format!("{} {}", &band[..i], &band[i..]),
        _ => band.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_from_the_contract() {
        assert_eq!(band_for_hz(14_025_000), Some("20m"));
        assert_eq!(band_for_hz(14_500_000), Some("20m"));
        assert_eq!(band_for_hz(14_500_500), None);
        assert_eq!(band_for_hz(3_525_190), Some("80m"));
        assert_eq!(band_for_hz(7_185_500), Some("40m"));
        assert_eq!(band_for_hz(21_074_000), Some("15m"));
        assert_eq!(band_for_hz(135_000), Some("2200m"));
        assert_eq!(band_for_hz(10_489_750_000), Some("3cm_qo100"));
        assert_eq!(band_for_hz(10_400_000_000), Some("3cm"));
        assert_eq!(band_for_hz(0), None);
        assert_eq!(band_for_hz(100_000_000), None);
    }

    #[test]
    fn labels_put_a_space_before_the_unit() {
        assert_eq!(band_label("40m"), "40 m");
        assert_eq!(band_label("70cm"), "70 cm");
        assert_eq!(band_label("1.25m"), "1.25 m");
        assert_eq!(band_label("3cm_qo100"), "3 cm QO-100");
    }
}
