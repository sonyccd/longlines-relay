//! The UDP listener: binds `0.0.0.0:<port>` without address reuse, drops datagrams that don't come
//! from loopback, and hands the rest to the engine over a lossless channel.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{Datagram, Listener, ListenerStatus};
use crate::source_filter::SourceFilter;

/// Large enough for any UDP datagram.
const MAX_DATAGRAM: usize = 65_536;

#[derive(Debug)]
struct Running {
    status: ListenerStatus,
    task: Option<JoinHandle<()>>,
}

/// Receives logger broadcasts on one UDP port.
#[derive(Debug)]
pub struct UdpListener {
    port: u16,
    filter: Arc<SourceFilter>,
    tx: mpsc::Sender<Datagram>,
    running: Mutex<Running>,
}

impl UdpListener {
    /// Port 0 binds an ephemeral port (tests); the status reports the bound port.
    pub fn new(port: u16, filter: Arc<SourceFilter>, tx: mpsc::Sender<Datagram>) -> Self {
        Self {
            port,
            filter,
            tx,
            running: Mutex::new(Running {
                status: ListenerStatus::Stopped { port },
                task: None,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stops receiving and waits until the socket is closed, so the port can be bound again at
    /// once. ([`Listener::stop`] only asks the receive task to end.)
    pub async fn shutdown(&self) {
        let task = {
            let mut running = self.lock();
            running.status = ListenerStatus::Stopped { port: self.port };
            running.task.take()
        };
        if let Some(task) = task {
            task.abort();
            // A cancelled task's future, and the socket it owns, is dropped before this returns.
            let _ = task.await;
        }
    }
}

/// `std` binds without `SO_REUSEADDR`, so a port another program holds fails here instead of
/// being shared.
fn bind(port: u16) -> io::Result<UdpSocket> {
    let socket = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
    socket.set_nonblocking(true)?;
    UdpSocket::from_std(socket)
}

impl Listener for UdpListener {
    fn start(&self) -> ListenerStatus {
        let mut running = self.lock();
        if running.task.as_ref().is_some_and(|t| !t.is_finished()) {
            // Rebinding here would race the old socket, which closes only once its task ends.
            return running.status.clone();
        }
        let socket = match bind(self.port) {
            Ok(s) => s,
            Err(e) => {
                // Any bind failure is shown as "in use": on Windows a port held exclusively
                // answers access denied, and the remedy is the same, another port.
                if matches!(running.status, ListenerStatus::PortInUse { .. }) {
                    tracing::debug!(port = self.port, error = %e, "the UDP port is still in use");
                } else {
                    tracing::warn!(port = self.port, error = %e, kind = ?e.kind(), "couldn't bind the UDP port");
                }
                running.status = ListenerStatus::PortInUse { port: self.port };
                return running.status.clone();
            }
        };
        let port = socket.local_addr().map_or(self.port, |a| a.port());
        tracing::info!(port, "listening for logger broadcasts");
        let filter = Arc::clone(&self.filter);
        let tx = self.tx.clone();
        running.task = Some(tokio::spawn(receive(socket, filter, tx)));
        running.status = ListenerStatus::Listening { port };
        running.status.clone()
    }

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

impl Drop for UdpListener {
    fn drop(&mut self) {
        if let Some(task) = self.lock().task.take() {
            task.abort();
        }
    }
}

async fn receive(socket: UdpSocket, filter: Arc<SourceFilter>, tx: mpsc::Sender<Datagram>) {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    loop {
        let (len, source): (usize, SocketAddr) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                // Windows reports ICMP port unreachable for an earlier send as a receive
                // error (WSAECONNRESET); none of these end the listener.
                tracing::debug!(error = %e, "UDP receive error");
                continue;
            }
        };
        if !filter.allows(source.ip()) {
            filter.record_drop();
            continue;
        }
        let datagram = Datagram {
            at: Utc::now(),
            source,
            bytes: buf[..len].to_vec(),
        };
        if tx.send(datagram).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn listener(port: u16, filter: SourceFilter) -> (UdpListener, mpsc::Receiver<Datagram>) {
        let (tx, rx) = mpsc::channel(16);
        (UdpListener::new(port, Arc::new(filter), tx), rx)
    }

    #[tokio::test]
    async fn receives_from_loopback() {
        let (l, mut rx) = listener(0, SourceFilter::new());
        let ListenerStatus::Listening { port } = l.start() else {
            panic!("not listening")
        };
        let sender = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(b"<contactinfo/>", ("127.0.0.1", port))
            .unwrap();
        let got = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.bytes, b"<contactinfo/>");
        assert!(got.source.ip().is_loopback());
        l.stop();
        assert_eq!(l.status(), ListenerStatus::Stopped { port: 0 });
    }

    #[tokio::test]
    async fn start_while_listening_keeps_the_socket() {
        let (l, mut rx) = listener(0, SourceFilter::new());
        let first = l.start();
        assert!(matches!(first, ListenerStatus::Listening { .. }));
        for _ in 0..20 {
            assert_eq!(l.start(), first);
        }
        let ListenerStatus::Listening { port } = first else {
            unreachable!()
        };
        StdUdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .send_to(b"<x/>", ("127.0.0.1", port))
            .unwrap();
        let got = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(got.unwrap().is_some());
    }

    #[tokio::test]
    async fn after_shutdown_the_same_port_binds_at_once() {
        let probe = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        for _ in 0..20 {
            let (l, _rx) = listener(port, SourceFilter::new());
            assert_eq!(l.start(), ListenerStatus::Listening { port });
            l.shutdown().await;
            assert_eq!(l.status(), ListenerStatus::Stopped { port });
        }
    }

    #[tokio::test]
    async fn a_held_port_is_in_use() {
        let holder = StdUdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = holder.local_addr().unwrap().port();
        let (l, _rx) = listener(port, SourceFilter::new());
        assert_eq!(l.start(), ListenerStatus::PortInUse { port });
        assert_eq!(l.status(), ListenerStatus::PortInUse { port });
    }
}
