//! The local DX cluster (contract sections 10 and 11): the spot line against the real DXSpider
//! capture, and a program connecting over TCP to an engine whose spots come from a fake stream.

mod support;

use std::time::Duration;

use relay_core::cluster::line::{format_line, parse_line};
use relay_core::engine::{ClusterError, Engine};
use relay_core::listeners::ListenerStatus;
use relay_core::state::{ClusterStatus, ClusterUpstream};
use support::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const LINES: &str = include_str!("fixtures/dxspider-lines.txt");

/// Every `DX de` line of the capture comes out byte for byte when parsed into its parts and
/// formatted again (contract 10.3). The capture's lines end in two BELs and LF; served lines end
/// in CR LF and nothing else.
#[test]
fn the_dxspider_capture_round_trips() {
    let lines: Vec<&str> = LINES.lines().collect();
    assert_eq!(lines.len(), 23);
    for captured in lines {
        let served = captured.trim_end_matches('\u{7}');
        assert_ne!(served, captured, "the capture keeps DXSpider's BELs");
        let parts = parse_line(captured).unwrap_or_else(|| panic!("fits the layout: {captured}"));
        assert_eq!(format_line(&parts), format!("{served}\r\n"), "{captured}");
    }
}

#[test]
fn the_capture_includes_a_cut_comment_ending_in_a_space() {
    let third = LINES.lines().nth(3).unwrap();
    let parts = parse_line(third).unwrap();
    assert_eq!(parts.callsign, "M7VDP");
    assert_eq!(parts.comment, "B/G-1042 ROC Group HQ No 12 -");
    assert_eq!(&third[39..69], "B/G-1042 ROC Group HQ No 12 - ");
}

struct Setup {
    server: FakeLongLines,
    stream: FakeStream,
    engine: Engine,
    port: u16,
    _dir: tempfile::TempDir,
}

async fn setup() -> Setup {
    let server = FakeLongLines::start().await;
    let stream = FakeStream::start().await;
    server.serve_v2(&stream, &BOTH_SCOPES);
    let dir = tempfile::tempdir().unwrap();
    let engine = start_engine_with(
        &dir.path().join("q.sqlite3"),
        &server.uri(),
        signed_in(),
        |config| {
            config.cluster_enabled = true;
            config.cluster_port = 0;
        },
    )
    .await;
    wait_signed_in(&engine).await;
    let ListenerStatus::Listening { port } = engine.snapshot().cluster.listener else {
        panic!(
            "the cluster isn't listening: {:?}",
            engine.snapshot().cluster
        )
    };
    stream.wait("the stream to open", |st| st.open == 1).await;
    wait_until(&engine, "hello", |s| {
        s.cluster.upstream == ClusterUpstream::Connected
    })
    .await;
    Setup {
        server,
        stream,
        engine,
        port,
        _dir: dir,
    }
}

/// A cluster client: reads exact byte counts or CR LF lines, with a timeout.
struct Client {
    socket: TcpStream,
    pending: Vec<u8>,
}

impl Client {
    async fn connect(port: u16) -> Self {
        Self {
            socket: TcpStream::connect(("127.0.0.1", port)).await.unwrap(),
            pending: Vec::new(),
        }
    }

    async fn fill(&mut self) -> bool {
        let mut buf = [0u8; 1024];
        match tokio::time::timeout(Duration::from_secs(5), self.socket.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => false,
            Ok(Ok(n)) => {
                self.pending.extend_from_slice(&buf[..n]);
                true
            }
            Err(_) => panic!(
                "nothing arrived; have {:?}",
                String::from_utf8_lossy(&self.pending)
            ),
        }
    }

    async fn read_exact_text(&mut self, len: usize) -> String {
        while self.pending.len() < len {
            assert!(self.fill().await, "closed early");
        }
        let rest = self.pending.split_off(len);
        String::from_utf8(std::mem::replace(&mut self.pending, rest)).unwrap()
    }

