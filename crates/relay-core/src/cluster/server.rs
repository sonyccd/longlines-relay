//! The DX cluster's TCP server (contract 11).
//!
//! It binds `127.0.0.1` (or every IPv4 interface while other computers may connect) on port
//! 7373, falling through to 7374 ... 7399 when the default port is taken, and never moving off a
//! port the user chose. Each connection gets `login: `, a greeting and a prompt, then every spot
//! line the [`ClusterHub`] broadcasts. Lines from the client are read only to answer `bye`,
//! `quit` and `DX`; nothing goes anywhere. A slow client lags behind the broadcast and loses
//! lines; it never holds up the others.

use std::io;
use std::net::{IpAddr, Ipv4Addr, TcpListener as StdTcpListener};
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio::task::{JoinHandle, JoinSet};

use super::line::LINE_END;
use crate::listeners::{Listener, ListenerStatus};
use crate::settings::DEFAULT_CLUSTER_PORT;

/// The last port tried when the default one is taken (contract 11.1).
pub const LAST_FALLBACK_PORT: u16 = 7399;
/// A line from a client longer than this is dropped (contract 11.2).
pub const MAX_LINE: usize = 512;
/// A client whose writes take longer than this is closed.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest [`ClusterServer::close`] waits for the closed connections to go.
const CLOSE_WAIT: Duration = Duration::from_secs(1);
/// Spot lines waiting for the slowest client before it starts losing them.
const BACKLOG: usize = 256;
/// The reply to a spot a logger tries to send (contract 11.3).
pub const REFUSE_DX: &str = "Spots can't be sent through Long Lines Relay.";
/// The node name in the prompt.
pub const NODE: &str = "LONGLINES";

/// What the server and the engine share: the spot lines, the connection count and the device
/// name for the greeting. Outlives any one server, so a rebind keeps it.
#[derive(Debug)]
pub struct ClusterHub {
    lines: broadcast::Sender<Arc<str>>,
    clients: watch::Sender<usize>,
    device_name: Mutex<String>,
}

impl ClusterHub {
    pub fn new(device_name: impl Into<String>) -> Arc<Self> {
        let (lines, _) = broadcast::channel(BACKLOG);
        Arc::new(Self {
            lines,
            clients: watch::Sender::new(0),
            device_name: Mutex::new(device_name.into()),
        })
    }

    /// Sends one spot line (CR LF included) to every logged-in client; returns how many.
    pub fn send_line(&self, line: impl Into<Arc<str>>) -> usize {
        self.lines.send(line.into()).unwrap_or(0)
    }

    /// Connections open now, logged in or not.
    pub fn clients(&self) -> usize {
        *self.clients.borrow()
    }

    /// Moves whenever the connection count changes.
    pub fn watch_clients(&self) -> watch::Receiver<usize> {
        self.clients.subscribe()
    }

    /// The relay's device name, as the greeting names it.
    pub fn set_device_name(&self, name: &str) {
        *self.device_name.lock().unwrap_or_else(|e| e.into_inner()) = name.to_string();
    }

    fn device_name(&self) -> String {
        self.device_name
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn connected(self: &Arc<Self>) -> Connection {
        self.clients.send_modify(|n| *n += 1);
        Connection(Arc::clone(self))
    }
}

/// Counts one connection while it lives.
struct Connection(Arc<ClusterHub>);

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.clients.send_modify(|n| *n = n.saturating_sub(1));
    }
}

#[derive(Debug)]
struct Running {
    status: ListenerStatus,
    task: Option<JoinHandle<()>>,
}

/// The DX cluster's listener.
#[derive(Debug)]
pub struct ClusterServer {
    hub: Arc<ClusterHub>,
    port: u16,
    lan: bool,
    running: Mutex<Running>,
}

