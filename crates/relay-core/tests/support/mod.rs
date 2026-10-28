//! A fake Long Lines for the integration tests: a wiremock server whose responder follows the
//! contract's `GET /me` and `POST /contacts` semantics, a raw HTTP server for the spots stream
//! (wiremock answers with whole bodies, and a stream must stay open), plus helpers to run an
//! engine against them.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use relay_core::callsign::base_callsign;
use relay_core::engine::{Engine, EngineConfig};
use relay_core::listeners::ListenerStatus;
use relay_core::source_filter::SourceFilter;
use relay_core::state::Snapshot;
use relay_core::upstream::client::ClientConfig;
use relay_core::upstream::queue::Backoff;
use relay_core::upstream::token_store::{MemoryTokenStore, Token};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TOKEN: &str = "ll_relay_abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst";
pub const CALLSIGN: &str = "KK4PWJ";
pub const EMAIL: &str = "operator@example.com";
pub const RELAY_ID: &str = "3f6c2a9e-8b1d-4c7a-9e2f-5a0b1c2d3e4f";
pub const DEVICE_NAME: &str = "SHACK-PC";

/// A canned answer for the next `POST /contacts`, ahead of the normal handling.
#[derive(Clone, Debug)]
pub enum Scripted {
    Status(u16),
    RateLimited { retry_after_secs: u64 },
}

#[derive(Clone, Debug)]
pub struct ContactsRequest {
    pub at: Instant,
    pub body: Value,
    pub user_agent: String,
}

#[derive(Debug)]
struct Stored {
    content: Value,
    live: bool,
}

#[derive(Debug, Default)]
pub struct ServerState {
    pub revoked: bool,
    /// Answer every `POST /contacts` with this status while set.
    pub fail_contacts: Option<u16>,
    pub script: VecDeque<Scripted>,
    pub requests: Vec<ContactsRequest>,
    pub me_calls: usize,
    /// `GET /me`'s v2 `spots_url`; left out while `None`.
    pub spots_url: Option<String>,
    /// `GET /me`'s v2 `scopes`; left out while `None`.
    pub scopes: Option<Vec<String>>,
    contacts: HashMap<String, Stored>,
}

struct Handler {
    state: Arc<Mutex<ServerState>>,
}

impl Respond for Handler {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut state = self.state.lock().unwrap();
        let authorized = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {TOKEN}"));
        match (request.method.as_str(), request.url.path()) {
            ("GET", "/me") => {
                state.me_calls += 1;
                if !authorized || state.revoked {
                    return ResponseTemplate::new(401).set_body_json(json!({"error": "bad_token"}));
                }
                let mut me = json!({"callsign": CALLSIGN, "email": EMAIL});
                if let Some(url) = &state.spots_url {
                    me["spots_url"] = json!(url);
                    me["relay_id"] = json!(RELAY_ID);
                    me["device_name"] = json!(DEVICE_NAME);
                }
                if let Some(scopes) = &state.scopes {
                    me["scopes"] = json!(scopes);
                }
                ResponseTemplate::new(200).set_body_json(me)
            }
            ("POST", "/contacts") => {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let user_agent = request
                    .headers
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                state.requests.push(ContactsRequest {
                    at: Instant::now(),
                    body: body.clone(),
                    user_agent,
                });
                if !authorized || state.revoked {
                    return ResponseTemplate::new(401).set_body_json(json!({"error": "bad_token"}));
                }
                if let Some(status) = state.fail_contacts {
                    return ResponseTemplate::new(status);
                }
                match state.script.pop_front() {
                    Some(Scripted::Status(status)) => return ResponseTemplate::new(status),
                    Some(Scripted::RateLimited { retry_after_secs }) => {
                        return ResponseTemplate::new(429)
                            .insert_header("Retry-After", retry_after_secs.to_string().as_str())
                            .set_body_json(json!({"error": "rate_limited"}));
                    }
                    None => {}
                }
                let results: Vec<Value> = body["contacts"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|item| apply(&mut state, item))
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"results": results}))
            }
            _ => ResponseTemplate::new(404).set_body_json(json!({"error": "not_found"})),
        }
    }
}

fn apply(state: &mut ServerState, item: &Value) -> Value {
    let id = item["external_id"].as_str().unwrap_or_default().to_string();
    match item["op"].as_str() {
        Some("upsert") => {
            if base_callsign(item["mycall"].as_str().unwrap_or_default()) != base_callsign(CALLSIGN)
            {
                return json!({"external_id": id, "status": "rejected", "reason": "mycall_mismatch"});
            }
            // Content as contract 6.3 defines it: the request's band is not compared.
            let content = json!([
                item["mycall"],
                item["call"],
                item["frequency_hz"],
                item["mode"],
                item["timestamp_utc"]
            ]);
            let status = match state.contacts.get(&id) {
                Some(s) if s.live && s.content == content => "unchanged",
                Some(s) if s.live => "updated",
                _ => "created",
            };
            state.contacts.insert(
                id.clone(),
                Stored {
                    content,
                    live: true,
                },
            );
            json!({"external_id": id, "status": status})
        }
        Some("delete") => {
            let status = match state.contacts.get_mut(&id) {
                Some(s) if s.live => {
                    s.live = false;
                    "retracted"
                }
                _ => "unknown",
            };
            json!({"external_id": id, "status": status})
        }
        _ => json!({"external_id": id, "status": "rejected", "reason": "invalid"}),
    }
}