    /// One line, CR LF included.
    async fn line(&mut self) -> String {
        loop {
            if let Some(i) = self.pending.windows(2).position(|w| w == b"\r\n") {
                return self.read_exact_text(i + 2).await;
            }
            assert!(self.fill().await, "closed early");
        }
    }

    async fn send(&mut self, text: &str) {
        self.socket.write_all(text.as_bytes()).await.unwrap();
    }

    /// True when nothing arrives for `wait`.
    async fn silent_for(&mut self, wait: Duration) -> bool {
        let mut buf = [0u8; 64];
        self.pending.is_empty()
            && tokio::time::timeout(wait, self.socket.read(&mut buf))
                .await
                .is_err()
    }

    async fn closed(&mut self) -> bool {
        let mut buf = [0u8; 64];
        matches!(
            tokio::time::timeout(Duration::from_secs(5), self.socket.read(&mut buf)).await,
            Ok(Ok(0)) | Ok(Err(_))
        )
    }
}

fn assert_prompt(line: &str, callsign: &str) {
    let stamp = line
        .strip_prefix(&format!("{callsign} de LONGLINES "))
        .and_then(|rest| rest.strip_suffix(" >\r\n"))
        .unwrap_or_else(|| panic!("not a prompt: {line:?}"));
    // 28-Oct-2026 2114Z
    let b = stamp.as_bytes();
    assert_eq!(b.len(), 17, "{stamp}");
    assert!(b[..2].iter().all(u8::is_ascii_digit) && b[2] == b'-' && b[6] == b'-');
    assert!(b[7..11].iter().all(u8::is_ascii_digit) && b[11] == b' ' && b[16] == b'Z');
}