impl ClusterServer {
    /// `port` 0 binds an ephemeral port (tests). Only [`DEFAULT_CLUSTER_PORT`] falls through to
    /// the next ports.
    pub fn new(hub: Arc<ClusterHub>, port: u16, lan: bool) -> Self {
        Self {
            hub,
            port,
            lan,
            running: Mutex::new(Running {
                status: ListenerStatus::Stopped { port },
                task: None,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The configured port (the bound one is in [`Listener::status`]).
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn lan(&self) -> bool {
        self.lan
    }

    fn ports(&self) -> RangeInclusive<u16> {
        if self.port == DEFAULT_CLUSTER_PORT {
            DEFAULT_CLUSTER_PORT..=LAST_FALLBACK_PORT
        } else {
            self.port..=self.port
        }
    }
}

fn bind(ip: IpAddr, port: u16) -> io::Result<TcpListener> {
    let listener = StdTcpListener::bind((ip, port))?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// Binds the first port of `ports` that is free. A port that is taken (or, on Windows, reserved:
/// access denied) moves on to the next; any other error stops.
pub fn bind_first(ip: IpAddr, ports: RangeInclusive<u16>) -> io::Result<TcpListener> {
    let mut last = io::Error::new(io::ErrorKind::AddrInUse, "no port to try");
    for port in ports {
        match bind(ip, port) {
            Ok(listener) => return Ok(listener),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
                ) =>
            {
                last = e;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

impl ClusterServer {
    /// Stops listening and waits until the port is free again and the connections are closed,
    /// so a listener started right after binds the same port. [`Listener::stop`] only asks the
    /// accept task to end; the socket stays bound until the runtime drops that task, so a rebind
    /// on the same port (the network setting, the switch turned off and on) would find it taken.
    pub async fn close(&self) {
        let task = {
            let mut running = self.lock();
            running.status = ListenerStatus::Stopped { port: self.port };
            running.task.take()
        };
        let Some(task) = task else { return };
        task.abort();
        // Cancelled: the accept loop, its listener and its connection set are dropped.
        let _ = task.await;
        // The connections' tasks were aborted with the set; wait (briefly) until they are gone.
        let mut clients = self.hub.watch_clients();
        let _ = tokio::time::timeout(CLOSE_WAIT, clients.wait_for(|n| *n == 0)).await;
    }
}

impl Listener for ClusterServer {
    fn start(&self) -> ListenerStatus {
        let mut running = self.lock();
        if let Some(task) = running.task.take() {
            task.abort();
        }
        let ip = if self.lan {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        let listener = match bind_first(ip, self.ports()) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(port = self.port, error = %e, kind = ?e.kind(), "couldn't bind the DX cluster port");
                running.status = ListenerStatus::PortInUse { port: self.port };
                return running.status.clone();
            }
        };
        let port = listener.local_addr().map_or(self.port, |a| a.port());
        tracing::info!(port, lan = self.lan, "DX cluster listening");
        running.task = Some(tokio::spawn(accept(listener, Arc::clone(&self.hub))));
        running.status = ListenerStatus::Listening { port };
        running.status.clone()
    }

    /// Stops listening and closes every connection.
    fn stop(&self) {
        let mut running = self.lock();
        if let Some(task) = running.task.take() {
            task.abort();
        }
        running.status = ListenerStatus::Stopped { port: self.port };
    }

    fn status(&self) -> ListenerStatus {
        self.lock().status.clone()
    }
}

impl Drop for ClusterServer {
    fn drop(&mut self) {
        if let Some(task) = self.lock().task.take() {
            task.abort();
        }
    }
}

/// Accepts connections until aborted; aborting it drops the set, which closes every session.
async fn accept(listener: TcpListener, hub: Arc<ClusterHub>) {
    let mut sessions = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((socket, peer)) => {
                    tracing::info!(peer = %peer.ip(), "a program connected to the DX cluster");
                    sessions.spawn(session(socket, Arc::clone(&hub)));
                }
                Err(e) => {
                    // Out of file descriptors and the like: wait instead of spinning.
                    tracing::debug!(error = %e, "couldn't accept a DX cluster connection");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            Some(_) = sessions.join_next(), if !sessions.is_empty() => {}
        }
    }
}

/// `K4ABC de LONGLINES 28-Oct-2026 2114Z >`, CR LF included.
fn prompt(callsign: &str) -> String {
    format!(
        "{callsign} de {NODE} {} >{LINE_END}",
        Utc::now().format("%d-%b-%Y %H%MZ")
    )
}

/// Printable ASCII only: anything else becomes `?`.
fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| {
            if (' '..='\u{7e}').contains(&c) {
                c
            } else {
                '?'
            }
        })
        .collect()
}

/// What a line from a logged-in client asks for (contract 11.3).
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Empty or only spaces: N1MM's keep-alive. No reply.
    Nothing,
    /// `bye` or `quit`.
    Close,
    /// A spot the logger tries to send.
    RefuseSpot,
    /// Anything else: a prompt.
    Prompt,
}

pub fn command(line: &str) -> Command {
    match line.split_whitespace().next() {
        None => Command::Nothing,
        Some(w) if w.eq_ignore_ascii_case("bye") || w.eq_ignore_ascii_case("quit") => {
            Command::Close
        }
        Some(w) if w.eq_ignore_ascii_case("dx") => Command::RefuseSpot,
        Some(_) => Command::Prompt,
    }
}

async fn write(writer: &mut OwnedWriteHalf, text: &str) -> io::Result<()> {
    match tokio::time::timeout(WRITE_TIMEOUT, writer.write_all(text.as_bytes())).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "client too slow")),
    }
}

