//! Inbound listeners share one lifecycle so the feed card can show any of them.
//!
//! Phase 1 registers only [`udp::UdpListener`]. A phase 2 telnet server (a DX cluster on
//! loopback) implements the same trait.

pub mod udp;

use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// What a listener is doing, as the feed card shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ListenerStatus {
    /// Bound and receiving on `port`.
    Listening { port: u16 },
    /// Another program holds `port`. The relay never moves to another port by itself.
    PortInUse { port: u16 },
    /// Not started, or stopped.
    Stopped { port: u16 },
}

impl ListenerStatus {
    pub fn port(&self) -> u16 {
        match self {
            ListenerStatus::Listening { port }
            | ListenerStatus::PortInUse { port }
            | ListenerStatus::Stopped { port } => *port,
        }
    }
}

/// One datagram (or, later, one line) from a local program.
#[derive(Clone, Debug)]
pub struct Datagram {
    pub at: DateTime<Utc>,
    pub source: SocketAddr,
    pub bytes: Vec<u8>,
}

/// The shared lifecycle. `start` must be called inside a Tokio runtime.
pub trait Listener: Send + Sync {
    /// Binds and starts receiving. Calling it while running restarts it.
    fn start(&self) -> ListenerStatus;
    /// Stops receiving and releases the port.
    fn stop(&self);
    fn status(&self) -> ListenerStatus;
}
