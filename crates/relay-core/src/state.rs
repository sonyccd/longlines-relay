//! What the UI shows: the feed card, the account, the upstream connection and the last 50
//! stream rows. In memory only; test mode is always off at launch.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::bands::band_label;
use crate::listeners::ListenerStatus;
use crate::n1mm::ContactKind;
use crate::status::RowStatus;
use crate::upstream::auth::PairingView;

/// Contacts seen keeps this many rows.
pub const STREAM_LIMIT: usize = 50;

/// The Long Lines account the token belongs to (`GET /me`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub callsign: String,
    pub email: String,
}

/// One row of Contacts seen.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StreamRow {
    pub id: u64,
    /// `None` for a packet that could not be read.
    pub kind: Option<ContactKind>,
    pub call: String,
    /// The contract's band (`40m`), derived from the frequency.
    pub band: Option<String>,
    pub mode: String,
    pub frequency_hz: Option<u64>,
    pub logged_at: Option<DateTime<Utc>>,
    pub received_at: DateTime<Utc>,
    /// "W4ABC · 40 m CW"; empty for an unreadable packet.
    pub title: String,
    /// "14:22 UTC": the logged time, else the time it arrived.
    pub time_utc: String,
    pub status: RowStatus,
    pub status_text: String,
    /// Used to fold an N1MM edit's delete into its replace. Not shown.
    #[serde(skip)]
    pub external_id: Option<String>,
}

impl StreamRow {
    /// A row for a contact packet. The id is assigned by [`RelayState::push_row`].
    pub fn for_contact(
        kind: ContactKind,
        call: &str,
        frequency_hz: Option<u64>,
        mode: &str,
        logged_at: Option<DateTime<Utc>>,
        received_at: DateTime<Utc>,
        status: RowStatus,
    ) -> Self {
        let band = frequency_hz
            .and_then(crate::bands::band_for_hz)
            .map(str::to_string);
        Self {
            id: 0,
            kind: Some(kind),
            call: call.to_string(),
            title: title(call, band.as_deref(), frequency_hz, mode),
            band,
            mode: mode.to_string(),
            frequency_hz,
            logged_at,
            received_at,
            time_utc: logged_at
                .unwrap_or(received_at)
                .format("%H:%M UTC")
                .to_string(),
            status_text: status.text(),
            status,
            external_id: None,
        }
    }

    /// A row for a packet that could not be read.
    pub fn unreadable(received_at: DateTime<Utc>) -> Self {
        let status = RowStatus::Unreadable;
        Self {
            id: 0,
            kind: None,
            call: String::new(),
            band: None,
            mode: String::new(),
            frequency_hz: None,
            logged_at: None,
            received_at,
            title: String::new(),
            time_utc: received_at.format("%H:%M UTC").to_string(),
            status_text: status.text(),
            status,
            external_id: None,
        }
    }
}

/// "W4ABC · 40 m CW". Out of band, the frequency in MHz stands in for the band.
fn title(call: &str, band: Option<&str>, frequency_hz: Option<u64>, mode: &str) -> String {
    let middle = match (band, frequency_hz) {
        (Some(b), _) => band_label(b),
        (None, Some(hz)) => format!("{} MHz", hz as f64 / 1_000_000.0),
        (None, None) => String::new(),
    };
    let tail = [middle.as_str(), mode]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    match (call.is_empty(), tail.is_empty()) {
        (true, _) => tail,
        (false, true) => call.to_string(),
        (false, false) => format!("{call} \u{b7} {tail}"),
    }
}

/// The Logging feed card's chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedStatus {
    /// A packet arrived since the listener started.
    Receiving,
    /// Listening; nothing arrived yet.
    Waiting,
    /// The latest contact failed the callsign check.
    IgnoringContacts,
    /// The port could not be bound.
    PortInUse,
    /// Test mode is on.
    TestMode,
}

