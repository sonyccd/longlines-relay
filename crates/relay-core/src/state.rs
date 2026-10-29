//! What the UI shows: the feed card, the account, the upstream connection and the last 50
//! stream rows, plus the DX cluster card and its last 50 spots sent. In memory only; test mode
//! is always off at launch.

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
/// Spots sent keeps this many rows.
pub const SENT_LIMIT: usize = 50;
/// The address logging software on this computer connects to.
pub const LOOPBACK_ADDRESS: &str = "127.0.0.1";

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
    /// The DX cluster tab.
    pub cluster: ClusterView,
    /// Spots sent to logging software, newest first.
    pub sent: Vec<SentRow>,
}

/// The DX cluster card's chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterStatus {
    /// The switch is off; nothing listens.
    Off,
    /// Listening; no spot sent since it started.
    Listening,
    /// Listening, and a spot has been sent since it started.
    Running,
    /// No port could be bound.
    PortInUse,
    /// The token lacks `spots:read`: sign out and pair again (contract 3.1).
    ScopeMissing,
}

impl ClusterStatus {
    pub fn label(self) -> &'static str {
        match self {
            ClusterStatus::Off => "Off",
            ClusterStatus::Listening => "Listening",
            ClusterStatus::Running => "Running",
            ClusterStatus::PortInUse => "Port in use",
            ClusterStatus::ScopeMissing => "Sign in again",
        }
    }
}

/// The spots stream's state (contract section 9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClusterUpstream {
    /// Closed: the switch is off, signed out, or the token can't read spots.
    Off,
    /// Opening, before `hello`.
    Connecting,
    /// Open; `hello` arrived.
    Connected,
    /// Closed after a failure; the next try is at `at`.
    Retrying { at: DateTime<Utc> },
}

/// Where logging software connects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClusterAddresses {
    /// Always `127.0.0.1`.
    pub local: String,
    /// This computer's network address, only while other computers may connect.
    pub lan: Option<String>,
}

/// Everything the DX cluster card shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClusterView {
    /// The DX cluster switch.
    pub enabled: bool,
    pub status: ClusterStatus,
    /// The chip's words.
    pub label: String,
    pub listener: ListenerStatus,
    /// The bound port, else the configured one.
    pub port: u16,
    /// "Let other computers on this network connect".
    pub lan: bool,
    pub addresses: ClusterAddresses,
    pub last_sent_at: Option<DateTime<Utc>>,
    /// Spots sent since midnight UTC, test spots included.
    pub sent_today: u64,
    /// Programs connected to the cluster.
    pub clients: usize,
    pub upstream: ClusterUpstream,
}

/// One row of Spots sent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SentRow {
    pub id: u64,
    /// The spot's id in Long Lines; `None` for a test spot.
    pub spot_id: Option<u64>,
    pub callsign: String,
    pub frequency_khz: f64,
    pub mode: Option<String>,
    /// Program first (`POTA K-0039`).
    pub reference: Option<String>,
    /// The connection that matched it.
    pub connection: String,
    pub source_label: String,
    pub sent_at: DateTime<Utc>,
    /// "K4ABC · 14062.0 kHz CW".
    pub title: String,
    /// "POTA K-0039 · from POTA states, CW", or "Test spot sent from this relay".
    pub detail: String,
    /// "21:14 UTC": the spot's time.
    pub time_utc: String,
    /// The relay's own test spot.
    pub test: bool,
}

impl SentRow {
    /// A row for a spot from the stream. The id is assigned by [`RelayState::push_sent`].
    pub fn for_spot(spot: &crate::downstream::Spot, sent_at: DateTime<Utc>) -> Self {
        let mode = spot.mode.clone().filter(|m| !m.is_empty());
        let reference = spot.reference.clone().filter(|r| !r.is_empty());
        let mut from = (!spot.connection.is_empty()).then(|| format!("from {}", spot.connection));
        if let (Some(from), Some(mode)) = (from.as_mut(), mode.as_deref()) {
            from.push_str(", ");
            from.push_str(mode);
        }
        let detail = [reference.clone(), from]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        Self {
            id: 0,
            spot_id: Some(spot.id),
            callsign: spot.callsign.clone(),
            frequency_khz: spot.frequency_khz,
            title: sent_title(&spot.callsign, spot.frequency_khz, mode.as_deref()),
            mode,
            reference,
            connection: spot.connection.clone(),
            source_label: spot.source_label.clone(),
            sent_at,
            detail,
            time_utc: hhmm_label(&spot.time_utc).unwrap_or_else(|| utc_label(sent_at)),
            test: false,
        }
    }