#[tokio::test]
async fn a_logger_logs_in_and_receives_spot_lines() {
    let s = setup().await;
    let mut client = Client::connect(s.port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");
    // An empty line asks again.
    client.send("\r\n").await;
    assert_eq!(client.read_exact_text(7).await, "login: ");
    client.send("k4abc-2 extra\r\n").await;
    assert_eq!(
        client.line().await,
        "Hello K4ABC-2, this is Long Lines Relay on SHACK-PC. Spots from your Long Lines connections appear here.\r\n"
    );
    assert_prompt(&client.line().await, "K4ABC-2");
    wait_until(&s.engine, "one program connected", |snap| {
        snap.cluster.clients == 1
    })
    .await;

    // N1MM's start-up commands get a prompt; its keep-alive (a blank line) gets nothing.
    client.send("set/page 0\r\n").await;
    assert_prompt(&client.line().await, "K4ABC-2");
    client.send("sh/dx\n").await;
    assert_prompt(&client.line().await, "K4ABC-2");
    client.send("\r\n").await;
    // A spot the logger tries to send is refused and goes nowhere.
    client.send("DX de K4ABC: 14025.0 W1AW cq test\r\n").await;
    assert_eq!(
        client.line().await,
        "Spots can't be sent through Long Lines Relay.\r\n"
    );
    assert_prompt(&client.line().await, "K4ABC-2");

    // A spot from the stream arrives as the exact line.
    s.stream.send(spot_event(&example_spot()));
    assert_eq!(
        client.line().await,
        "DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z\r\n"
    );
    let snap = wait_until(&s.engine, "the sent row", |snap| snap.sent.len() == 1).await;
    assert_eq!(snap.cluster.status, ClusterStatus::Running);

    // The test spot, from the account's callsign.
    let row = s.engine.send_test_spot().unwrap();
    assert!(row.test);
    let line = client.line().await;
    assert!(
        line.starts_with("DX de KK4PWJ:    14050.0  LLTEST       CW Long Lines Relay test spot  "),
        "{line:?}"
    );
    assert!(line.ends_with("Z\r\n") && line.len() == 77, "{line:?}");
    assert_eq!(s.engine.snapshot().sent.len(), 2);

    // Nothing the client sent became a contact or a request.
    assert!(s.server.requests().is_empty());
    assert!(s.engine.snapshot().stream.is_empty());

    client.send("bye\r\n").await;
    assert!(client.closed().await);
    wait_until(&s.engine, "no program connected", |snap| {
        snap.cluster.clients == 0
    })
    .await;
    s.engine.shutdown().await;
}

#[tokio::test]
async fn only_logged_in_programs_get_spots_and_only_new_ones() {
    let s = setup().await;
    s.stream.send(spot_event(&example_spot()));
    wait_until(&s.engine, "the first spot", |snap| snap.sent.len() == 1).await;

    let mut waiting = Client::connect(s.port).await;
    assert_eq!(waiting.read_exact_text(7).await, "login: ");
    let mut other = example_spot();
    other["id"] = serde_json::json!(2);
    other["callsign"] = serde_json::json!("W9XYZ");
    s.stream.send(spot_event(&other));
    wait_until(&s.engine, "the second spot", |snap| snap.sent.len() == 2).await;
    assert!(waiting.silent_for(Duration::from_millis(300)).await);

    // Logging in now shows no history.
    waiting.send("N0CALL\r\n").await;
    assert!(waiting.line().await.starts_with("Hello N0CALL,"));
    assert_prompt(&waiting.line().await, "N0CALL");
    assert!(waiting.silent_for(Duration::from_millis(300)).await);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn the_switch_and_the_network_setting_rebind() {
    let s = setup().await;
    let mut client = Client::connect(s.port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");

    // Off closes every connection and the stream.
    let view = s.engine.set_cluster_enabled(false).await;
    assert_eq!(view.status, ClusterStatus::Off);
    assert!(client.closed().await);
    s.stream
        .wait("the stream to close", |st| st.open == 0)
        .await;
    assert_eq!(s.engine.send_test_spot(), Err(ClusterError::NotListening));
    assert!(TcpStream::connect(("127.0.0.1", s.port)).await.is_err());

    // On again listens again.
    let view = s.engine.set_cluster_enabled(true).await;
    let ListenerStatus::Listening { port } = view.listener else {
        panic!("not listening: {view:?}")
    };
    let mut client = Client::connect(port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");

    // The network setting rebinds, which closes the connection, and still answers on loopback.
    let view = s.engine.set_cluster_lan(true).await;
    assert!(view.lan);
    assert!(client.closed().await);
    let ListenerStatus::Listening { port } = view.listener else {
        panic!("not listening: {view:?}")
    };
    let mut client = Client::connect(port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");
    let view = s.engine.set_cluster_lan(false).await;
    assert!(!view.lan);
    assert_eq!(view.addresses.lan, None);
    assert!(client.closed().await);
    s.engine.shutdown().await;
}

#[tokio::test]
async fn a_chosen_port_that_is_taken_is_in_use() {
    let s = setup().await;
    let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let held = holder.local_addr().unwrap().port();
    let view = s.engine.set_cluster_port(held).await;
    assert_eq!(view.status, ClusterStatus::PortInUse);
    assert_eq!(view.port, held);
    drop(holder);
    let view = s.engine.set_cluster_port(0).await;
    assert!(matches!(view.listener, ListenerStatus::Listening { .. }));
    s.engine.shutdown().await;
}

/// The network setting and the switch rebind on the same chosen port at once: the old listener
/// lets go of it before the new one binds.
#[tokio::test]
async fn rebinding_keeps_a_chosen_port() {
    let s = setup().await;
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    let listening = ListenerStatus::Listening { port };
    assert_eq!(s.engine.set_cluster_port(port).await.listener, listening);
    let mut client = Client::connect(port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");

    let view = s.engine.set_cluster_lan(true).await;
    assert_eq!(view.listener, listening);
    assert_eq!(view.status, ClusterStatus::Listening);
    assert!(client.closed().await);
    assert_eq!(s.engine.set_cluster_lan(false).await.listener, listening);
    assert_eq!(
        s.engine.set_cluster_enabled(false).await.status,
        ClusterStatus::Off
    );
    assert_eq!(s.engine.set_cluster_enabled(true).await.listener, listening);
    assert_eq!(s.engine.set_cluster_port(port).await.listener, listening);
    let mut client = Client::connect(port).await;
    assert_eq!(client.read_exact_text(7).await, "login: ");
    s.engine.shutdown().await;
}
