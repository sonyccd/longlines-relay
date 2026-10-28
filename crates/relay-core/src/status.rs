//! Stream row statuses and the exact words the app shows for them.
//!
//! The strings come from Brad's brief and the page 2 mockups. Strings marked "invented" in a
//! comment were not in either and are Brad's to reword.

use serde::{Deserialize, Serialize};

/// What happened to one packet the relay received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowStatus {
    /// Queued, not answered yet.
    Waiting,
    /// A new contact the server stored (or already had).
    Sent,
    /// An edit (`contactreplace`) the server applied.
    Updated,
    /// A delete (`contactdelete`) the server applied.
    Removed,
    /// Seen while test mode was on; never sent.
    TestMode,
    /// The station callsign's base call is not the account's.
    Mismatch { mycall: String },
    /// The logger sent no station callsign.
    NoMycall,
    /// Logged more than 30 minutes ago (on arrival, while queued, or by the server's clock).
    TooOld,
    /// Logged more than 5 minutes ahead of the clock.
    ClockSkew,
    /// `IsOriginal` false: another networked N1MM station's contact, sent there.
    Forwarded,
    /// Not a packet the relay could read.
    Unreadable,
    /// Refused for good; the reason is one of [`reasons`].
    CouldntSend { reason: String },
}

impl RowStatus {
    /// The status line under the row.
    pub fn text(&self) -> String {
        match self {
            RowStatus::Waiting => "Waiting to send".into(),
            RowStatus::Sent => "Sent as a spot".into(),
            RowStatus::Updated => "Edited in logger. Spot updated.".into(),
            RowStatus::Removed => "Deleted in logger. Spot removed.".into(),
            RowStatus::TestMode => "Not sent. Test mode is on.".into(),
            RowStatus::Mismatch { mycall } => format!("Ignored. Logged as {mycall}."),
            RowStatus::NoMycall => "Ignored. No station callsign in the logger.".into(),
            RowStatus::TooOld => "Not sent. Logged more than 30 minutes ago.".into(),
            // Invented: the brief names the rule but gives no words.
            RowStatus::ClockSkew => {
                "Not sent. Logged more than 5 minutes ahead of this computer's clock.".into()
            }
            // Invented (named in the task).
            RowStatus::Forwarded => "Ignored. Forwarded by another N1MM station.".into(),
            RowStatus::Unreadable => "Couldn't read packet".into(),
            RowStatus::CouldntSend { reason } => format!("Couldn't send: {reason}"),
        }
    }

    /// True once nothing more will happen to the row.
    pub fn is_final(&self) -> bool {
        !matches!(self, RowStatus::Waiting)
    }
}

/// Reasons for [`RowStatus::CouldntSend`]. All invented; Brad picks the wording.
pub mod reasons {
    /// The server answered the item `rejected` / `invalid`.
    pub const INVALID: &str = "Long Lines couldn't read this contact.";
    /// An item status or reason this version does not know (contract section 2).
    pub const UNEXPECTED: &str =
        "Long Lines gave an answer this version of the relay doesn't know.";
    /// 400 `invalid_request`.
    pub const BAD_REQUEST: &str = "Long Lines refused the request.";
    /// 403 `insufficient_scope`.
    pub const NOT_ALLOWED: &str = "This computer isn't allowed to post contacts. Sign in again.";
    /// 404 `not_found`.
    pub const NOT_FOUND: &str =
        "Long Lines doesn't accept contacts from this version of the relay.";
    /// A delete without a logger ID and without the fields to hash.
    pub const NO_CONTACT_ID: &str = "The logger didn't say which contact was deleted.";
    /// The queue database could not store the contact.
    pub const NOT_SAVED: &str = "The relay couldn't save this contact to send later.";

    /// Any other 4xx.
    pub fn http(status: u16) -> String {
        format!("Long Lines answered with error {status}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brief_strings_are_exact() {
        assert_eq!(RowStatus::Sent.text(), "Sent as a spot");
        assert_eq!(RowStatus::TestMode.text(), "Not sent. Test mode is on.");
        assert_eq!(
            RowStatus::Mismatch {
                mycall: "K4XYZ".into()
            }
            .text(),
            "Ignored. Logged as K4XYZ."
        );
        assert_eq!(
            RowStatus::TooOld.text(),
            "Not sent. Logged more than 30 minutes ago."
        );
        assert_eq!(RowStatus::Waiting.text(), "Waiting to send");
        assert_eq!(
            RowStatus::CouldntSend { reason: "x".into() }.text(),
            "Couldn't send: x"
        );
        assert_eq!(RowStatus::Updated.text(), "Edited in logger. Spot updated.");
        assert_eq!(
            RowStatus::Removed.text(),
            "Deleted in logger. Spot removed."
        );
        assert_eq!(RowStatus::Unreadable.text(), "Couldn't read packet");
        assert_eq!(
            RowStatus::NoMycall.text(),
            "Ignored. No station callsign in the logger."
        );
        assert_eq!(
            RowStatus::Forwarded.text(),
            "Ignored. Forwarded by another N1MM station."
        );
    }

    #[test]
    fn only_waiting_is_not_final() {
        assert!(!RowStatus::Waiting.is_final());
        assert!(RowStatus::Sent.is_final());
        assert!(RowStatus::TestMode.is_final());
    }
}