    /// A row for the relay's test spot (contract 11.4).
    pub fn test_spot(
        callsign: &str,
        frequency_khz: f64,
        mode: &str,
        sent_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: 0,
            spot_id: None,
            callsign: callsign.to_string(),
            frequency_khz,
            mode: Some(mode.to_string()),
            reference: None,
            connection: String::new(),
            source_label: String::new(),
            sent_at,
            title: sent_title(callsign, frequency_khz, Some(mode)),
            detail: "Test spot sent from this relay".into(),
            time_utc: utc_label(sent_at),
            test: true,
        }
    }
}

/// "K4ABC · 14062.0 kHz CW".
fn sent_title(callsign: &str, frequency_khz: f64, mode: Option<&str>) -> String {
    match mode {
        Some(mode) => format!("{callsign} \u{b7} {frequency_khz:.1} kHz {mode}"),
        None => format!("{callsign} \u{b7} {frequency_khz:.1} kHz"),
    }
}

fn utc_label(t: DateTime<Utc>) -> String {
    t.format("%H:%M UTC").to_string()
}

/// `2114` as "21:14 UTC"; `None` unless it is four digits.
fn hhmm_label(hhmm: &str) -> Option<String> {
    (hhmm.len() == 4 && hhmm.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}:{} UTC", &hhmm[..2], &hhmm[2..]))
}

#[derive(Debug)]
struct Cluster {
    enabled: bool,
    lan: bool,
    lan_address: Option<String>,
    listener: ListenerStatus,
    /// When it last started listening (or was switched on); a spot sent since makes it Running.
    since: Option<DateTime<Utc>>,
    scope_missing: bool,
    upstream: ClusterUpstream,
    clients: usize,
    last_sent_at: Option<DateTime<Utc>>,
    /// The UTC day `sent_today` counts.
    sent_day: Option<chrono::NaiveDate>,
    sent_today: u64,
    rows: VecDeque<SentRow>,
    next_row_id: u64,
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
    cluster: Cluster,
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
                cluster: Cluster {
                    enabled: false,
                    lan: false,
                    lan_address: None,
                    listener: ListenerStatus::Stopped {
                        port: crate::settings::DEFAULT_CLUSTER_PORT,
                    },
                    since: None,
                    scope_missing: false,
                    upstream: ClusterUpstream::Off,
                    clients: 0,
                    last_sent_at: None,
                    sent_day: None,
                    sent_today: 0,
                    rows: VecDeque::with_capacity(SENT_LIMIT),
                    next_row_id: 1,
                },
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

    // ---- DX cluster ----

    /// Turns the switch on or off; returns true when it changed.
    pub fn set_cluster_enabled(&self, on: bool, now: DateTime<Utc>) -> bool {
        let mut inner = self.lock();
        let changed = inner.cluster.enabled != on;
        inner.cluster.enabled = on;
        if changed && on {
            inner.cluster.since = Some(now);
        }
        changed
    }

    pub fn cluster_enabled(&self) -> bool {
        self.lock().cluster.enabled
    }

    /// "Let other computers on this network connect", with the address they use.
    pub fn set_cluster_lan(&self, on: bool, address: Option<String>) {
        let mut inner = self.lock();
        inner.cluster.lan = on;
        inner.cluster.lan_address = address;
    }

    pub fn cluster_lan(&self) -> bool {
        self.lock().cluster.lan
    }

    pub fn set_cluster_listener(&self, status: ListenerStatus, now: DateTime<Utc>) {
        let mut inner = self.lock();
        if matches!(status, ListenerStatus::Listening { .. }) {
            inner.cluster.since = Some(now);
        }
        inner.cluster.listener = status;
    }

