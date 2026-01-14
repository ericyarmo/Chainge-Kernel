//! Sync protocol state machine.
//!
//! Implements the anti-entropy algorithm for converging receipt sets.

use std::collections::{HashMap, HashSet};

use chainge_kernel_core::{canonical_bytes, validate_receipt, Receipt, ReceiptId, StreamId};
use chainge_kernel_store::{InsertResult, Store};

use crate::error::{Result, SyncError};
use crate::messages::{
    NodeId, ReceiptRequest, SeqRange, StreamHead, SyncErrorCode, SyncMessage, PROTOCOL_VERSION,
};
use crate::transport::Transport;

/// Result of a sync session.
#[derive(Debug, Default)]
pub struct SyncReport {
    /// Number of receipts sent to peer.
    pub sent_count: usize,
    /// Number of receipts received from peer.
    pub received_count: usize,
    /// Number of receipts that were duplicates.
    pub duplicate_count: usize,
    /// Number of receipts that failed validation.
    pub invalid_count: usize,
    /// Streams that were synced.
    pub streams_synced: HashSet<StreamId>,
    /// Whether sync completed successfully.
    pub success: bool,
    /// Error message if sync failed.
    pub error: Option<String>,
}

/// Configuration for sync behavior.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Timeout for waiting for peer messages.
    pub message_timeout: std::time::Duration,
    /// Maximum receipts to request in one batch.
    pub max_batch_size: usize,
    /// Whether to validate receipts before storing.
    pub validate_receipts: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            message_timeout: std::time::Duration::from_secs(30),
            max_batch_size: 50,
            validate_receipts: true,
        }
    }
}

/// Sync session state.
pub struct SyncSession<S: Store, T: Transport> {
    /// The local store.
    store: S,
    /// The transport layer.
    transport: T,
    /// Configuration.
    config: SyncConfig,
    /// Streams we're interested in (empty = all).
    streams_of_interest: Vec<StreamId>,
    /// Peer's node ID (set after Hello exchange).
    peer_id: Option<NodeId>,
    /// Our view of peer's stream heads.
    peer_heads: HashMap<StreamId, StreamHead>,
}

impl<S: Store, T: Transport> SyncSession<S, T> {
    /// Create a new sync session.
    pub fn new(store: S, transport: T, config: SyncConfig) -> Self {
        Self {
            store,
            transport,
            config,
            streams_of_interest: Vec::new(),
            peer_id: None,
            peer_heads: HashMap::new(),
        }
    }

    /// Set streams to sync (empty = all known streams).
    pub fn with_streams(mut self, streams: Vec<StreamId>) -> Self {
        self.streams_of_interest = streams;
        self
    }

    /// Run the sync protocol with a specific peer.
    pub async fn sync_with(&mut self, peer: &NodeId) -> Result<SyncReport> {
        let mut report = SyncReport::default();

        // Phase 1: Hello exchange
        self.send_hello(peer).await?;
        self.receive_hello(peer).await?;

        // Phase 2: Exchange stream heads
        self.send_stream_heads(peer).await?;
        self.receive_stream_heads().await?;

        // Phase 3: Compute what we need
        let needs = self.compute_needs().await?;

        // Phase 4: Request missing receipts
        if !needs.is_empty() {
            self.request_receipts(peer, &needs).await?;

            // Phase 5: Receive and process receipts
            let received = self.receive_receipts(&mut report).await?;

            // Phase 6: Acknowledge
            self.send_ack(peer, &received).await?;
        }

        // Phase 7: Handle incoming requests (peer may need our receipts)
        self.handle_peer_requests(peer, &mut report).await?;

        report.success = true;
        Ok(report)
    }

    /// Send Hello message.
    async fn send_hello(&self, peer: &NodeId) -> Result<()> {
        let msg = SyncMessage::Hello {
            node_id: self.transport.local_node_id(),
            protocol_version: PROTOCOL_VERSION,
            streams_of_interest: self.streams_of_interest.clone(),
        };
        self.transport.send(peer, msg).await
    }

