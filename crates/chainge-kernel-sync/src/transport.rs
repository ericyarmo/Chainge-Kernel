//! Transport abstraction for sync protocol.
//!
//! The transport layer handles message serialization and delivery.
//! Implementations may use WebSockets, HTTP, or any other transport.

use async_trait::async_trait;

use crate::error::SyncError;
use crate::messages::{NodeId, SyncMessage};

/// Result type for transport operations.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Transport trait for sending and receiving sync messages.
///
/// Implementations must be thread-safe (Send + Sync).
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a message to a specific peer.
    async fn send(&self, peer: &NodeId, message: SyncMessage) -> Result<()>;

    /// Receive the next message from any peer.
    ///
    /// Returns the sender's NodeId and the message.
    /// Blocks until a message is available or an error occurs.
    async fn recv(&self) -> Result<(NodeId, SyncMessage)>;

    /// Receive with timeout.
    ///
    /// Returns None if timeout expires before a message arrives.
    async fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Option<(NodeId, SyncMessage)>>;

    /// Broadcast a message to all connected peers.
    async fn broadcast(&self, message: SyncMessage) -> Result<()>;

    /// Get the local node's identity.
    fn local_node_id(&self) -> NodeId;

    /// List currently connected peers.
    async fn connected_peers(&self) -> Result<Vec<NodeId>>;

    /// Check if a specific peer is connected.
    async fn is_connected(&self, peer: &NodeId) -> bool;
}

/// A simple in-memory transport for testing.
///
/// Uses channels to simulate message passing between nodes.
pub mod memory {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::{mpsc, RwLock};

    /// Message envelope for internal routing.
    #[derive(Debug, Clone)]
    struct Envelope {
        from: NodeId,
        to: Option<NodeId>, // None = broadcast
        message: SyncMessage,
    }

    /// Shared state for the memory transport network.
    pub struct MemoryNetwork {
        /// Sender channels for each node.
        senders: RwLock<HashMap<NodeId, mpsc::Sender<Envelope>>>,
    }

    impl MemoryNetwork {
        /// Create a new memory network.
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                senders: RwLock::new(HashMap::new()),
            })
        }

        /// Create a transport connected to this network.
        pub async fn create_transport(self: &Arc<Self>, node_id: NodeId) -> MemoryTransport {
            let (tx, rx) = mpsc::channel(1000);

            self.senders.write().await.insert(node_id, tx);

            MemoryTransport {
                node_id,
                network: Arc::clone(self),
                receiver: RwLock::new(rx),
            }
        }
    }

    impl Default for MemoryNetwork {
        fn default() -> Self {
            Self {
                senders: RwLock::new(HashMap::new()),
            }
        }
    }

    /// In-memory transport implementation.
    pub struct MemoryTransport {
        node_id: NodeId,
        network: Arc<MemoryNetwork>,
        receiver: RwLock<mpsc::Receiver<Envelope>>,
    }

    #[async_trait]
    impl Transport for MemoryTransport {
        async fn send(&self, peer: &NodeId, message: SyncMessage) -> Result<()> {
            let senders = self.network.senders.read().await;
            if let Some(sender) = senders.get(peer) {
                let envelope = Envelope {
                    from: self.node_id,
                    to: Some(*peer),
                    message,
                };
                sender
                    .send(envelope)
                    .await
                    .map_err(|_| SyncError::TransportError("peer disconnected".into()))?;
            } else {
                return Err(SyncError::TransportError("peer not found".into()));
            }
            Ok(())
        }

        async fn recv(&self) -> Result<(NodeId, SyncMessage)> {
            let mut rx = self.receiver.write().await;
            match rx.recv().await {
                Some(envelope) => Ok((envelope.from, envelope.message)),
                None => Err(SyncError::TransportError("channel closed".into())),
            }
        }

        async fn recv_timeout(
            &self,
            timeout: std::time::Duration,
        ) -> Result<Option<(NodeId, SyncMessage)>> {
            let mut rx = self.receiver.write().await;
            match tokio::time::timeout(timeout, rx.recv()).await {
                Ok(Some(envelope)) => Ok(Some((envelope.from, envelope.message))),
                Ok(None) => Err(SyncError::TransportError("channel closed".into())),
                Err(_) => Ok(None), // Timeout
            }
        }

        async fn broadcast(&self, message: SyncMessage) -> Result<()> {
            let senders = self.network.senders.read().await;
            for (peer_id, sender) in senders.iter() {
                if peer_id != &self.node_id {
                    let envelope = Envelope {
                        from: self.node_id,
                        to: None, // Broadcast
                        message: message.clone(),
                    };
                    // Ignore errors for broadcast (some peers may have disconnected)
                    let _ = sender.send(envelope).await;
                }
            }
            Ok(())
        }

        fn local_node_id(&self) -> NodeId {
            self.node_id
        }

        async fn connected_peers(&self) -> Result<Vec<NodeId>> {
            let senders = self.network.senders.read().await;
            Ok(senders
                .keys()
                .filter(|id| *id != &self.node_id)
                .copied()
                .collect())
        }

        async fn is_connected(&self, peer: &NodeId) -> bool {
            let senders = self.network.senders.read().await;
            senders.contains_key(peer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::memory::MemoryNetwork;
    use crate::messages::PROTOCOL_VERSION;

    #[tokio::test]
    async fn test_memory_transport_send_recv() {
        let network = MemoryNetwork::new();

        let node_a = NodeId::from_bytes([0xAA; 32]);
        let node_b = NodeId::from_bytes([0xBB; 32]);

        let transport_a = network.create_transport(node_a).await;
        let transport_b = network.create_transport(node_b).await;

        // Send from A to B
        let msg = SyncMessage::Hello {
            node_id: node_a,
            protocol_version: PROTOCOL_VERSION,
            streams_of_interest: vec![],
        };

        transport_a.send(&node_b, msg.clone()).await.unwrap();

        // Receive on B
        let (from, received) = transport_b.recv().await.unwrap();
        assert_eq!(from, node_a);

        if let SyncMessage::Hello { node_id, .. } = received {
            assert_eq!(node_id, node_a);
        } else {
            panic!("expected Hello message");
        }
    }

    #[tokio::test]
    async fn test_memory_transport_broadcast() {
        let network = MemoryNetwork::new();

        let node_a = NodeId::from_bytes([0xAA; 32]);
        let node_b = NodeId::from_bytes([0xBB; 32]);
        let node_c = NodeId::from_bytes([0xCC; 32]);

        let transport_a = network.create_transport(node_a).await;
        let transport_b = network.create_transport(node_b).await;
        let transport_c = network.create_transport(node_c).await;

        // Broadcast from A
        let msg = SyncMessage::Hello {
            node_id: node_a,
            protocol_version: PROTOCOL_VERSION,
            streams_of_interest: vec![],
        };

        transport_a.broadcast(msg).await.unwrap();

        // Both B and C should receive
        let (from_b, _) = transport_b.recv().await.unwrap();
        let (from_c, _) = transport_c.recv().await.unwrap();

        assert_eq!(from_b, node_a);
        assert_eq!(from_c, node_a);
    }
}
