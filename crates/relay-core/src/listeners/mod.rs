//! Inbound listeners share one lifecycle so the window can show any of them.
//!
//! [`udp::UdpListener`] receives logger broadcasts for the contacts pipeline; the DX cluster's
//! [`crate::cluster::server::ClusterServer`] implements the same trait but never hands anything
//! to that pipeline.

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

/// One datagram from a local program.
#[derive(Clone, Debug)]
pub struct Datagram {
    pub at: DateTime<Utc>,
    pub source: SocketAddr,
    pub bytes: Vec<u8>,
}

/// The shared lifecycle. `start` must be called inside a Tokio runtime.
pub trait Listener: Send + Sync {
    /// Binds and starts receiving. While it is already receiving it keeps the socket it has; after
    /// a failed bind it tries again.
    fn start(&self) -> ListenerStatus;
    /// Stops receiving. The port is released once the receive task has ended.
    fn stop(&self);
    fn status(&self) -> ListenerStatus;
}