impl FeedStatus {
    pub fn label(self) -> &'static str {
        match self {
            FeedStatus::Receiving => "Receiving",
            FeedStatus::Waiting => "Waiting for logging software",
            FeedStatus::IgnoringContacts => "Ignoring contacts",
            FeedStatus::PortInUse => "Port in use",
            FeedStatus::TestMode => "Test mode",
        }
    }
}

/// Everything the Logging feed card shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FeedView {
    pub status: FeedStatus,
    pub label: String,
    pub listener: ListenerStatus,
    /// The configured or bound port.
    pub port: u16,
    pub last_packet_at: Option<DateTime<Utc>>,
    /// Datagrams from other computers, dropped unread.
    pub dropped_packets: u64,
    /// The station callsign of the latest contact that failed the callsign check.
    pub mismatch: Option<String>,
    pub test_mode: bool,
}

/// The sender's view of Long Lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpstreamStatus {
    /// No token, so nothing is sent.
    SignedOut,
    /// Signed in; the last request (if any) went through.
    Connected,
    /// The last request failed; the next try is at `at`.
    Retrying { at: DateTime<Utc>, reason: String },
    /// 429: waiting until `until`.
    RateLimited { until: DateTime<Utc> },
}

/// Everything the window needs to draw itself.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Snapshot {
    pub account: Option<Account>,
    pub feed: FeedView,
    pub upstream: UpstreamStatus,
    /// The sign-in screen.
    pub pairing: PairingView,
    /// Newest first.
    pub stream: Vec<StreamRow>,
}

#[derive(Debug)]
struct Inner {
    rows: VecDeque<StreamRow>,
    next_row_id: u64,
    listener: ListenerStatus,
    listening_since: Option<DateTime<Utc>>,
    last_packet_at: Option<DateTime<Utc>>,
    mismatch: Option<String>,
    test_mode: bool,
    account: Option<Account>,
    upstream: UpstreamStatus,
    pairing: PairingView,
}

/// Shared, lock-protected app state.
#[derive(Debug)]
pub struct RelayState {
    inner: Mutex<Inner>,
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                rows: VecDeque::with_capacity(STREAM_LIMIT),
                next_row_id: 1,
                listener: ListenerStatus::Stopped { port: 0 },
                listening_since: None,
                last_packet_at: None,
                mismatch: None,
                test_mode: false,
                account: None,
                upstream: UpstreamStatus::SignedOut,
                pairing: PairingView::Idle,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep going with it.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds a row at the top, assigning its id, and drops the oldest past [`STREAM_LIMIT`].
    pub fn push_row(&self, mut row: StreamRow) -> StreamRow {
        let mut inner = self.lock();
        row.id = inner.next_row_id;
        inner.next_row_id += 1;
        inner.rows.push_front(row.clone());
        inner.rows.truncate(STREAM_LIMIT);
        row
    }

    /// Sets a row's status. `None` when the row has left the stream.
    pub fn set_row_status(&self, row_id: u64, status: RowStatus) -> Option<StreamRow> {
        let mut inner = self.lock();
        let row = inner.rows.iter_mut().find(|r| r.id == row_id)?;
        row.status_text = status.text();
        row.status = status;
        Some(row.clone())
    }

    pub fn remove_row(&self, row_id: u64) -> bool {
        let mut inner = self.lock();
        let before = inner.rows.len();
        inner.rows.retain(|r| r.id != row_id);
        inner.rows.len() != before
    }

    /// The newest row for `external_id` of `kind` received at or after `since`.
    pub fn find_recent_row(
        &self,
        external_id: &str,
        kind: ContactKind,
        since: DateTime<Utc>,
    ) -> Option<StreamRow> {
        self.lock()
            .rows
            .iter()
            .find(|r| {
                r.kind == Some(kind)
                    && r.external_id.as_deref() == Some(external_id)
                    && r.received_at >= since
            })
            .cloned()
    }

    /// Newest first.
    pub fn rows(&self) -> Vec<StreamRow> {
        self.lock().rows.iter().cloned().collect()
    }