/// The fake server.
pub struct FakeLongLines {
    pub server: MockServer,
    pub state: Arc<Mutex<ServerState>>,
}

impl FakeLongLines {
    pub async fn start() -> Self {
        Self::mount(MockServer::start().await).await
    }

    /// Starts on a listener the test already holds (to come up on a known port).
    pub async fn start_on(listener: std::net::TcpListener) -> Self {
        Self::mount(MockServer::builder().listener(listener).start().await).await
    }

    async fn mount(server: MockServer) -> Self {
        let state = Arc::new(Mutex::new(ServerState::default()));
        Mock::given(any())
            .respond_with(Handler {
                state: Arc::clone(&state),
            })
            .mount(&server)
            .await;
        Self { server, state }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    pub fn requests(&self) -> Vec<ContactsRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Every item sent, in order.
    pub fn items(&self) -> Vec<Value> {
        self.requests()
            .iter()
            .flat_map(|r| r.body["contacts"].as_array().cloned().unwrap_or_default())
            .collect()
    }

    pub fn is_live(&self, external_id: &str) -> Option<bool> {
        self.state
            .lock()
            .unwrap()
            .contacts
            .get(external_id)
            .map(|s| s.live)
    }

    pub fn stored_frequency(&self, external_id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap()
            .contacts
            .get(external_id)
            .and_then(|s| s.content[2].as_u64())
    }

    pub fn script(&self, step: Scripted) {
        self.state.lock().unwrap().script.push_back(step);
    }

    pub fn set_fail_contacts(&self, status: Option<u16>) {
        self.state.lock().unwrap().fail_contacts = status;
    }

    pub fn revoke(&self) {
        self.state.lock().unwrap().revoked = true;
    }

    /// Makes `GET /me` a v2 answer pointing at `stream` with `scopes`.
    pub fn serve_v2(&self, stream: &FakeStream, scopes: &[&str]) {
        let mut state = self.state.lock().unwrap();
        state.spots_url = Some(stream.url());
        state.scopes = Some(scopes.iter().map(|s| s.to_string()).collect());
    }
}

/// Both scopes, as a v2 pairing grants them.
pub const BOTH_SCOPES: [&str; 2] = ["contacts:write", "spots:read"];

/// `hello` as the server sends it.
pub fn hello_event() -> String {
    format!(
        "event: hello\ndata: {}\n\n",
        json!({"relay_id": RELAY_ID, "device_name": DEVICE_NAME})
    )
}

/// A `spot` event for `spot`.
pub fn spot_event(spot: &Value) -> String {
    format!("event: spot\ndata: {spot}\n\n")
}

/// The contract's example spot (9.4).
pub fn example_spot() -> Value {
    json!({
        "id": 48213377, "spotter": "W4DOG", "frequency_khz": 14062.0, "callsign": "K4ABC",
        "comment": "CW POTA K-0039", "time_utc": "2114", "mode": "CW",
        "reference": "POTA K-0039", "source": "pota", "source_label": "POTA",
        "connection": "POTA states", "spot_time": "2026-10-28T21:14:07Z"
    })
}

/// How the fake spots stream answers one request.
#[derive(Clone, Debug)]
pub enum StreamReply {
    /// A 200 stream that sends `initial`, then whatever [`FakeStream::send`] pushes, and stays
    /// open until the client closes it.
    Open { initial: String },
    /// A 200 stream that sends `body` and ends.
    Close { body: String },
    /// An error answer with a JSON body.
    Status {
        code: u16,
        error: &'static str,
        retry_after: Option<u64>,
    },
    /// A 200 that is not an event stream.
    Plain,
}

#[derive(Debug, Default)]
pub struct StreamState {
    /// Each request's head, in order.
    pub requests: Vec<String>,
    /// Answers for the next requests; an open stream with `hello` when empty.
    pub replies: VecDeque<StreamReply>,
    /// Streams open now.
    pub open: usize,
    /// Streams the client closed.
    pub closed: usize,
}

/// A raw HTTP/1.1 server for `GET /v2/spots`.
pub struct FakeStream {
    pub addr: SocketAddr,
    pub state: Arc<Mutex<StreamState>>,
    events: broadcast::Sender<String>,
}

impl FakeStream {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(StreamState::default()));
        let (events, _) = broadcast::channel(64);
        let (st, ev) = (Arc::clone(&state), events.clone());
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve_stream(socket, Arc::clone(&st), ev.clone()));
            }
        });
        Self {
            addr,
            state,
            events,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/v2/spots", self.addr)
    }

    pub fn reply(&self, reply: StreamReply) {
        self.state.lock().unwrap().replies.push_back(reply);
    }

    /// Writes `text` to every open stream.
    pub fn send(&self, text: impl Into<String>) {
        let _ = self.events.send(text.into());
    }

    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    pub fn open(&self) -> usize {
        self.state.lock().unwrap().open
    }

    pub fn closed(&self) -> usize {
        self.state.lock().unwrap().closed
    }

    /// Polls until `done` holds, or fails after 10 seconds.
    pub async fn wait(&self, what: &str, done: impl Fn(&StreamState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if done(&self.state.lock().unwrap()) {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "timed out waiting for {what}: {:?}",
                    self.state.lock().unwrap()
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

async fn serve_stream(
    mut socket: TcpStream,
    state: Arc<Mutex<StreamState>>,
    events: broadcast::Sender<String>,
) {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        match socket.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let reply = {
        let mut state = state.lock().unwrap();
        state
            .requests
            .push(String::from_utf8_lossy(&head).into_owned());
        state.replies.pop_front().unwrap_or(StreamReply::Open {
            initial: hello_event(),
        })
    };
    const STREAM_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    match reply {
        StreamReply::Status {
            code,
            error,
            retry_after,
        } => {
            let body = json!({ "error": error }).to_string();
            let retry = retry_after.map_or(String::new(), |s| format!("Retry-After: {s}\r\n"));
            let text = format!(
                "HTTP/1.1 {code} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(text.as_bytes()).await;
        }
        StreamReply::Plain => {
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi")
                .await;
        }
        StreamReply::Close { body } => {
            let _ = socket
                .write_all(format!("{STREAM_HEAD}{body}").as_bytes())
                .await;
        }
        StreamReply::Open { initial } => {
            let mut rx = events.subscribe();
            if socket
                .write_all(format!("{STREAM_HEAD}{initial}").as_bytes())
                .await
                .is_err()
            {
                return;
            }
            state.lock().unwrap().open += 1;
            loop {
                tokio::select! {
                    event = rx.recv() => {
                        let Ok(event) = event else { continue };
                        if socket.write_all(event.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    read = socket.read(&mut buf) => {
                        if matches!(read, Ok(0) | Err(_)) {
                            break;
                        }
                    }
                }
            }
            let mut state = state.lock().unwrap();
            state.open -= 1;
            state.closed += 1;
        }
    }
}

/// A token store holding the test token.
pub fn signed_in() -> Arc<MemoryTokenStore> {
    Arc::new(MemoryTokenStore::with_token(Token::new(TOKEN)))
}

/// An engine on an ephemeral port with fast backoff, against `base_url`.
pub async fn start_engine(
    queue_path: &Path,
    base_url: &str,
    tokens: Arc<MemoryTokenStore>,
) -> Engine {
    start_engine_with(queue_path, base_url, tokens, |_| {}).await
}

/// [`start_engine`], with `tweak` applied to the config last.
pub async fn start_engine_with(
    queue_path: &Path,
    base_url: &str,
    tokens: Arc<MemoryTokenStore>,
    tweak: impl FnOnce(&mut EngineConfig),
) -> Engine {
    let mut config = EngineConfig::new(
        0,
        queue_path.to_path_buf(),
        ClientConfig::new(base_url, "0.1.0-test"),
        tokens,
    );
    config.source_filter = Arc::new(SourceFilter::with_addresses([]));
    config.backoff = Backoff {
        base: Duration::from_millis(100),
        max: Duration::from_millis(400),
    };
    config.idle_tick = Duration::from_millis(50);
    tweak(&mut config);
    Engine::start(config).await.expect("engine starts")
}

/// The bound UDP port.
pub fn port(engine: &Engine) -> u16 {
    match engine.snapshot().feed.listener {
        ListenerStatus::Listening { port } => port,
        other => panic!("not listening: {other:?}"),
    }
}

/// Polls the snapshot until `done` holds, or fails after 10 seconds.
pub async fn wait_until(engine: &Engine, what: &str, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = engine.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}; last snapshot: {snapshot:#?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// `(title, status line)` for every row, newest first.
pub fn rows(snapshot: &Snapshot) -> Vec<(String, String)> {
    snapshot
        .stream
        .iter()
        .map(|r| (r.title.clone(), r.status_text.clone()))
        .collect()
}

/// Waits until the account is known.
pub async fn wait_signed_in(engine: &Engine) {
    wait_until(engine, "the account", |s| s.account.is_some()).await;
}

/// Waits until every row's status is final and there are `count` rows.
pub async fn wait_rows_final(engine: &Engine, count: usize) -> Snapshot {
    wait_until(engine, &format!("{count} final rows"), |s| {
        s.stream.len() == count && s.stream.iter().all(|r| r.status.is_final())
    })
    .await
}