    /// Receive and validate Hello from peer.
    async fn receive_hello(&mut self, peer: &NodeId) -> Result<()> {
        let timeout = self.config.message_timeout;
        let result = self.transport.recv_timeout(timeout).await?;

        match result {
            Some((from, SyncMessage::Hello { node_id, protocol_version, .. })) => {
                if &from != peer {
                    return Err(SyncError::InvalidMessage(
                        "Hello from unexpected peer".into(),
                    ));
                }
                if protocol_version != PROTOCOL_VERSION {
                    return Err(SyncError::VersionMismatch {
                        local: PROTOCOL_VERSION,
                        peer: protocol_version,
                    });
                }
                self.peer_id = Some(node_id);
                Ok(())
            }
            Some((_, msg)) => Err(SyncError::InvalidMessage(format!(
                "expected Hello, got {:?}",
                std::mem::discriminant(&msg)
            ))),
            None => Err(SyncError::Timeout("waiting for Hello".into())),
        }
    }

    /// Send our stream heads.
    async fn send_stream_heads(&self, peer: &NodeId) -> Result<()> {
        let all_heads = self.store.get_all_stream_heads().await?;

        let heads: Vec<StreamHead> = all_heads
            .into_iter()
            .filter(|(stream_id, _, _)| {
                self.streams_of_interest.is_empty()
                    || self.streams_of_interest.contains(stream_id)
            })
            .map(|(stream_id, head_seq, head_receipt_id)| StreamHead {
                stream_id,
                head_seq,
                head_receipt_id,
            })
            .collect();

        let msg = SyncMessage::StreamHeads { heads };
        self.transport.send(peer, msg).await
    }

    /// Receive peer's stream heads.
    async fn receive_stream_heads(&mut self) -> Result<()> {
        let timeout = self.config.message_timeout;
        let result = self.transport.recv_timeout(timeout).await?;

        match result {
            Some((_, SyncMessage::StreamHeads { heads })) => {
                for head in heads {
                    self.peer_heads.insert(head.stream_id, head);
                }
                Ok(())
            }
            Some((_, SyncMessage::Error { code, message })) => {
                Err(SyncError::PeerError { code, message })
            }
            Some((_, msg)) => Err(SyncError::InvalidMessage(format!(
                "expected StreamHeads, got {:?}",
                std::mem::discriminant(&msg)
            ))),
            None => Err(SyncError::Timeout("waiting for StreamHeads".into())),
        }
    }

    /// Compute what receipts we need from peer.
    async fn compute_needs(&self) -> Result<Vec<ReceiptRequest>> {
        let mut requests = Vec::new();

        for (stream_id, peer_head) in &self.peer_heads {
            // Get our state for this stream
            let our_head_seq = match self.store.get_stream_state(stream_id).await? {
                Some(state) => state.head_seq,
                None => 0, // We don't have this stream yet
            };

            // If peer is ahead, we need receipts
            if peer_head.head_seq > our_head_seq {
                requests.push(ReceiptRequest {
                    stream_id: *stream_id,
                    seqs: SeqRange::Range {
                        start: our_head_seq + 1,
                        end: peer_head.head_seq,
                    },
                });
            }

            // Also check for gaps we have
            let gaps = self.store.get_gaps(stream_id).await?;
            if !gaps.is_empty() {
                requests.push(ReceiptRequest {
                    stream_id: *stream_id,
                    seqs: SeqRange::List(gaps),
                });
            }
        }

        Ok(requests)
    }

    /// Request receipts from peer.
    async fn request_receipts(
        &self,
        peer: &NodeId,
        requests: &[ReceiptRequest],
    ) -> Result<()> {
        // Chunk requests to respect limits
        for chunk in requests.chunks(self.config.max_batch_size) {
            let msg = SyncMessage::NeedReceipts {
                requests: chunk.to_vec(),
            };
            self.transport.send(peer, msg).await?;
        }
        Ok(())
    }