    pub fn cluster_listener(&self) -> ListenerStatus {
        self.lock().cluster.listener.clone()
    }

    /// Returns true when it changed.
    pub fn set_scope_missing(&self, missing: bool) -> bool {
        let mut inner = self.lock();
        let changed = inner.cluster.scope_missing != missing;
        inner.cluster.scope_missing = missing;
        changed
    }

    pub fn scope_missing(&self) -> bool {
        self.lock().cluster.scope_missing
    }

    /// Returns true when it changed.
    pub fn set_cluster_upstream(&self, status: ClusterUpstream) -> bool {
        let mut inner = self.lock();
        let changed = inner.cluster.upstream != status;
        inner.cluster.upstream = status;
        changed
    }

    /// Returns true when it changed.
    pub fn set_cluster_clients(&self, clients: usize) -> bool {
        let mut inner = self.lock();
        let changed = inner.cluster.clients != clients;
        inner.cluster.clients = clients;
        changed
    }

    /// Adds a row at the top of Spots sent, assigning its id, and counts it.
    pub fn push_sent(&self, mut row: SentRow) -> SentRow {
        let mut inner = self.lock();
        let cluster = &mut inner.cluster;
        row.id = cluster.next_row_id;
        cluster.next_row_id += 1;
        let day = row.sent_at.date_naive();
        if cluster.sent_day != Some(day) {
            cluster.sent_day = Some(day);
            cluster.sent_today = 0;
        }
        cluster.sent_today += 1;
        cluster.last_sent_at = Some(row.sent_at);
        cluster.rows.push_front(row.clone());
        cluster.rows.truncate(SENT_LIMIT);
        row
    }

    /// Newest first.
    pub fn sent_rows(&self) -> Vec<SentRow> {
        self.lock().cluster.rows.iter().cloned().collect()
    }

    /// The DX cluster card as of `now` (which decides what "today" is).
    pub fn cluster_view(&self, now: DateTime<Utc>) -> ClusterView {
        let inner = self.lock();
        let c = &inner.cluster;
        let running = match (c.since, c.last_sent_at) {
            (Some(since), Some(last)) => last >= since,
            _ => false,
        };
        let status = if !c.enabled {
            ClusterStatus::Off
        } else if matches!(c.listener, ListenerStatus::PortInUse { .. }) {
            ClusterStatus::PortInUse
        } else if c.scope_missing {
            ClusterStatus::ScopeMissing
        } else if running {
            ClusterStatus::Running
        } else {
            ClusterStatus::Listening
        };
        ClusterView {
            enabled: c.enabled,
            status,
            label: status.label().to_string(),
            listener: c.listener.clone(),
            port: c.listener.port(),
            lan: c.lan,
            addresses: ClusterAddresses {
                local: LOOPBACK_ADDRESS.to_string(),
                lan: if c.lan { c.lan_address.clone() } else { None },
            },
            last_sent_at: c.last_sent_at,
            sent_today: if c.sent_day == Some(now.date_naive()) {
                c.sent_today
            } else {
                0
            },
            clients: c.clients,
            upstream: c.upstream.clone(),
        }
    }

    pub fn snapshot(&self, dropped_packets: u64) -> Snapshot {
        self.snapshot_at(dropped_packets, Utc::now())
    }