async fn session(socket: TcpStream, hub: Arc<ClusterHub>) {
    let _connection = hub.connected();
    let _ = socket.set_nodelay(true);
    let (reader, mut writer) = socket.into_split();
    if let Err(e) = run_session(LineReader::new(reader), &mut writer, &hub).await {
        tracing::debug!(error = %e, "a DX cluster connection ended");
    }
    tracing::info!("a program left the DX cluster");
}

async fn run_session<R: AsyncRead + Unpin>(
    mut lines: LineReader<R>,
    writer: &mut OwnedWriteHalf,
    hub: &ClusterHub,
) -> io::Result<()> {
    let callsign = loop {
        write(writer, "login: ").await?;
        let Some(line) = lines.next_line().await? else {
            return Ok(());
        };
        if let Some(word) = line.split_whitespace().next() {
            break ascii(&word.to_uppercase());
        }
    };
    // Subscribed before the greeting: every spot after login reaches this client.
    let mut spots = hub.lines.subscribe();
    let greeting = format!(
        "Hello {callsign}, this is Long Lines Relay on {}. Spots from your Long Lines connections appear here.{LINE_END}",
        ascii(&hub.device_name())
    );
    write(writer, &greeting).await?;
    write(writer, &prompt(&callsign)).await?;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { return Ok(()) };
                match command(&line) {
                    Command::Nothing => {}
                    Command::Close => return Ok(()),
                    Command::RefuseSpot => {
                        write(writer, &format!("{REFUSE_DX}{LINE_END}")).await?;
                        write(writer, &prompt(&callsign)).await?;
                    }
                    Command::Prompt => write(writer, &prompt(&callsign)).await?,
                }
            }
            spot = spots.recv() => match spot {
                Ok(line) => write(writer, &line).await?,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::info!(lines = n, "a slow DX cluster client lost spot lines");
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Telnet {
    Data,
    /// After IAC (255).
    Command,
    /// After IAC WILL/WONT/DO/DONT: the option byte.
    Option,
    /// Inside IAC SB ... IAC SE.
    Sub,
    /// IAC inside a subnegotiation.
    SubCommand,
}

/// Turns a client's bytes into lines (contract 11.2): telnet commands and NUL are stripped, CR
/// LF, LF and CR all end a line, and a line over [`MAX_LINE`] bytes is dropped.
#[derive(Debug)]
pub struct LineDecoder {
    state: Telnet,
    line: Vec<u8>,
    after_cr: bool,
    overflow: bool,
}

impl Default for LineDecoder {
    fn default() -> Self {
        Self {
            state: Telnet::Data,
            line: Vec::new(),
            after_cr: false,
            overflow: false,
        }
    }
}

impl LineDecoder {
    /// Feeds bytes; completed lines are appended to `out`.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<String>) {
        for &b in bytes {
            self.state = match (self.state, b) {
                (Telnet::Data, 255) => Telnet::Command,
                (Telnet::Data, _) => {
                    self.data(b, out);
                    Telnet::Data
                }
                (Telnet::Command, 251..=254) => Telnet::Option,
                (Telnet::Command, 250) => Telnet::Sub,
                // IAC IAC is a literal 255: not text a logger sends, so it is dropped too.
                (Telnet::Command, _) | (Telnet::Option, _) => Telnet::Data,
                (Telnet::Sub, 255) => Telnet::SubCommand,
                (Telnet::Sub, _) => Telnet::Sub,
                (Telnet::SubCommand, 240) => Telnet::Data,
                (Telnet::SubCommand, _) => Telnet::Sub,
            };
        }
    }

    fn data(&mut self, b: u8, out: &mut Vec<String>) {
        let after_cr = std::mem::take(&mut self.after_cr);
        match b {
            0 => self.after_cr = after_cr,
            b'\n' if after_cr => {}
            b'\r' | b'\n' => {
                self.after_cr = b == b'\r';
                let line = std::mem::take(&mut self.line);
                if !std::mem::take(&mut self.overflow) {
                    out.push(String::from_utf8_lossy(&line).into_owned());
                }
            }
            _ if self.overflow => {}
            _ if self.line.len() >= MAX_LINE => {
                self.overflow = true;
                self.line.clear();
            }
            _ => self.line.push(b),
        }
    }
}