    /// Receive and process receipts from peer.
    async fn receive_receipts(&self, report: &mut SyncReport) -> Result<Vec<ReceiptId>> {
        let mut received = Vec::new();
        let timeout = self.config.message_timeout;

        loop {
            let result = self.transport.recv_timeout(timeout).await?;

            match result {
                Some((_, SyncMessage::Receipts { receipts })) => {
                    for receipt in receipts {
                        match self.process_receipt(&receipt).await {
                            Ok(true) => {
                                received.push(receipt.compute_id());
                                report.received_count += 1;
                                report.streams_synced.insert(*receipt.stream_id());
                            }
                            Ok(false) => {
                                report.duplicate_count += 1;
                            }
                            Err(e) => {
                                report.invalid_count += 1;
                                tracing::warn!("Invalid receipt: {}", e);
                            }
                        }
                    }
                }
                Some((_, SyncMessage::Ack { .. })) => {
                    // Peer is done sending, we can stop waiting
                    break;
                }
                Some((_, SyncMessage::Error { code, message })) => {
                    return Err(SyncError::PeerError { code, message });
                }
                Some((_, _)) => {
                    // Ignore other messages while receiving
                    continue;
                }
                None => {
                    // Timeout - assume peer is done
                    break;
                }
            }
        }

        Ok(received)
    }

    /// Process a single receipt.
    ///
    /// Returns true if the receipt was new, false if duplicate.
    async fn process_receipt(&self, receipt: &Receipt) -> Result<bool> {
        // Validate if configured
        if self.config.validate_receipts {
            validate_receipt(receipt)?;
        }

        // Compute canonical bytes and insert
        let canonical = canonical_bytes(receipt);
        let result = self.store.insert_receipt(receipt, &canonical).await?;

        match result {
            InsertResult::Inserted => Ok(true),
            InsertResult::AlreadyExists => Ok(false),
            InsertResult::Conflict { existing } => {
                // This is a fork - store should handle it
                tracing::warn!(
                    "Fork detected at stream {:?} seq {}: existing={}, new={}",
                    receipt.stream_id(),
                    receipt.seq(),
                    existing.to_hex(),
                    receipt.compute_id().to_hex()
                );
                Ok(false)
            }
        }
    }

    /// Send acknowledgment.
    async fn send_ack(&self, peer: &NodeId, received: &[ReceiptId]) -> Result<()> {
        let msg = SyncMessage::Ack {
            received: received.to_vec(),
        };
        self.transport.send(peer, msg).await
    }

    /// Handle incoming requests from peer.
    async fn handle_peer_requests(
        &self,
        peer: &NodeId,
        report: &mut SyncReport,
    ) -> Result<()> {
        let timeout = self.config.message_timeout;

        loop {
            let result = self.transport.recv_timeout(timeout).await?;

            match result {
                Some((_, SyncMessage::NeedReceipts { requests })) => {
                    // Gather and send requested receipts
                    let mut receipts = Vec::new();

                    for request in requests {
                        let seqs = request.seqs.to_vec();
                        for seq in seqs {
                            if let Some(receipt) = self
                                .store
                                .get_receipt_by_position(&request.stream_id, seq)
                                .await?
                            {
                                receipts.push(receipt);
                                report.sent_count += 1;

                                // Send in batches
                                if receipts.len() >= self.config.max_batch_size {
                                    self.send_receipts(peer, &receipts).await?;
                                    receipts.clear();
                                }
                            }
                        }
                    }

                    // Send remaining
                    if !receipts.is_empty() {
                        self.send_receipts(peer, &receipts).await?;
                    }

                    // Signal we're done
                    self.send_ack(peer, &[]).await?;
                }
                Some((_, SyncMessage::Ack { .. })) => {
                    // Peer is done
                    break;
                }
                Some((_, SyncMessage::Error { code, message })) => {
                    return Err(SyncError::PeerError { code, message });
                }
                Some((_, _)) => {
                    // Ignore other messages
                    continue;
                }
                None => {
                    // Timeout - assume peer is done
                    break;
                }
            }
        }

        Ok(())
    }

    /// Send receipts to peer.
    async fn send_receipts(&self, peer: &NodeId, receipts: &[Receipt]) -> Result<()> {
        let msg = SyncMessage::Receipts {
            receipts: receipts.to_vec(),
        };
        self.transport.send(peer, msg).await
    }
}