    pub fn record_packet(&self, at: DateTime<Utc>) {
        self.lock().last_packet_at = Some(at);
    }

    pub fn set_listener(&self, status: ListenerStatus, now: DateTime<Utc>) {
        let mut inner = self.lock();
        inner.listening_since = matches!(status, ListenerStatus::Listening { .. }).then_some(now);
        inner.listener = status;
    }

    pub fn listener(&self) -> ListenerStatus {
        self.lock().listener.clone()
    }

    pub fn set_mismatch(&self, mycall: Option<String>) {
        self.lock().mismatch = mycall;
    }

    pub fn set_test_mode(&self, on: bool) {
        self.lock().test_mode = on;
    }

    pub fn test_mode(&self) -> bool {
        self.lock().test_mode
    }

    /// Sets the account. A new or cleared account clears the callsign warning.
    pub fn set_account(&self, account: Option<Account>) {
        let mut inner = self.lock();
        if inner.account != account {
            inner.mismatch = None;
        }
        inner.account = account;
    }

    pub fn account(&self) -> Option<Account> {
        self.lock().account.clone()
    }

    /// Sets the upstream status; returns true when it changed.
    pub fn set_upstream(&self, status: UpstreamStatus) -> bool {
        let mut inner = self.lock();
        let changed = inner.upstream != status;
        inner.upstream = status;
        changed
    }

    pub fn upstream(&self) -> UpstreamStatus {
        self.lock().upstream.clone()
    }

    /// Sets the sign-in screen's state; returns true when it changed.
    pub fn set_pairing(&self, view: PairingView) -> bool {
        let mut inner = self.lock();
        let changed = inner.pairing != view;
        inner.pairing = view;
        changed
    }

    pub fn pairing(&self) -> PairingView {
        self.lock().pairing.clone()
    }

    pub fn feed_view(&self, dropped_packets: u64) -> FeedView {
        let inner = self.lock();
        let receiving = match (inner.listening_since, inner.last_packet_at) {
            (Some(since), Some(last)) => last >= since,
            _ => false,
        };
        let status = if matches!(inner.listener, ListenerStatus::PortInUse { .. }) {
            FeedStatus::PortInUse
        } else if inner.test_mode {
            FeedStatus::TestMode
        } else if inner.mismatch.is_some() {
            FeedStatus::IgnoringContacts
        } else if receiving {
            FeedStatus::Receiving
        } else {
            FeedStatus::Waiting
        };
        FeedView {
            status,
            label: status.label().to_string(),
            port: inner.listener.port(),
            listener: inner.listener.clone(),
            last_packet_at: inner.last_packet_at,
            dropped_packets,
            mismatch: inner.mismatch.clone(),
            test_mode: inner.test_mode,
        }
    }