    /// The snapshot as of `now` (tests pin the day).
    pub fn snapshot_at(&self, dropped_packets: u64, now: DateTime<Utc>) -> Snapshot {
        Snapshot {
            feed: self.feed_view(dropped_packets),
            account: self.account(),
            upstream: self.upstream(),
            pairing: self.pairing(),
            stream: self.rows(),
            cluster: self.cluster_view(now),
            sent: self.sent_rows(),
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

    fn spot() -> crate::downstream::Spot {
        crate::downstream::Spot::from_data(
            r#"{"id":48213377,"spotter":"W4DOG","frequency_khz":14062.0,"callsign":"K4ABC","comment":"CW POTA K-0039","time_utc":"2114","mode":"CW","reference":"POTA K-0039","source":"pota","source_label":"POTA","connection":"POTA states","spot_time":"2026-10-28T21:14:07Z"}"#,
        )
        .unwrap()
    }

    #[test]
    fn sent_rows_read_as_the_mocks() {
        let row = SentRow::for_spot(&spot(), at(30));
        assert_eq!(row.title, "K4ABC \u{b7} 14062.0 kHz CW");
        assert_eq!(row.detail, "POTA K-0039 \u{b7} from POTA states, CW");
        assert_eq!(row.time_utc, "21:14 UTC");
        assert!(!row.test);
        let mut bare = spot();
        bare.mode = None;
        bare.reference = None;
        bare.time_utc = "late".into();
        let row = SentRow::for_spot(&bare, at(30));
        assert_eq!(row.title, "K4ABC \u{b7} 14062.0 kHz");
        assert_eq!(row.detail, "from POTA states");
        assert_eq!(row.time_utc, "14:30 UTC");
        let test = SentRow::test_spot("LLTEST", 14050.0, "CW", at(2));
        assert_eq!(test.title, "LLTEST \u{b7} 14050.0 kHz CW");
        assert_eq!(test.detail, "Test spot sent from this relay");
        assert_eq!(test.time_utc, "14:02 UTC");
        assert!(test.test);
    }

    #[test]
    fn cluster_status_priority_and_counts() {
        let state = RelayState::new();
        let view = state.cluster_view(at(0));
        assert_eq!(view.status, ClusterStatus::Off);
        assert_eq!(view.label, "Off");
        assert_eq!(view.port, 7373);
        assert_eq!(view.addresses.local, "127.0.0.1");
        assert_eq!(view.upstream, ClusterUpstream::Off);

        assert!(state.set_cluster_enabled(true, at(1)));
        assert!(!state.set_cluster_enabled(true, at(1)));
        state.set_cluster_listener(ListenerStatus::Listening { port: 7374 }, at(1));
        let view = state.cluster_view(at(1));
        assert_eq!(view.status, ClusterStatus::Listening);
        assert_eq!(view.port, 7374);

        state.push_sent(SentRow::for_spot(&spot(), at(2)));
        state.push_sent(SentRow::test_spot("LLTEST", 14050.0, "CW", at(3)));
        let view = state.cluster_view(at(4));
        assert_eq!(view.status, ClusterStatus::Running);
        assert_eq!(view.sent_today, 2);
        assert_eq!(view.last_sent_at, Some(at(3)));
        let rows = state.sent_rows();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].test && rows[0].id > rows[1].id);
        // A new day starts the count again.
        assert_eq!(
            state
                .cluster_view(at(4) + chrono::TimeDelta::days(1))
                .sent_today,
            0
        );

        // A restart of the listener is Listening until the next spot.
        state.set_cluster_listener(ListenerStatus::Listening { port: 7373 }, at(5));
        assert_eq!(state.cluster_view(at(5)).status, ClusterStatus::Listening);

        assert!(state.set_scope_missing(true));
        assert_eq!(
            state.cluster_view(at(5)).status,
            ClusterStatus::ScopeMissing
        );
        state.set_cluster_listener(ListenerStatus::PortInUse { port: 7373 }, at(6));
        assert_eq!(state.cluster_view(at(6)).status, ClusterStatus::PortInUse);
        state.set_cluster_enabled(false, at(7));
        assert_eq!(state.cluster_view(at(7)).status, ClusterStatus::Off);

        state.set_cluster_lan(true, Some("192.168.1.42".into()));
        assert_eq!(
            state.cluster_view(at(7)).addresses.lan.as_deref(),
            Some("192.168.1.42")
        );
        state.set_cluster_lan(false, Some("192.168.1.42".into()));
        assert_eq!(state.cluster_view(at(7)).addresses.lan, None);
        assert!(state.set_cluster_clients(3));
        assert_eq!(state.cluster_view(at(7)).clients, 3);
    }

    #[test]
    fn sent_rows_are_capped() {
        let state = RelayState::new();
        for _ in 0..60 {
            state.push_sent(SentRow::for_spot(&spot(), at(1)));
        }
        assert_eq!(state.sent_rows().len(), SENT_LIMIT);
        assert_eq!(state.cluster_view(at(1)).sent_today, 60);
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
