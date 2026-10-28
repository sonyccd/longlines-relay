//! Freshness rules shared with the server (contract section 6.3).

use chrono::{DateTime, TimeDelta, Utc};

/// An upsert logged more than this long ago is not sent ("too old").
pub const MAX_CONTACT_AGE: TimeDelta = TimeDelta::minutes(30);

/// No item is sent when its time is more than this far ahead of this computer's clock.
pub const MAX_CLOCK_SKEW: TimeDelta = TimeDelta::minutes(5);

/// Where a contact's logged time falls relative to now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    /// Logged more than [`MAX_CONTACT_AGE`] ago. Deletes are exempt; upserts are not sent.
    TooOld,
    /// Logged more than [`MAX_CLOCK_SKEW`] in the future. Nothing is sent.
    Future,
}

/// Classifies a logged time against `now`. Exactly 30 minutes old, or exactly 5 minutes ahead,
/// is still fresh: the limits are "more than".
pub fn classify(logged_at: DateTime<Utc>, now: DateTime<Utc>) -> Freshness {
    if logged_at > now + MAX_CLOCK_SKEW {
        Freshness::Future
    } else if logged_at < now - MAX_CONTACT_AGE {
        Freshness::TooOld
    } else {
        Freshness::Fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 21, 14, 0).unwrap()
    }

    #[test]
    fn classifies_against_the_limits() {
        let now = now();
        assert_eq!(classify(now, now), Freshness::Fresh);
        assert_eq!(
            classify(now - TimeDelta::minutes(29), now),
            Freshness::Fresh
        );
        assert_eq!(
            classify(now - TimeDelta::minutes(30), now),
            Freshness::Fresh
        );
        assert_eq!(
            classify(now - TimeDelta::minutes(30) - TimeDelta::seconds(1), now),
            Freshness::TooOld
        );
        assert_eq!(classify(now - TimeDelta::days(3), now), Freshness::TooOld);
        assert_eq!(classify(now + TimeDelta::minutes(5), now), Freshness::Fresh);
        assert_eq!(
            classify(now + TimeDelta::minutes(5) + TimeDelta::seconds(1), now),
            Freshness::Future
        );
    }

    #[test]
    fn constants_match_the_contract() {
        assert_eq!(MAX_CONTACT_AGE.num_seconds(), 30 * 60);
        assert_eq!(MAX_CLOCK_SKEW.num_seconds(), 5 * 60);
    }
}
