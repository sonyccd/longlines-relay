//! The orchestrator: listener -> parser -> checks -> queue -> sender.
//!
//! Two tasks run per engine. The pipeline turns each datagram into a stream row and, when the
//! contact passes every check, a queued item; it only touches memory and the local SQLite file,
//! so network trouble never slows UDP receive. The sender takes the oldest queued items, one
//! request at a time, and turns each answer into the row's final status.
//!
//! A third task holds the spots stream ([`crate::downstream::stream`]) open while the DX cluster
//! switch is on, and hands each spot to the local DX cluster ([`crate::cluster`]), which serves
//! it as one line to every logged-in program.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::{broadcast, mpsc, watch, Notify};
use tokio::task::JoinHandle;

use crate::bands::band_for_hz;
use crate::bus::{AuthEvent, Bus, Event, SignOutReason};
use crate::callsign::same_station;
use crate::cluster::line::format_spot;
use crate::cluster::server::{ClusterHub, ClusterServer};
use crate::downstream::stream::{self, StreamClient, StreamConfig, StreamHost, Target};
use crate::downstream::{Hello, Spot, NO_SPOTTER};
use crate::external_id::{external_id, HashFields};
use crate::freshness::{classify, Freshness};
use crate::listeners::udp::UdpListener;
use crate::listeners::{Datagram, Listener, ListenerStatus};
use crate::n1mm::{self, parse_timestamp, ContactKind, LoggedContact, Packet};
use crate::settings::DEFAULT_CLUSTER_PORT;
use crate::source_filter::SourceFilter;
use crate::state::{
    Account, ClusterUpstream, ClusterView, RelayState, SentRow, Snapshot, StreamRow, UpstreamStatus,
};
use crate::status::{reasons, RowStatus};
use crate::upstream::auth::{self, PairingFailure, PairingView};
use crate::upstream::client::{
    ClientConfig, ClientError, ItemStatus, Me, RejectReason, RelayClient, SPOTS_READ,
};
use crate::upstream::queue::{Backoff, DropReason, NextBatch, Queue, QueueError, QueueItem};
use crate::upstream::token_store::{Token, TokenStore, TokenStoreError};
use crate::upstream::{Op, QueuedContact, WireItem};

/// The most items in one `POST /contacts`.
pub const MAX_BATCH: usize = 100;
/// The client keeps to this many contacts a minute without waiting for a 429.
pub const CONTACTS_PER_MINUTE: usize = 120;
/// How often `GET /me` runs while signed in.
pub const ACCOUNT_REFRESH: Duration = Duration::from_secs(3600);
/// A `contactreplace` this soon after a `contactdelete` for the same ID is one N1MM edit.
pub const EDIT_WINDOW: Duration = Duration::from_secs(10);
/// The sender wakes at least this often, to expire queued contacts while offline.
pub const IDLE_TICK: Duration = Duration::from_secs(5);
/// One second of the pairing protocol (`interval`, `expires_in`). Tests shorten it.
pub const PAIRING_SECOND: Duration = Duration::from_secs(1);
/// While the port is in use, binding it is tried again this often.
pub const PORT_RETRY: Duration = Duration::from_secs(5);
/// Next to the queue: present while a signed-out token is still in the credential store.
const SIGN_OUT_MARKER: &str = "sign-out-pending";
/// The longest callsign the server accepts.
const MAX_CALL_LEN: usize = 64;
/// The longest mode the server accepts.
const MAX_MODE_LEN: usize = 32;
/// The longest `logger_app` the server accepts.
const MAX_APP_LEN: usize = 64;
/// Datagrams waiting for the pipeline.
const DATAGRAM_BACKLOG: usize = 1024;
/// A spot id served once is not served again for this long (contract 9.4).
pub const SPOT_REPEAT_WINDOW: Duration = Duration::from_secs(600);
/// The test spot (contract 11.4).
pub const TEST_SPOT_CALLSIGN: &str = "LLTEST";
pub const TEST_SPOT_FREQUENCY_KHZ: f64 = 14050.0;
pub const TEST_SPOT_MODE: &str = "CW";
pub const TEST_SPOT_COMMENT: &str = "CW Long Lines Relay test spot";

/// How to build an engine. [`EngineConfig::new`] fills in the contract's defaults.
#[derive(Debug)]
pub struct EngineConfig {
    /// The UDP port; 0 binds an ephemeral port (tests).
    pub port: u16,
    pub queue_path: PathBuf,
    pub client: ClientConfig,
    pub token_store: Arc<dyn TokenStore>,
    pub source_filter: Arc<SourceFilter>,
    pub backoff: Backoff,
    pub max_batch: usize,
    pub contacts_per_minute: usize,
    pub account_refresh: Duration,
    pub edit_window: Duration,
    pub idle_tick: Duration,
    pub pairing_second: Duration,
    /// The DX cluster switch at launch (off by default).
    pub cluster_enabled: bool,
    /// The DX cluster's port; only the default falls through to the next ports. 0 binds an
    /// ephemeral port (tests).
    pub cluster_port: u16,
    /// "Let other computers on this network connect".
    pub cluster_lan: bool,
    /// The greeting's device name until `GET /me` gives the paired one.
    pub device_name: String,
    /// The spots stream's silence limit (contract 9.5). The stream's backoff is `backoff`.
    pub keepalive_timeout: Duration,
    /// How long a stream stays open after `hello` before the backoff starts over.
    pub stream_stable_after: Duration,
    pub port_retry: Duration,
}

impl EngineConfig {
    pub fn new(
        port: u16,
        queue_path: PathBuf,
        client: ClientConfig,
        token_store: Arc<dyn TokenStore>,
    ) -> Self {
        Self {
            port,
            queue_path,
            client,
            token_store,
            source_filter: Arc::new(SourceFilter::system()),
            backoff: Backoff::DEFAULT,
            max_batch: MAX_BATCH,
            contacts_per_minute: CONTACTS_PER_MINUTE,
            account_refresh: ACCOUNT_REFRESH,
            edit_window: EDIT_WINDOW,
            idle_tick: IDLE_TICK,
            pairing_second: PAIRING_SECOND,
            cluster_enabled: false,
            cluster_port: DEFAULT_CLUSTER_PORT,
            cluster_lan: false,
            device_name: auth::FALLBACK_DEVICE_NAME.to_string(),
            keepalive_timeout: stream::KEEPALIVE_TIMEOUT,
            stream_stable_after: stream::STABLE_AFTER,
            port_retry: PORT_RETRY,
        }
    }
}

/// Why the test spot was not sent.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClusterError {
    #[error("The DX cluster isn't listening.")]
    NotListening,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error(transparent)]
    Client(#[from] ClientError),
}

