//! ZMQ connection state shared between the listener and the status API.
//!
//! Only facts the listener observes are recorded: socket monitor events
//! for each endpoint and the time of the last message received on it.
//! "connected" means the ZMQ session with Bitcoin Core's publisher is up;
//! it does not prove Core will publish anything on it.

use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bitcoincore_zmq::SocketEvent;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    /// The listener has not subscribed yet (initial sync still running).
    NotStarted,
    /// Subscribed; no connection established yet.
    Connecting,
    Connected,
    /// Was connected, or a connection attempt failed; ZMQ keeps retrying.
    Disconnected,
    /// No endpoint configured (`ZMQ_TX_URL` unset).
    NotConfigured,
}

impl EndpointState {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Disconnected,
            4 => Self::NotConfigured,
            _ => Self::NotStarted,
        }
    }
}

#[derive(Debug)]
pub struct EndpointStatus {
    state: AtomicU8,
    /// Unix seconds; 0 = no message received yet.
    last_message_at: AtomicI64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EndpointSnapshot {
    pub status: EndpointState,
    pub last_message_at: Option<i64>,
}

impl EndpointStatus {
    fn new(state: EndpointState) -> Self {
        Self {
            state: AtomicU8::new(state as u8),
            last_message_at: AtomicI64::new(0),
        }
    }

    pub fn set(&self, state: EndpointState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    /// Updates the state from a socket monitor event. Events that say
    /// nothing about the connection (listening, accepted, …) are ignored.
    pub fn apply(&self, event: &SocketEvent) {
        match event {
            SocketEvent::Connected { .. } | SocketEvent::HandshakeSucceeded => {
                self.set(EndpointState::Connected)
            }
            SocketEvent::Disconnected { .. }
            | SocketEvent::Closed { .. }
            | SocketEvent::ConnectRetried { .. }
            | SocketEvent::HandshakeFailedNoDetail { .. }
            | SocketEvent::HandshakeFailedProtocol { .. }
            | SocketEvent::HandshakeFailedAuth { .. } => self.set(EndpointState::Disconnected),
            _ => {}
        }
    }

    pub fn message_received(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.last_message_at.store(now, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> EndpointSnapshot {
        let last = self.last_message_at.load(Ordering::Relaxed);
        EndpointSnapshot {
            status: EndpointState::from_u8(self.state.load(Ordering::Relaxed)),
            last_message_at: (last > 0).then_some(last),
        }
    }
}

#[derive(Debug)]
pub struct ZmqStatus {
    pub blocks: EndpointStatus,
    pub transactions: EndpointStatus,
}

impl ZmqStatus {
    pub fn new(transactions_configured: bool) -> Self {
        Self {
            blocks: EndpointStatus::new(EndpointState::NotStarted),
            transactions: EndpointStatus::new(if transactions_configured {
                EndpointState::NotStarted
            } else {
                EndpointState::NotConfigured
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_events_drive_endpoint_state() {
        let endpoint = EndpointStatus::new(EndpointState::Connecting);

        endpoint.apply(&SocketEvent::ConnectDelayed);
        assert_eq!(endpoint.snapshot().status, EndpointState::Connecting);

        endpoint.apply(&SocketEvent::HandshakeSucceeded);
        assert_eq!(endpoint.snapshot().status, EndpointState::Connected);

        endpoint.apply(&SocketEvent::Disconnected { fd: 7 });
        assert_eq!(endpoint.snapshot().status, EndpointState::Disconnected);

        endpoint.apply(&SocketEvent::Connected { fd: 8 });
        assert_eq!(endpoint.snapshot().status, EndpointState::Connected);
    }

    #[test]
    fn last_message_time_is_unset_until_a_message_arrives() {
        let endpoint = EndpointStatus::new(EndpointState::Connected);
        assert_eq!(endpoint.snapshot().last_message_at, None);

        endpoint.message_received();
        assert!(endpoint.snapshot().last_message_at.unwrap() > 1_700_000_000);
    }

    #[test]
    fn unconfigured_transaction_endpoint_says_so() {
        let status = ZmqStatus::new(false);
        assert_eq!(
            status.transactions.snapshot().status,
            EndpointState::NotConfigured
        );
        assert_eq!(status.blocks.snapshot().status, EndpointState::NotStarted);
    }
}
