//! The event bus every module publishes to (`tokio::sync::broadcast`).
//!
//! The UI bridge subscribes and forwards events to the window. A slow subscriber can lag and
//! miss events; it then reads [`crate::engine::Engine::snapshot`] again. Nothing that must not be
//! lost (datagrams, queued contacts) travels on the bus.

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::state::{Account, ClusterView, FeedView, SentRow, StreamRow, UpstreamStatus};
use crate::status::RowStatus;
use crate::upstream::auth::PairingView;

/// Why the relay is signed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignOutReason {
    /// The user chose Sign out.
    User,
    /// The server answered 401: the token was revoked or is unknown.
    Revoked,
}

/// Account events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthEvent {
    /// `GET /me` answered; the account is known (after sign-in, at launch, hourly).
    Account { account: Account },
    /// The token is gone from the credential store.
    SignedOut { reason: SignOutReason },
    /// Whether a signed-out token is still waiting to be deleted from the credential store.
    SignOutIncomplete { incomplete: bool },
}

/// Everything the relay announces.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// Any datagram from this computer arrived, readable or not.
    PacketReceived { at: DateTime<Utc> },
    /// A new row at the top of Contacts seen.
    ContactSeen(StreamRow),
    /// A row's status changed.
    ContactStatus {
        row_id: u64,
        status: RowStatus,
        text: String,
    },
    /// A row left the stream (an N1MM edit folds its delete into the replace).
    RowRemoved { row_id: u64 },
    /// The Logging feed card changed.
    Feed(FeedView),
    /// The sender's connection to Long Lines changed.
    Upstream(UpstreamStatus),
    /// Account changes.
    Auth(AuthEvent),
    /// The sign-in screen changed.
    Pairing(PairingView),
    /// The DX cluster card changed.
    Cluster(ClusterView),
    /// A new row at the top of Spots sent.
    SpotSent(SentRow),
}

/// A cloneable handle to the broadcast channel.
#[derive(Clone, Debug)]
pub struct Bus {
    tx: broadcast::Sender<Event>,
}

impl Bus {
    /// A bus that keeps up to `capacity` events for each subscriber.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self { tx }
    }

    /// Publishes to every current subscriber. No subscriber is fine.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_get_events_in_order() {
        let bus = Bus::new(8);
        let mut rx = bus.subscribe();
        bus.publish(Event::RowRemoved { row_id: 1 });
        bus.publish(Event::RowRemoved { row_id: 2 });
        assert_eq!(rx.recv().await.unwrap(), Event::RowRemoved { row_id: 1 });
        assert_eq!(rx.recv().await.unwrap(), Event::RowRemoved { row_id: 2 });
    }

    #[test]
    fn publishing_without_subscribers_is_fine() {
        Bus::default().publish(Event::RowRemoved { row_id: 1 });
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let json = serde_json::to_value(Event::Auth(AuthEvent::SignedOut {
            reason: SignOutReason::Revoked,
        }))
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "auth", "data": {"kind": "signed_out", "reason": "revoked"}})
        );
    }
}