/// Why signing in with a new token failed.
#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error(transparent)]
    Store(#[from] TokenStoreError),
    #[error(transparent)]
    Client(#[from] ClientError),
}

#[derive(Debug)]
struct Shared {
    state: RelayState,
    bus: Bus,
    queue: Queue,
    client: RelayClient,
    tokens: Arc<dyn TokenStore>,
    filter: Arc<SourceFilter>,
    /// Held across a port change, so the old socket is gone before the new one binds.
    listener: tokio::sync::Mutex<UdpListener>,
    /// See [`SIGN_OUT_MARKER`].
    sign_out_marker: PathBuf,
    wake: Notify,
    /// Which stream row each queued item belongs to.
    rows_by_seq: Mutex<HashMap<i64, u64>>,
    /// When `GET /me` last answered; `None` forces a refresh.
    account_checked_at: Mutex<Option<Instant>>,
    backoff: Backoff,
    max_batch: usize,
    contacts_per_minute: usize,
    account_refresh: Duration,
    edit_window: TimeDelta,
    idle_tick: Duration,
    pairing_second: Duration,
    port_retry: Duration,
    /// Bumped by every sign-in start and cancel, so a stale attempt never shows.
    pairing_attempt: AtomicU64,
    /// The latest `GET /me`, for the spots stream's URL and scopes.
    me: Mutex<Option<Me>>,
    /// The scopes the last pairing granted, until `GET /me` lists them.
    granted_scopes: Mutex<Option<Vec<String>>>,
    /// Moves whenever the spots stream's target may have changed.
    stream_changes: watch::Sender<u64>,
    /// Spot ids served lately, so a repeat is dropped (contract 9.4).
    served: Mutex<HashMap<u64, Instant>>,
    /// The stream answered 403 for the stored token: never again with it (contract 3.1), whatever
    /// a later `GET /me` says. Cleared by signing in or out.
    scope_refused: AtomicBool,
    /// The DX cluster's spot lines and connection count.
    hub: Arc<ClusterHub>,
}

/// The running relay core.
#[derive(Debug)]
pub struct Engine {
    shared: Arc<Shared>,
    /// The DX cluster's listener; replaced when its port or bind address changes.
    cluster: Mutex<Arc<ClusterServer>>,
    /// Serialises the DX cluster's switch, port and network changes: each one waits for the old
    /// listener to let go of its port before it binds again.
    cluster_changes: tokio::sync::Mutex<()>,
    datagrams: mpsc::Sender<Datagram>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// The token poll of the sign-in in progress.
    pairing_task: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    /// Opens the queue, binds the port and starts the pipeline and the sender. Must be called
    /// inside a Tokio runtime. A port in use is not an error: the feed shows it.
    pub async fn start(config: EngineConfig) -> Result<Self, EngineError> {
        let queue = Queue::open(&config.queue_path, config.backoff)?;
        let stream_config = StreamConfig {
            backoff: config.backoff,
            keepalive_timeout: config.keepalive_timeout,
            stable_after: config.stream_stable_after,
            ..StreamConfig::new(config.client.relay_version.clone())
        };
        let stream_client = StreamClient::new(&stream_config)?;
        let client = RelayClient::new(config.client)?;
        let (tx, rx) = mpsc::channel(DATAGRAM_BACKLOG);
        let listener = UdpListener::new(config.port, Arc::clone(&config.source_filter), tx.clone());
        let shared = Arc::new(Shared {
            state: RelayState::new(),
            bus: Bus::default(),
            queue,
            client,
            tokens: config.token_store,
            filter: config.source_filter,
            listener: tokio::sync::Mutex::new(listener),
            sign_out_marker: config.queue_path.with_file_name(SIGN_OUT_MARKER),
            wake: Notify::new(),
            rows_by_seq: Mutex::new(HashMap::new()),
            account_checked_at: Mutex::new(None),
            backoff: config.backoff,
            max_batch: config.max_batch.clamp(1, MAX_BATCH),
            contacts_per_minute: config.contacts_per_minute.max(1),
            account_refresh: config.account_refresh,
            edit_window: TimeDelta::from_std(config.edit_window).unwrap_or(TimeDelta::seconds(10)),
            idle_tick: config.idle_tick,
            pairing_second: config.pairing_second,
            port_retry: config.port_retry,
            pairing_attempt: AtomicU64::new(0),
            me: Mutex::new(None),
            granted_scopes: Mutex::new(None),
            stream_changes: watch::Sender::new(0),
            served: Mutex::new(HashMap::new()),
            scope_refused: AtomicBool::new(false),
            hub: ClusterHub::new(config.device_name),
        });
        shared
            .state
            .set_cluster_enabled(config.cluster_enabled, Utc::now());
        shared
            .state
            .set_cluster_lan(config.cluster_lan, lan_address_if(config.cluster_lan));
        let cluster = ClusterServer::new(
            Arc::clone(&shared.hub),
            config.cluster_port,
            config.cluster_lan,
        );
        let cluster_status = if config.cluster_enabled {
            cluster.start()
        } else {
            cluster.status()
        };
        shared
            .state
            .set_cluster_listener(cluster_status, Utc::now());
        shared.restore_rows()?;
        if shared.sign_out_marker.exists() {
            // Signed out last run, but the credential store kept the token.
            shared.state.set_sign_out_incomplete(true);
            shared.retry_token_delete();
        }
        if !shared.state.sign_out_incomplete() {
            match shared.tokens.load() {
                Ok(Some(_)) => {
                    shared.state.set_upstream(UpstreamStatus::Connected);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "couldn't read the token; trying again");
                    shared
                        .state
                        .set_upstream(UpstreamStatus::CredentialStoreUnavailable);
                }
            }
        }

        let status = shared.listener.lock().await.start();
        shared.state.set_listener(status, Utc::now());
        let tasks = vec![
            tokio::spawn(pipeline(Arc::clone(&shared), rx)),
            tokio::spawn(sender(Arc::clone(&shared))),
            tokio::spawn(stream::run(
                Arc::clone(&shared),
                stream_client,
                stream_config,
                shared.stream_changes.subscribe(),
            )),
            tokio::spawn(watch_clients(Arc::clone(&shared))),
            tokio::spawn(rebind(Arc::clone(&shared))),
        ];
        shared.publish_feed();
        Ok(Self {
            shared,
            cluster: Mutex::new(Arc::new(cluster)),
            cluster_changes: tokio::sync::Mutex::new(()),
            datagrams: tx,
            tasks: Mutex::new(tasks),
            pairing_task: Mutex::new(None),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.shared.bus.subscribe()
    }

    /// Everything the window shows.
    pub fn snapshot(&self) -> Snapshot {
        self.shared.state.snapshot(self.shared.filter.dropped())
    }

    /// The API client, for pairing.
    pub fn client(&self) -> RelayClient {
        self.shared.client.clone()
    }

    /// Turns test mode on or off. Contacts seen while it is on are never queued.
    pub fn set_test_mode(&self, on: bool) {
        self.shared.state.set_test_mode(on);
        self.shared.publish_feed();
    }

    /// The current DX cluster listener. Never held across an await.
    fn cluster(&self) -> Arc<ClusterServer> {
        Arc::clone(&self.cluster.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The DX cluster switch: on listens and holds the spots stream open, off closes every
    /// connection and the stream. The caller saves the setting.
    pub async fn set_cluster_enabled(&self, on: bool) -> ClusterView {
        let _changing = self.cluster_changes.lock().await;
        self.shared.state.set_cluster_enabled(on, Utc::now());
        let cluster = self.cluster();
        let status = match (on, cluster.status()) {
            (true, status @ ListenerStatus::Listening { .. }) => status,
            (true, _) => cluster.start(),
            (false, _) => {
                cluster.close().await;
                cluster.status()
            }
        };
        let now = Utc::now();
        self.shared.state.set_cluster_listener(status, now);
        self.shared.stream_changed();
        self.shared.publish_cluster();
        self.shared.state.cluster_view(now)
    }

    /// Replaces the DX cluster's listener (closing its connections and waiting for its port to
    /// be free) and starts the new one when the switch is on. The caller holds
    /// `cluster_changes`.
    async fn rebind_cluster(&self, port: u16, lan: bool) -> ClusterView {
        self.cluster().close().await;
        let cluster = Arc::new(ClusterServer::new(Arc::clone(&self.shared.hub), port, lan));
        let status = if self.shared.state.cluster_enabled() {
            cluster.start()
        } else {
            cluster.status()
        };
        *self.cluster.lock().unwrap_or_else(|e| e.into_inner()) = cluster;
        let now = Utc::now();
        self.shared.state.set_cluster_listener(status, now);
        self.shared.publish_cluster();
        self.shared.state.cluster_view(now)
    }

    /// "Change port". A port the user chose is used as is; only the default falls through. The
    /// caller saves the setting.
    pub async fn set_cluster_port(&self, port: u16) -> ClusterView {
        let _changing = self.cluster_changes.lock().await;
        let lan = self.shared.state.cluster_lan();
        self.rebind_cluster(port, lan).await
    }

    /// "Let other computers on this network connect": rebinds on every IPv4 interface, or on
    /// loopback only, on the same port. The caller saves the setting.
    pub async fn set_cluster_lan(&self, on: bool) -> ClusterView {
        let _changing = self.cluster_changes.lock().await;
        self.shared.state.set_cluster_lan(on, lan_address_if(on));
        let port = self.cluster().port();
        self.rebind_cluster(port, on).await
    }

    /// "Send test spot" (contract 11.4): one line to every logged-in program, without the server.
    pub fn send_test_spot(&self) -> Result<SentRow, ClusterError> {
        if !matches!(
            self.shared.state.cluster_listener(),
            ListenerStatus::Listening { .. }
        ) || !self.shared.state.cluster_enabled()
        {
            return Err(ClusterError::NotListening);
        }
        let now = Utc::now();
        let spotter = self
            .shared
            .state
            .account()
            .map_or_else(|| NO_SPOTTER.to_string(), |a| a.callsign);
        let spot = Spot {
            id: 0,
            spotter,
            frequency_khz: TEST_SPOT_FREQUENCY_KHZ,
            callsign: TEST_SPOT_CALLSIGN.into(),
            comment: TEST_SPOT_COMMENT.into(),
            time_utc: now.format("%H%M").to_string(),
            mode: Some(TEST_SPOT_MODE.into()),
            reference: None,
            source: String::new(),
            source_label: String::new(),
            connection: String::new(),
            spot_time: utc_text(now),
        };
        let clients = self.shared.hub.send_line(format_spot(&spot, now));
        tracing::info!(clients, "sent a test spot");
        let row = self.shared.state.push_sent(SentRow::test_spot(
            TEST_SPOT_CALLSIGN,
            TEST_SPOT_FREQUENCY_KHZ,
            TEST_SPOT_MODE,
            now,
        ));
        self.shared.bus.publish(Event::SpotSent(row.clone()));
        self.shared.publish_cluster();
        Ok(row)
    }

    /// Moves the listener to another port, or tries the same port again when it was in use. The
    /// caller saves the setting.
    pub async fn set_port(&self, port: u16) -> ListenerStatus {
        let mut current = self.shared.listener.lock().await;
        let status = current.status();
        if status == (ListenerStatus::Listening { port }) {
            return status;
        }
        // Wait for the old socket to close, or binding the same port again would fail.
        current.shutdown().await;
        *current = UdpListener::new(
            port,
            Arc::clone(&self.shared.filter),
            self.datagrams.clone(),
        );
        let status = current.start();
        drop(current);
        self.shared.state.set_listener(status.clone(), Utc::now());
        self.shared.publish_feed();
        status
    }

    /// Stores a freshly paired token and reads the account. A refused token is deleted again.
    pub async fn sign_in(&self, token: Token) -> Result<Account, SignInError> {
        self.shared.sign_in(token, None).await
    }

    /// Starts signing in: asks Long Lines for a code and polls for the token in the background.
    /// Returns the waiting view (whose `verification_uri_complete` the shell opens in the
    /// browser), or why it could not start. Starting again replaces an attempt in progress.
    pub async fn start_sign_in(&self, device_name: &str) -> Result<PairingView, PairingFailure> {
        self.abort_pairing_task();
        let attempt = self.shared.pairing_attempt.fetch_add(1, Ordering::SeqCst) + 1;
        self.shared.set_pairing(PairingView::Starting);
        let started =
            auth::start(&self.shared.client, device_name, self.shared.pairing_second).await;
        if self.shared.pairing_attempt.load(Ordering::SeqCst) != attempt {
            // Cancelled (or restarted) while the request was out.
            return Ok(self.shared.state.pairing());
        }
        let pending = match started {
            Ok(pending) => pending,
            Err(failure) => {
                self.shared.set_pairing(PairingView::Failed { failure });
                return Err(failure);
            }
        };
        let view = pending.view();
        self.shared.set_pairing(view.clone());
        let shared = Arc::clone(&self.shared);
        let task = tokio::spawn(async move {
            let outcome =
                auth::wait_for_token(&shared.client, &pending, shared.pairing_second).await;
            if shared.pairing_attempt.load(Ordering::SeqCst) != attempt {
                return;
            }
            let next = match outcome {
                Ok(granted) => match shared.sign_in(granted.token, Some(granted.scopes)).await {
                    Err(SignInError::Client(ClientError::SignedOut)) => PairingView::Failed {
                        failure: PairingFailure::Refused,
                    },
                    Err(SignInError::Store(e)) => {
                        tracing::error!(error = %e, "couldn't store the token");
                        PairingView::Failed {
                            failure: PairingFailure::Refused,
                        }
                    }
                    // Signed in; a network failure reading the account is retried by the sender.
                    Ok(_) | Err(SignInError::Client(_)) => PairingView::Idle,
                },
                Err(failure) => PairingView::Failed { failure },
            };
            shared.set_pairing(next);
        });
        *self.pairing_task.lock().unwrap_or_else(|e| e.into_inner()) = Some(task);
        Ok(view)
    }

    /// Cancel on the sign-in screen: stops polling and goes back to signed out.
    pub fn cancel_sign_in(&self) {
        self.shared.pairing_attempt.fetch_add(1, Ordering::SeqCst);
        self.abort_pairing_task();
        self.shared.set_pairing(PairingView::Idle);
    }

    fn abort_pairing_task(&self) {
        if let Some(task) = self
            .pairing_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.abort();
        }
    }

    /// Deletes the token. Queued contacts stay queued for the next sign-in, which checks their
    /// station callsign against that account before sending.
    ///
    /// When the credential store can't delete the token, the relay stops using it anyway and
    /// keeps trying to delete it, across restarts, until it succeeds or the user signs in again.
    pub fn sign_out(&self) -> Result<(), TokenStoreError> {
        let result = self.shared.tokens.delete();
        if let Err(e) = &result {
            tracing::warn!(error = %e, "couldn't delete the token; will keep trying");
        }
        self.shared.set_sign_out_incomplete(result.is_err());
        self.shared.signed_out(SignOutReason::User);
        result
    }

    /// Asks the sender to call `GET /me` again now.
    pub fn refresh_account(&self) {
        self.shared.mark_account_stale();
        self.shared.wake.notify_one();
    }

    /// Stops both tasks and releases the port. The queue stays on disk.
    pub async fn shutdown(self) {
        self.abort_pairing_task();
        let tasks: Vec<_> =
            std::mem::take(&mut *self.tasks.lock().unwrap_or_else(|e| e.into_inner()));
        self.cluster().close().await;
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
        self.shared.listener.lock().await.shutdown().await;
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.abort_pairing_task();
        for task in self
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            task.abort();
        }
    }
}

/// `now` plus `wait`, without panicking past the calendar's end.
fn later(now: DateTime<Utc>, wait: Duration) -> DateTime<Utc> {
    TimeDelta::from_std(wait)
        .ok()
        .and_then(|d| now.checked_add_signed(d))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn utc_text(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// A call the server accepts (contract 6.1): 1 to 64 characters, none of them whitespace or a
/// control character (Unicode White_Space, U+FEFF and the Cc category, which holds U+0000).
fn valid_call(call: &str) -> bool {
    !call.is_empty()
        && call.chars().count() <= MAX_CALL_LEN
        && !call
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\u{feff}')
}

/// A mode the server accepts: 1 to 32 characters and no control character (U+0000 can't be
/// stored; the others are junk from the packet).
fn valid_mode(mode: &str) -> bool {
    !mode.is_empty() && mode.chars().count() <= MAX_MODE_LEN && !mode.chars().any(char::is_control)
}

/// The logger's `app` as sent: control characters dropped (the server refuses U+0000), at most
/// 64 characters, and none when nothing is left.
fn logger_app(app: Option<&str>) -> Option<String> {
    let app: String = app?
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_APP_LEN)
        .collect();
    let app = app.trim();
    (!app.is_empty()).then(|| app.to_string())
}

/// What the pipeline decided for one contact packet.
enum Decision {
    Queue(QueuedContact),
    Final(RowStatus),
}

impl Shared {
    fn publish_feed(&self) {
        self.bus
            .publish(Event::Feed(self.state.feed_view(self.filter.dropped())));
    }

    fn set_upstream(&self, status: UpstreamStatus) {
        if self.state.set_upstream(status.clone()) {
            self.bus.publish(Event::Upstream(status));
        }
    }

    fn publish_cluster(&self) {
        self.bus
            .publish(Event::Cluster(self.state.cluster_view(Utc::now())));
    }

    /// Tells the spots stream to read its target again.
    fn stream_changed(&self) {
        self.stream_changes.send_modify(|v| *v = v.wrapping_add(1));
    }

    fn me_lock(&self) -> std::sync::MutexGuard<'_, Option<Me>> {
        self.me.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn granted_lock(&self) -> std::sync::MutexGuard<'_, Option<Vec<String>>> {
        self.granted_scopes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// The token's scopes: from the latest `GET /me`, else from the pairing.
    fn scopes(&self) -> Option<Vec<String>> {
        self.me_lock()
            .as_ref()
            .and_then(|me| me.scopes.clone())
            .or_else(|| self.granted_lock().clone())
    }

    /// Serves one spot from the stream, unless the same id was served in the last ten minutes.
    fn serve_spot(&self, spot: Spot) {
        {
            let mut served = self.served.lock().unwrap_or_else(|e| e.into_inner());
            served.retain(|_, at| at.elapsed() < SPOT_REPEAT_WINDOW);
            if served.contains_key(&spot.id) {
                tracing::debug!(id = spot.id, "dropped a repeated spot");
                return;
            }
            served.insert(spot.id, Instant::now());
        }
        let now = Utc::now();
        self.hub.send_line(format_spot(&spot, now));
        let row = self.state.push_sent(SentRow::for_spot(&spot, now));
        self.bus.publish(Event::SpotSent(row));
        self.publish_cluster();
    }

    fn set_pairing(&self, view: PairingView) {
        if self.state.set_pairing(view.clone()) {
            self.bus.publish(Event::Pairing(view));
        }
    }

    async fn sign_in(
        &self,
        token: Token,
        granted: Option<Vec<String>>,
    ) -> Result<Account, SignInError> {
        if let Some(scopes) = &granted {
            if !scopes.iter().any(|s| s == SPOTS_READ) {
                tracing::info!("this sign-in can't receive spots; the DX cluster stays off");
            }
        }
        self.tokens.save(&token)?;
        // The new token took the old one's place in the credential store.
        self.set_sign_out_incomplete(false);
        *self.me_lock() = None;
        *self.granted_lock() = granted;
        self.scope_refused.store(false, Ordering::SeqCst);
        if self.state.set_scope_missing(false) {
            self.publish_cluster();
        }
        self.stream_changed();
        match self.client.me(&token).await {
            Ok(me) => {
                let account = me.account();
                self.account_ok(me);
                self.wake.notify_one();
                Ok(account)
            }
            Err(ClientError::SignedOut) => {
                self.refused(&token);
                Err(ClientError::SignedOut.into())
            }
            Err(e) => {
                // The token is kept; the sender reads the account when Long Lines answers.
                self.mark_account_stale();
                self.set_upstream(UpstreamStatus::Connected);
                self.wake.notify_one();
                Err(e.into())
            }
        }
    }

    /// Records whether a signed-out token is still in the credential store, here and on disk.
    fn set_sign_out_incomplete(&self, incomplete: bool) {
        let marker = &self.sign_out_marker;
        let written = if incomplete {
            std::fs::write(marker, b"")
        } else {
            match std::fs::remove_file(marker) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        if let Err(e) = written {
            tracing::warn!(error = %e, path = %marker.display(), "couldn't update the sign-out marker");
        }
        if self.state.set_sign_out_incomplete(incomplete) {
            self.bus
                .publish(Event::Auth(AuthEvent::SignOutIncomplete { incomplete }));
        }
    }

    /// Tries again to delete a signed-out token.
    fn retry_token_delete(&self) {
        match self.tokens.delete() {
            Ok(()) => {
                tracing::info!("deleted the signed-out token");
                self.set_sign_out_incomplete(false);
            }
            Err(e) => tracing::debug!(error = %e, "still couldn't delete the token"),
        }
    }

    fn mark_account_stale(&self) {
        *self
            .account_checked_at
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn account_due(&self) -> bool {
        self.state.account().is_none()
            || self
                .account_checked_at
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none_or(|t| t.elapsed() >= self.account_refresh)
    }

    fn account_ok(&self, me: Me) {
        *self
            .account_checked_at
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        let account = me.account();
        if let Some(name) = me.device_name.as_deref().filter(|n| !n.trim().is_empty()) {
            self.hub.set_device_name(name);
        }
        *self.me_lock() = Some(me);
        let can_read = self
            .scopes()
            .is_some_and(|scopes| scopes.iter().any(|s| s == SPOTS_READ));
        let refused = self.scope_refused.load(Ordering::SeqCst);
        if self.state.set_scope_missing(!can_read || refused) {
            self.publish_cluster();
        }
        self.stream_changed();
        self.state.set_account(Some(account.clone()));
        self.set_upstream(UpstreamStatus::Connected);
        self.bus
            .publish(Event::Auth(AuthEvent::Account { account }));
        self.publish_feed();
    }

    /// Long Lines answered 401 to a request made with `token`. Signs out only when `token` is
    /// still the stored one: a request that went out before the user signed in again must not
    /// delete the new token (or sign it out).
    fn refused(&self, token: &Token) {
        if let Ok(Some(current)) = self.tokens.load() {
            if current != *token {
                tracing::info!("Long Lines refused a token that has since been replaced");
                self.wake.notify_one();
                return;
            }
        }
        self.signed_out(SignOutReason::Revoked);
    }

    fn signed_out(&self, reason: SignOutReason) {
        if reason == SignOutReason::Revoked {
            tracing::info!("Long Lines refused the token; signing out");
            if let Err(e) = self.tokens.delete() {
                tracing::warn!(error = %e, "couldn't delete the refused token");
            }
        }
        self.mark_account_stale();
        self.state.set_account(None);
        *self.me_lock() = None;
        *self.granted_lock() = None;
        self.scope_refused.store(false, Ordering::SeqCst);
        self.state.set_scope_missing(false);
        self.stream_changed();
        self.publish_cluster();
        self.set_upstream(UpstreamStatus::SignedOut);
        self.bus
            .publish(Event::Auth(AuthEvent::SignedOut { reason }));
        self.publish_feed();
    }

    /// Shows queued items left from the last run as waiting rows.
    fn restore_rows(&self) -> Result<(), QueueError> {
        let now = Utc::now();
        for item in self.queue.pending()? {
            let c = &item.contact;
            let mut row = StreamRow::for_contact(
                c.kind,
                &c.call,
                c.frequency_hz,
                &c.mode,
                parse_timestamp(&c.timestamp_utc),
                now,
                RowStatus::Waiting,
            );
            row.external_id = Some(c.external_id.clone());
            let row = self.state.push_row(row);
            self.rows_lock().insert(item.seq, row.id);
        }
        Ok(())
    }

    fn rows_lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, u64>> {
        self.rows_by_seq.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push_row(&self, row: StreamRow) -> StreamRow {
        let row = self.state.push_row(row);
        self.bus.publish(Event::ContactSeen(row.clone()));
        row
    }

    fn set_row_status(&self, row_id: u64, status: RowStatus) {
        if self.state.set_row_status(row_id, status.clone()).is_some() {
            self.bus.publish(Event::ContactStatus {
                row_id,
                text: status.text(),
                status,
            });
        }
    }

    /// A queued item's final status.
    fn finish(&self, seq: i64, status: RowStatus) {
        let row_id = self.rows_lock().remove(&seq);
        if let Some(row_id) = row_id {
            self.set_row_status(row_id, status);
        }
    }

    fn remove_row(&self, row_id: u64) {
        if self.state.remove_row(row_id) {
            self.bus.publish(Event::RowRemoved { row_id });
        }
    }

    // ---- pipeline ----

    fn handle_datagram(&self, datagram: Datagram) {
        let now = datagram.at;
        self.state.record_packet(now);
        self.bus.publish(Event::PacketReceived { at: now });
        match n1mm::parse(&datagram.bytes) {
            Err(e) => {
                tracing::warn!(error = %e, bytes = datagram.bytes.len(), "couldn't read a packet");
                self.push_row(StreamRow::unreadable(now));
            }
            Ok(Packet::Other(root)) => {
                tracing::debug!(root = %root, "ignored a packet that isn't a contact");
            }
            Ok(Packet::Contact(contact)) => self.handle_contact(contact, now),
        }
        self.publish_feed();
    }

    fn handle_contact(&self, contact: LoggedContact, now: DateTime<Utc>) {
        let decision = match contact.kind {
            ContactKind::Delete => self.decide_delete(&contact, now),
            ContactKind::Info | ContactKind::Replace => self.decide_upsert(&contact, now),
        };
        let mut row = StreamRow::for_contact(
            contact.kind,
            &contact.call,
            contact.frequency_hz,
            &contact.mode,
            contact.timestamp,
            now,
            RowStatus::Waiting,
        );
        let status = match &decision {
            Decision::Queue(q) => {
                row.external_id = Some(q.external_id.clone());
                if self.state.test_mode() {
                    RowStatus::TestMode
                } else {
                    RowStatus::Waiting
                }
            }
            Decision::Final(s) => s.clone(),
        };
        row.status_text = status.text();
        row.status = status.clone();

        // An N1MM edit is a delete followed by a replace: fold the delete into the replace.
        if contact.kind == ContactKind::Replace
            && matches!(status, RowStatus::Waiting | RowStatus::TestMode)
        {
            if let Some(id) = row.external_id.as_deref() {
                self.fold_edit(id, now, status == RowStatus::Waiting);
            }
        }

        let row = self.push_row(row);
        if let (Decision::Queue(queued), RowStatus::Waiting) = (decision, &status) {
            match self.queue.enqueue(&queued, now) {
                Ok(seq) => {
                    self.rows_lock().insert(seq, row.id);
                    self.wake.notify_one();
                }
                Err(e) => {
                    tracing::error!(error = %e, "couldn't queue a contact");
                    self.set_row_status(
                        row.id,
                        RowStatus::CouldntSend {
                            reason: reasons::NOT_SAVED.into(),
                        },
                    );
                }
            }
        }
    }

    fn fold_edit(&self, external_id: &str, now: DateTime<Utc>, queued: bool) {
        let Some(delete_row) =
            self.state
                .find_recent_row(external_id, ContactKind::Delete, now - self.edit_window)
        else {
            return;
        };
        if queued {
            match self.queue.remove_waiting(external_id, Op::Delete) {
                Ok(seqs) => {
                    let mut rows = self.rows_lock();
                    for seq in seqs {
                        rows.remove(&seq);
                    }
                }
                Err(e) => tracing::warn!(error = %e, "couldn't fold an edit's delete"),
            }
        }
        self.remove_row(delete_row.id);
    }

    fn decide_upsert(&self, c: &LoggedContact, now: DateTime<Utc>) -> Decision {
        if !c.is_original {
            return Decision::Final(RowStatus::Forwarded);
        }
        let (Some(logged_at), Some(frequency_hz)) = (c.timestamp, c.frequency_hz) else {
            tracing::warn!("a contact packet had no usable timestamp or frequency");
            return Decision::Final(RowStatus::Unreadable);
        };
        if !valid_call(&c.call) || !valid_mode(&c.mode) {
            tracing::warn!("a contact packet had no usable call or mode");
            return Decision::Final(RowStatus::Unreadable);
        }
        if c.mycall.is_empty() {
            return Decision::Final(RowStatus::NoMycall);
        }
        if let Some(account) = self.state.account() {
            if !same_station(&c.mycall, &account.callsign) {
                self.state.set_mismatch(Some(c.mycall.clone()));
                return Decision::Final(RowStatus::Mismatch {
                    mycall: c.mycall.clone(),
                });
            }
            self.state.set_mismatch(None);
        }
        if !valid_call(&c.mycall) {
            return Decision::Final(RowStatus::Unreadable);
        }
        match classify(logged_at, now) {
            Freshness::TooOld => return Decision::Final(RowStatus::TooOld),
            Freshness::Future => return Decision::Final(RowStatus::ClockSkew),
            Freshness::Fresh => {}
        }
        let band = band_for_hz(frequency_hz).map(str::to_string);
        let timestamp_utc = utc_text(logged_at);
        let id = external_id(
            c.id.as_deref(),
            HashFields {
                mycall: &c.mycall,
                call: &c.call,
                band: band.as_deref(),
                mode: &c.mode,
                timestamp_utc: &timestamp_utc,
            },
        );
        Decision::Queue(QueuedContact {
            external_id: id,
            op: Op::Upsert,
            kind: c.kind,
            mycall: c.mycall.clone(),
            call: c.call.clone(),
            frequency_hz: Some(frequency_hz),
            band,
            mode: c.mode.clone(),
            timestamp_utc,
            logger_app: logger_app(c.app.as_deref()),
        })
    }

    fn decide_delete(&self, c: &LoggedContact, now: DateTime<Utc>) -> Decision {
        if !c.is_original {
            return Decision::Final(RowStatus::Forwarded);
        }
        let Some(logged_at) = c.timestamp else {
            tracing::warn!("a delete packet had no usable timestamp");
            return Decision::Final(RowStatus::Unreadable);
        };
        let timestamp_utc = utc_text(logged_at);
        let band = c.frequency_hz.and_then(band_for_hz).map(str::to_string);
        let has_logger_id =
            c.id.as_deref()
                .is_some_and(crate::external_id::usable_logger_id);
        let can_hash = !c.mycall.is_empty()
            && !c.call.is_empty()
            && !c.mode.is_empty()
            && c.frequency_hz.is_some();
        if !has_logger_id && !can_hash {
            return Decision::Final(RowStatus::CouldntSend {
                reason: reasons::NO_CONTACT_ID.into(),
            });
        }
        if classify(logged_at, now) == Freshness::Future {
            return Decision::Final(RowStatus::ClockSkew);
        }
        let id = external_id(
            c.id.as_deref(),
            HashFields {
                mycall: &c.mycall,
                call: &c.call,
                band: band.as_deref(),
                mode: &c.mode,
                timestamp_utc: &timestamp_utc,
            },
        );
        Decision::Queue(QueuedContact {
            external_id: id,
            op: Op::Delete,
            kind: ContactKind::Delete,
            mycall: c.mycall.clone(),
            call: c.call.clone(),
            frequency_hz: c.frequency_hz,
            band,
            mode: c.mode.clone(),
            timestamp_utc,
            logger_app: logger_app(c.app.as_deref()),
        })
    }

    // ---- sender ----

    /// Waits for new work, at most `max` (and never longer than the idle tick).
    async fn idle(&self, max: Option<Duration>) {
        let wait = max.map_or(self.idle_tick, |m| m.min(self.idle_tick));
        tokio::select! {
            () = self.wake.notified() => {}
            () = tokio::time::sleep(wait) => {}
        }
    }

    fn expire(&self, now: DateTime<Utc>) {
        match self.queue.expire(now) {
            Ok(dropped) => {
                for (item, reason) in dropped {
                    tracing::info!(external_id = %item.contact.external_id, ?reason, "dropped a queued contact");
                    let status = match reason {
                        DropReason::TooOld => RowStatus::TooOld,
                        DropReason::ClockSkew => RowStatus::ClockSkew,
                    };
                    self.finish(item.seq, status);
                }
            }
            Err(e) => tracing::error!(error = %e, "couldn't expire queued contacts"),
        }
    }

    async fn send_batch(
        &self,
        token: &Token,
        account: &Account,
        items: Vec<QueueItem>,
        rate: &mut RateWindow,
    ) {
        let now = Utc::now();
        let mut sendable = Vec::with_capacity(items.len());
        let mut refused = Vec::new();
        for item in items {
            if item.contact.op == Op::Upsert
                && !same_station(&item.contact.mycall, &account.callsign)
            {
                refused.push(item);
            } else {
                sendable.push(item);
            }
        }
        if !refused.is_empty() {
            let seqs: Vec<i64> = refused.iter().map(|i| i.seq).collect();
            if let Err(e) = self.queue.remove(&seqs) {
                tracing::error!(error = %e, "couldn't remove refused contacts");
            }
            for item in refused {
                self.state.set_mismatch(Some(item.contact.mycall.clone()));
                self.finish(
                    item.seq,
                    RowStatus::Mismatch {
                        mycall: item.contact.mycall,
                    },
                );
            }
            self.publish_feed();
        }
        if sendable.is_empty() {
            return;
        }
        let wire: Vec<WireItem> = sendable.iter().map(|i| i.contact.to_wire()).collect();
        let seqs: Vec<i64> = sendable.iter().map(|i| i.seq).collect();
        tracing::debug!(items = wire.len(), "sending contacts");
        let result = self.client.submit(token, &wire).await;
        // Count what reached the server; a request that never arrived used none of the limit.
        if !matches!(result, Err(ClientError::Retry { .. })) {
            rate.record(wire.len());
        }
        match result {
            Ok(results) => {
                if let Err(e) = self.queue.remove(&seqs) {
                    tracing::error!(error = %e, "couldn't remove sent contacts from the queue");
                }
                let mut mismatch = false;
                for (item, result) in sendable.iter().zip(results) {
                    if result.external_id != item.contact.external_id {
                        tracing::warn!(
                            "Long Lines echoed a different external_id; matching by position"
                        );
                    }
                    let status = row_status_for(&item.contact, &result.status);
                    mismatch |= matches!(status, RowStatus::Mismatch { .. });
                    if let RowStatus::Mismatch { mycall } = &status {
                        self.state.set_mismatch(Some(mycall.clone()));
                    }
                    self.finish(item.seq, status);
                }
                self.set_upstream(UpstreamStatus::Connected);
                if mismatch {
                    self.publish_feed();
                }
            }
            Err(ClientError::SignedOut) => self.refused(token),
            Err(ClientError::RateLimited { retry_after }) => {
                let until = later(now, retry_after);
                tracing::info!(
                    seconds = retry_after.as_secs(),
                    "rate limited by Long Lines"
                );
                if let Err(e) = self.queue.defer(&seqs, until, "rate limited") {
                    tracing::error!(error = %e, "couldn't defer contacts");
                }
                self.set_upstream(UpstreamStatus::RateLimited { until });
            }
            Err(ClientError::Rejected { status, code }) => {
                tracing::warn!(status, code = ?code, "Long Lines refused the request");
                if let Err(e) = self.queue.remove(&seqs) {
                    tracing::error!(error = %e, "couldn't remove refused contacts");
                }
                let reason = rejected_reason(status);
                for seq in seqs {
                    self.finish(
                        seq,
                        RowStatus::CouldntSend {
                            reason: reason.clone(),
                        },
                    );
                }
                self.set_upstream(UpstreamStatus::Connected);
            }
            Err(ClientError::Retry { reason }) => {
                tracing::info!(reason = %reason, "couldn't send; will retry");
                match self.queue.mark_failed(&seqs, &reason, now) {
                    Ok(at) => self.set_upstream(UpstreamStatus::Retrying { at, reason }),
                    Err(e) => tracing::error!(error = %e, "couldn't record a failed send"),
                }
            }
        }
    }
}

impl StreamHost for Shared {
    fn target(&self) -> Target {
        // A token whose delete failed on sign-out is never used again.
        if !self.state.cluster_enabled()
            || self.state.scope_missing()
            || self.state.sign_out_incomplete()
        {
            return Target::Off;
        }
        let Ok(Some(token)) = self.tokens.load() else {
            return Target::Off;
        };
        let can_read = self
            .scopes()
            .is_some_and(|scopes| scopes.iter().any(|s| s == SPOTS_READ));
        let url = self.me_lock().as_ref().and_then(|me| me.spots_url.clone());
        match url {
            Some(url) if can_read && stream::stream_url_ok(&url) => Target::Open { token, url },
            Some(_) if can_read => {
                tracing::warn!("the spots stream's URL isn't one the relay sends its token to");
                Target::Off
            }
            _ => Target::Off,
        }
    }

    fn status(&self, status: ClusterUpstream) {
        if self.state.set_cluster_upstream(status) {
            self.publish_cluster();
        }
    }

    fn hello(&self, hello: Hello) {
        tracing::debug!(relay_id = %hello.relay_id, "the spots stream said hello");
    }

    fn spot(&self, spot: Spot) {
        self.serve_spot(spot);
    }

    fn refused(&self, token: &Token) {
        Shared::refused(self, token);
    }

    fn scope_missing(&self, token: &Token) {
        if !matches!(self.tokens.load(), Ok(Some(current)) if current == *token) {
            return;
        }
        self.scope_refused.store(true, Ordering::SeqCst);
        if self.state.set_scope_missing(true) {
            self.publish_cluster();
        }
    }
}

fn row_status_for(contact: &QueuedContact, status: &ItemStatus) -> RowStatus {
    match status {
        ItemStatus::Created | ItemStatus::Updated | ItemStatus::Unchanged => match contact.kind {
            ContactKind::Replace => RowStatus::Updated,
            _ => RowStatus::Sent,
        },
        ItemStatus::Retracted | ItemStatus::Unknown => RowStatus::Removed,
        ItemStatus::Rejected(RejectReason::TooOld) => RowStatus::TooOld,
        ItemStatus::Rejected(RejectReason::ClockSkew) => RowStatus::ClockSkew,
        ItemStatus::Rejected(RejectReason::MycallMismatch) => RowStatus::Mismatch {
            mycall: contact.mycall.clone(),
        },
        ItemStatus::Rejected(RejectReason::Invalid) => RowStatus::CouldntSend {
            reason: reasons::INVALID.into(),
        },
        ItemStatus::Rejected(RejectReason::Unrecognized(_)) => RowStatus::CouldntSend {
            reason: reasons::UNEXPECTED.into(),
        },
    }
}

fn rejected_reason(status: u16) -> String {
    match status {
        400 => reasons::BAD_REQUEST.into(),
        403 => reasons::NOT_ALLOWED.into(),
        404 => reasons::NOT_FOUND.into(),
        s => reasons::http(s),
    }
}

/// Contacts sent in the last minute, so the client stays under the server's limit by itself.
#[derive(Debug)]
struct RateWindow {
    per_minute: usize,
    sent: VecDeque<(Instant, usize)>,
}

impl RateWindow {
    const WINDOW: Duration = Duration::from_secs(60);

    fn new(per_minute: usize) -> Self {
        Self {
            per_minute,
            sent: VecDeque::new(),
        }
    }

    fn prune(&mut self) {
        while self
            .sent
            .front()
            .is_some_and(|(t, _)| t.elapsed() >= Self::WINDOW)
        {
            self.sent.pop_front();
        }
    }

    /// How many more contacts may go now, and if none, how long until some may.
    fn allowance(&mut self) -> (usize, Duration) {
        self.prune();
        let used: usize = self.sent.iter().map(|(_, n)| n).sum();
        let left = self.per_minute.saturating_sub(used);
        let wait = self.sent.front().map_or(Duration::ZERO, |(t, _)| {
            Self::WINDOW.saturating_sub(t.elapsed())
        });
        (left, wait)
    }

    fn record(&mut self, n: usize) {
        self.sent.push_back((Instant::now(), n));
    }
}

/// The LAN address when other computers may connect.
fn lan_address_if(lan: bool) -> Option<String> {
    if lan {
        crate::cluster::lan_address()
    } else {
        None
    }
}

/// Keeps the DX cluster card's connection count current.
async fn watch_clients(shared: Arc<Shared>) {
    let mut clients = shared.hub.watch_clients();
    loop {
        let n = *clients.borrow_and_update();
        if shared.state.set_cluster_clients(n) {
            shared.publish_cluster();
        }
        if clients.changed().await.is_err() {
            return;
        }
    }
}

async fn pipeline(shared: Arc<Shared>, mut rx: mpsc::Receiver<Datagram>) {
    while let Some(datagram) = rx.recv().await {
        shared.handle_datagram(datagram);
    }
}

/// While another program holds the port, tries it again, so the relay picks the port up as soon
/// as it is free. It never moves to another port by itself.
async fn rebind(shared: Arc<Shared>) {
    loop {
        tokio::time::sleep(shared.port_retry).await;
        let listener = shared.listener.lock().await;
        if !matches!(listener.status(), ListenerStatus::PortInUse { .. }) {
            continue;
        }
        let status = listener.start();
        drop(listener);
        if let ListenerStatus::Listening { port } = status {
            tracing::info!(port, "the port is free again");
            shared.state.set_listener(status, Utc::now());
            shared.publish_feed();
        }
    }
}

async fn sender(shared: Arc<Shared>) {
    let mut rate = RateWindow::new(shared.contacts_per_minute);
    let mut account_failures: u32 = 0;
    loop {
        shared.expire(Utc::now());

        if shared.state.sign_out_incomplete() {
            shared.retry_token_delete();
            if shared.state.sign_out_incomplete() {
                // Signed out: the token still in the credential store is never used.
                shared.set_upstream(UpstreamStatus::SignedOut);
                shared.idle(None).await;
                continue;
            }
        }

        let token = match shared.tokens.load() {
            Ok(Some(token)) => token,
            Ok(None) => {
                shared.set_upstream(UpstreamStatus::SignedOut);
                shared.idle(None).await;
                continue;
            }
            Err(e) => {
                // Not signed out: the token may be there once the keyring unlocks.
                if shared.state.upstream() != UpstreamStatus::CredentialStoreUnavailable {
                    tracing::warn!(error = %e, "couldn't read the token; trying again");
                }
                shared.set_upstream(UpstreamStatus::CredentialStoreUnavailable);
                shared.idle(None).await;
                continue;
            }
        };

        if shared.account_due() {
            match shared.client.me(&token).await {
                Ok(me) => {
                    account_failures = 0;
                    shared.account_ok(me);
                }
                Err(ClientError::SignedOut) => {
                    shared.refused(&token);
                    continue;
                }
                Err(e) => {
                    if shared.state.account().is_some() {
                        // Keep the known account; try again at the next refresh.
                        tracing::info!(error = %e, "couldn't refresh the account");
                        *shared
                            .account_checked_at
                            .lock()
                            .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
                    } else {
                        account_failures = account_failures.saturating_add(1);
                        let wait = match &e {
                            ClientError::RateLimited { retry_after } => *retry_after,
                            _ => shared.backoff.delay(account_failures),
                        };
                        let at = later(Utc::now(), wait);
                        shared.set_upstream(UpstreamStatus::Retrying {
                            at,
                            reason: e.to_string(),
                        });
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }
            }
        }
        let Some(account) = shared.state.account() else {
            shared.idle(None).await;
            continue;
        };

        let (left, wait) = rate.allowance();
        if left == 0 {
            shared.idle(Some(wait)).await;
            continue;
        }
        // Freshness again right before sending (contract 6.5): `GET /me` above can take up to
        // the request timeout, long enough for a queued upsert to pass the 30-minute window.
        let now = Utc::now();
        shared.expire(now);
        match shared.queue.next_batch(now, left.min(shared.max_batch)) {
            Ok(NextBatch::Empty) => shared.idle(None).await,
            Ok(NextBatch::WaitUntil(at)) => {
                let wait = (at - now).to_std().unwrap_or(Duration::ZERO);
                shared.idle(Some(wait)).await;
            }
            Ok(NextBatch::Ready(items)) => {
                shared.send_batch(&token, &account, items, &mut rate).await
            }
            Err(e) => {
                tracing::error!(error = %e, "couldn't read the queue");
                shared.idle(None).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell keeps the engine in Tauri's state and calls it from any thread.
    #[test]
    fn the_engine_is_send_and_sync() {
        fn send_sync<T: Send + Sync + 'static>() {}
        send_sync::<Engine>();
    }

    /// Tauri runs commands on any worker thread, so the DX cluster changes' futures must be
    /// `Send` (the shell can't be built in every check, so the core says so here).
    #[test]
    fn cluster_changes_can_run_on_any_thread() {
        fn send<T: Send>(_: &T) {}
        let check = |engine: &Engine| {
            send(&engine.set_cluster_enabled(true));
            send(&engine.set_cluster_port(7373));
            send(&engine.set_cluster_lan(true));
        };
        let _ = check;
    }

    #[test]
    fn rate_window_counts_the_last_minute() {
        let mut w = RateWindow::new(120);
        assert_eq!(w.allowance().0, 120);
        w.record(100);
        assert_eq!(w.allowance().0, 20);
        w.record(20);
        let (left, wait) = w.allowance();
        assert_eq!(left, 0);
        assert!(wait > Duration::from_secs(50));
    }

    #[test]
    fn results_map_to_row_statuses() {
        let mut c = QueuedContact {
            external_id: "x".into(),
            op: Op::Upsert,
            kind: ContactKind::Info,
            mycall: "K4XYZ".into(),
            call: "W1AW".into(),
            frequency_hz: Some(14_025_000),
            band: Some("20m".into()),
            mode: "CW".into(),
            timestamp_utc: "2026-10-28T21:14:00Z".into(),
            logger_app: None,
        };
        assert_eq!(row_status_for(&c, &ItemStatus::Created), RowStatus::Sent);
        assert_eq!(row_status_for(&c, &ItemStatus::Unchanged), RowStatus::Sent);
        assert_eq!(
            row_status_for(&c, &ItemStatus::Rejected(RejectReason::MycallMismatch)),
            RowStatus::Mismatch {
                mycall: "K4XYZ".into()
            }
        );
        assert_eq!(
            row_status_for(&c, &ItemStatus::Rejected(RejectReason::TooOld)),
            RowStatus::TooOld
        );
        assert_eq!(
            row_status_for(&c, &ItemStatus::Rejected(RejectReason::ClockSkew)),
            RowStatus::ClockSkew
        );
        assert!(matches!(
            row_status_for(&c, &ItemStatus::Rejected(RejectReason::Invalid)),
            RowStatus::CouldntSend { .. }
        ));
        assert!(matches!(
            row_status_for(
                &c,
                &ItemStatus::Rejected(RejectReason::Unrecognized("new".into()))
            ),
            RowStatus::CouldntSend { .. }
        ));
        c.kind = ContactKind::Replace;
        assert_eq!(row_status_for(&c, &ItemStatus::Created), RowStatus::Updated);
        assert_eq!(row_status_for(&c, &ItemStatus::Updated), RowStatus::Updated);
        c.kind = ContactKind::Delete;
        c.op = Op::Delete;
        assert_eq!(
            row_status_for(&c, &ItemStatus::Retracted),
            RowStatus::Removed
        );
        assert_eq!(row_status_for(&c, &ItemStatus::Unknown), RowStatus::Removed);
    }

    #[test]
    fn rejections_have_reasons() {
        assert_eq!(rejected_reason(400), reasons::BAD_REQUEST);
        assert_eq!(rejected_reason(403), reasons::NOT_ALLOWED);
        assert_eq!(rejected_reason(404), reasons::NOT_FOUND);
        assert_eq!(rejected_reason(418), "Long Lines answered with error 418.");
    }

    #[test]
    fn call_shape() {
        assert!(valid_call("KK4PWJ/P"));
        assert!(!valid_call(""));
        assert!(!valid_call("KK4 PWJ"));
        assert!(!valid_call(&"A".repeat(65)));
        assert!(valid_call(&"\u{1F4FB}".repeat(64)), "characters, not bytes");
        for bad in [
            "W1\0AW",
            "W1\u{1}AW",
            "W1\u{85}AW",
            "W1\u{feff}AW",
            "W1\u{a0}AW",
        ] {
            assert!(!valid_call(bad), "{bad:?}");
        }
    }

    #[test]
    fn mode_and_app_shape() {
        assert!(valid_mode("CW"));
        assert!(valid_mode("DIGITAL VOICE"));
        assert!(!valid_mode(""));
        assert!(!valid_mode("C\0W"));
        assert!(!valid_mode(&"M".repeat(33)));
        assert_eq!(logger_app(Some("N1\0MM")), Some("N1MM".into()));
        assert_eq!(logger_app(Some("\0")), None);
        assert_eq!(logger_app(None), None);
        assert_eq!(logger_app(Some(&"x".repeat(70))), Some("x".repeat(64)));
    }
}