    pub fn snapshot(&self, dropped_packets: u64) -> Snapshot {
        Snapshot {
            feed: self.feed_view(dropped_packets),
            account: self.account(),
            upstream: self.upstream(),
            pairing: self.pairing(),
            stream: self.rows(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 14, min, 0).unwrap()
    }

    fn row(call: &str) -> StreamRow {
        StreamRow::for_contact(
            ContactKind::Info,
            call,
            Some(7_025_000),
            "CW",
            Some(at(22)),
            at(23),
            RowStatus::Waiting,
        )
    }

    #[test]
    fn rows_are_newest_first_and_capped() {
        let state = RelayState::new();
        for i in 0..60 {
            state.push_row(row(&format!("W{i}AW")));
        }
        let rows = state.rows();
        assert_eq!(rows.len(), STREAM_LIMIT);
        assert_eq!(rows[0].call, "W59AW");
        assert_eq!(rows[STREAM_LIMIT - 1].call, "W10AW");
        assert!(rows[0].id > rows[1].id);
    }

    #[test]
    fn row_title_and_time() {
        let r = row("W4ABC");
        assert_eq!(r.title, "W4ABC \u{b7} 40 m CW");
        assert_eq!(r.time_utc, "14:22 UTC");
        assert_eq!(r.status_text, "Waiting to send");
        let out_of_band = StreamRow::for_contact(
            ContactKind::Info,
            "W1AW",
            Some(14_500_500),
            "CW",
            None,
            at(5),
            RowStatus::Sent,
        );
        assert_eq!(out_of_band.title, "W1AW \u{b7} 14.5005 MHz CW");
        assert_eq!(out_of_band.time_utc, "14:05 UTC");
        assert_eq!(StreamRow::unreadable(at(1)).title, "");
    }

    #[test]
    fn status_updates_reach_the_row() {
        let state = RelayState::new();
        let r = state.push_row(row("W4ABC"));
        let updated = state.set_row_status(r.id, RowStatus::Sent).unwrap();
        assert_eq!(updated.status_text, "Sent as a spot");
        assert!(state.set_row_status(9999, RowStatus::Sent).is_none());
        assert!(state.remove_row(r.id));
        assert!(!state.remove_row(r.id));
    }

    #[test]
    fn feed_status_priority() {
        let state = RelayState::new();
        state.set_listener(ListenerStatus::Listening { port: 12060 }, at(0));
        assert_eq!(state.feed_view(0).status, FeedStatus::Waiting);
        assert_eq!(state.feed_view(0).label, "Waiting for logging software");
        state.record_packet(at(1));
        assert_eq!(state.feed_view(0).status, FeedStatus::Receiving);
        state.set_mismatch(Some("K4XYZ".into()));
        assert_eq!(state.feed_view(0).status, FeedStatus::IgnoringContacts);
        state.set_test_mode(true);
        assert_eq!(state.feed_view(0).status, FeedStatus::TestMode);
        state.set_listener(ListenerStatus::PortInUse { port: 12060 }, at(2));
        let view = state.feed_view(3);
        assert_eq!(view.status, FeedStatus::PortInUse);
        assert_eq!(view.port, 12060);
        assert_eq!(view.dropped_packets, 3);
    }

    #[test]
    fn a_packet_before_a_restart_is_not_receiving() {
        let state = RelayState::new();
        state.set_listener(ListenerStatus::Listening { port: 12060 }, at(0));
        state.record_packet(at(1));
        state.set_listener(ListenerStatus::Listening { port: 12061 }, at(2));
        assert_eq!(state.feed_view(0).status, FeedStatus::Waiting);
    }

    #[test]
    fn test_mode_is_off_at_launch_and_account_change_clears_mismatch() {
        let state = RelayState::new();
        assert!(!state.test_mode());
        state.set_mismatch(Some("K4XYZ".into()));
        state.set_account(Some(Account {
            callsign: "KK4PWJ".into(),
            email: "op@example.com".into(),
        }));
        assert_eq!(state.feed_view(0).mismatch, None);
    }

    #[test]
    fn pairing_starts_idle_and_reports_changes() {
        let state = RelayState::new();
        assert_eq!(state.pairing(), PairingView::Idle);
        assert!(state.set_pairing(PairingView::Starting));
        assert!(!state.set_pairing(PairingView::Starting));
        assert_eq!(state.snapshot(0).pairing, PairingView::Starting);
    }

    #[test]
    fn find_recent_row_matches_kind_id_and_time() {
        let state = RelayState::new();
        let mut r = StreamRow::for_contact(
            ContactKind::Delete,
            "W1AW",
            None,
            "",
            Some(at(1)),
            at(10),
            RowStatus::Waiting,
        );
        r.external_id = Some("abc".into());
        let pushed = state.push_row(r);
        assert_eq!(
            state
                .find_recent_row("abc", ContactKind::Delete, at(9))
                .map(|r| r.id),
            Some(pushed.id)
        );
        assert!(state
            .find_recent_row("abc", ContactKind::Delete, at(11))
            .is_none());
        assert!(state
            .find_recent_row("abc", ContactKind::Info, at(9))
            .is_none());
        assert!(state
            .find_recent_row("xyz", ContactKind::Delete, at(9))
            .is_none());
    }
}