/// Reads lines from a connection. Cancel-safe: bytes are decoded only after a read completes.
pub struct LineReader<R> {
    reader: R,
    decoder: LineDecoder,
    ready: std::collections::VecDeque<String>,
    buf: Box<[u8; 1024]>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: LineDecoder::default(),
            ready: std::collections::VecDeque::new(),
            buf: Box::new([0; 1024]),
        }
    }

    /// The next line, or `None` once the client has closed.
    pub async fn next_line(&mut self) -> io::Result<Option<String>> {
        loop {
            if let Some(line) = self.ready.pop_front() {
                return Ok(Some(line));
            }
            let n = self.reader.read(&mut self.buf[..]).await?;
            if n == 0 {
                return Ok(None);
            }
            let mut out = Vec::new();
            self.decoder.feed(&self.buf[..n], &mut out);
            self.ready.extend(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(chunks: &[&[u8]]) -> Vec<String> {
        let mut d = LineDecoder::default();
        let mut out = Vec::new();
        for c in chunks {
            d.feed(c, &mut out);
        }
        out
    }

    #[test]
    fn every_line_end() {
        assert_eq!(
            decode(&[b"a\r\nb\nc\rd\r", b"\ne\n"]),
            ["a", "b", "c", "d", "e"]
        );
        assert_eq!(decode(&[b"\r\n\r\n"]), ["", ""]);
        assert_eq!(decode(&[b"no end yet"]), Vec::<String>::new());
    }

    #[test]
    fn strips_telnet_and_nul() {
        // IAC DO ECHO, IAC WILL NAWS, IAC SB NAWS 0 80 0 24 IAC SE, IAC NOP, NUL after CR.
        let bytes = [
            255, 253, 1, b'K', 255, 251, 31, b'4', 255, 250, 31, 0, 80, 0, 24, 255, 240, b'A', 255,
            241, b'B', b'C', b'\r', 0, b's', b'h', b'/', b'd', b'x', b'\n',
        ];
        assert_eq!(decode(&[&bytes]), ["K4ABC", "sh/dx"]);
        // Split anywhere.
        let mut d = LineDecoder::default();
        let mut out = Vec::new();
        for b in bytes {
            d.feed(&[b], &mut out);
        }
        assert_eq!(out, ["K4ABC", "sh/dx"]);
    }

    #[test]
    fn drops_runaway_lines() {
        let long = vec![b'x'; MAX_LINE + 10];
        assert_eq!(decode(&[&long, b"\nok\n"]), ["ok"]);
        let exact = vec![b'y'; MAX_LINE];
        assert_eq!(decode(&[&exact, b"\n"]), ["y".repeat(MAX_LINE)]);
    }

    #[test]
    fn commands() {
        assert_eq!(command(""), Command::Nothing);
        assert_eq!(command("   "), Command::Nothing);
        assert_eq!(command("bye"), Command::Close);
        assert_eq!(command("QUIT now"), Command::Close);
        assert_eq!(
            command("DX de KK4PWJ: 14025.0 W1AW cq"),
            Command::RefuseSpot
        );
        assert_eq!(command("dx 14025.0 W1AW"), Command::RefuseSpot);
        assert_eq!(command("dxcc"), Command::Prompt);
        assert_eq!(command("set/page 0"), Command::Prompt);
        assert_eq!(command("sh/dx"), Command::Prompt);
    }

    #[test]
    fn prompts_name_the_time() {
        let p = prompt("K4ABC");
        assert!(p.starts_with("K4ABC de LONGLINES "));
        assert!(p.ends_with("Z >\r\n"));
        // "28-Oct-2026 2114Z": a two-digit day, an English month, four digits of time.
        let stamp = &p["K4ABC de LONGLINES ".len()..p.len() - " >\r\n".len()];
        assert_eq!(stamp.len(), "28-Oct-2026 2114Z".len(), "{stamp}");
    }

    #[tokio::test]
    async fn falls_through_only_from_the_default_port() {
        let holder = StdTcpListener::bind("127.0.0.1:0").unwrap();
        let held = holder.local_addr().unwrap().port();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(
            bind_first(ip, held..=held).unwrap_err().kind(),
            io::ErrorKind::AddrInUse
        );
        if let Some(last) = held.checked_add(20) {
            let next = bind_first(ip, held..=last).unwrap();
            assert_ne!(next.local_addr().unwrap().port(), held);
        }
        // A chosen port that is taken is in use, with no fall-through.
        let server = ClusterServer::new(ClusterHub::new("SHACK-PC"), held, false);
        assert_eq!(server.start(), ListenerStatus::PortInUse { port: held });
        assert_eq!(server.ports(), held..=held);
        let default = ClusterServer::new(ClusterHub::new("SHACK-PC"), DEFAULT_CLUSTER_PORT, false);
        assert_eq!(default.ports(), 7373..=7399);
    }

    /// `close` frees the port before it returns, so the same port binds again at once (a
    /// current-thread runtime never ran the aborted task in between).
    #[tokio::test]
    async fn close_frees_the_port_at_once() {
        let hub = ClusterHub::new("SHACK-PC");
        let first = ClusterServer::new(Arc::clone(&hub), 0, false);
        let ListenerStatus::Listening { port } = first.start() else {
            panic!("not listening")
        };
        let client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut clients = hub.watch_clients();
        tokio::time::timeout(Duration::from_secs(5), clients.wait_for(|n| *n == 1))
            .await
            .unwrap()
            .unwrap();
        first.close().await;
        assert_eq!(first.status(), ListenerStatus::Stopped { port: 0 });
        assert_eq!(hub.clients(), 0);
        for lan in [true, false] {
            let again = ClusterServer::new(Arc::clone(&hub), port, lan);
            assert_eq!(again.start(), ListenerStatus::Listening { port });
            again.close().await;
        }
        drop(client);
    }

    #[tokio::test]
    async fn counts_connections_and_stops() {
        let hub = ClusterHub::new("SHACK-PC");
        let server = ClusterServer::new(Arc::clone(&hub), 0, false);
        let ListenerStatus::Listening { port } = server.start() else {
            panic!("not listening")
        };
        let mut clients = hub.watch_clients();
        let client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), clients.wait_for(|n| *n == 1))
            .await
            .unwrap()
            .unwrap();
        server.stop();
        tokio::time::timeout(Duration::from_secs(5), clients.wait_for(|n| *n == 0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(server.status(), ListenerStatus::Stopped { port: 0 });
        drop(client);
    }
}
