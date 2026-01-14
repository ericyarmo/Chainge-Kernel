//! Sync protocol message types.
//!
//! These messages are exchanged between nodes to achieve receipt set convergence.

use serde::{Deserialize, Serialize};

use chainge_kernel_core::{Receipt, ReceiptId, StreamId};

/// Unique identifier for a node in the sync network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Generate a random node ID.
    pub fn random() -> Self {
        use rand::Rng;
        Self(rand::thread_rng().gen())
    }
}

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 0;

/// Message size limits (see SPEC_V0.md §5.2.1).
pub mod limits {
    /// Max streams in Hello.streams_of_interest.
    pub const MAX_STREAMS_OF_INTEREST: usize = 100;
    /// Max heads in StreamHeads.heads.
    pub const MAX_STREAM_HEADS: usize = 1000;
    /// Max requests in NeedReceipts.requests.
    pub const MAX_RECEIPT_REQUESTS: usize = 100;
    /// Max seqs in SeqRange::List.
    pub const MAX_SEQ_LIST: usize = 100;
    /// Max receipts in Receipts.receipts.
    pub const MAX_RECEIPTS_PER_MESSAGE: usize = 50;
    /// Max receipt IDs in Ack.received.
    pub const MAX_ACK_IDS: usize = 100;
}

/// Sync protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Discovery phase: introduce yourself.
    Hello {
        /// This node's identity.
        node_id: NodeId,
        /// Protocol version for compatibility checking.
        protocol_version: u8,
        /// Optional filter: only sync these streams.
        /// Empty means "all streams I know about".
        streams_of_interest: Vec<StreamId>,
    },

    /// State advertisement: share what you have.
    StreamHeads {
        /// Current head for each stream.
        heads: Vec<StreamHead>,
    },

    /// Request missing data.
    NeedReceipts {
        /// Batched requests for receipts.
        requests: Vec<ReceiptRequest>,
    },

    /// Provide requested data.
    Receipts {
        /// The receipts being sent.
        receipts: Vec<Receipt>,
    },

    /// Acknowledge receipt of data.
    Ack {
        /// Receipt IDs we successfully ingested.
        received: Vec<ReceiptId>,
    },

    /// Error condition.
    Error {
        /// Error code for programmatic handling.
        code: SyncErrorCode,
        /// Human-readable description.
        message: String,
    },
}

impl SyncMessage {
    /// Check if this message respects size limits.
    pub fn validate_limits(&self) -> Result<(), &'static str> {
        match self {
            SyncMessage::Hello { streams_of_interest, .. } => {
                if streams_of_interest.len() > limits::MAX_STREAMS_OF_INTEREST {
                    return Err("too many streams_of_interest");
                }
            }
            SyncMessage::StreamHeads { heads } => {
                if heads.len() > limits::MAX_STREAM_HEADS {
                    return Err("too many stream heads");
                }
            }
            SyncMessage::NeedReceipts { requests } => {
                if requests.len() > limits::MAX_RECEIPT_REQUESTS {
                    return Err("too many receipt requests");
                }
                for req in requests {
                    if let SeqRange::List(seqs) = &req.seqs {
                        if seqs.len() > limits::MAX_SEQ_LIST {
                            return Err("too many seqs in list");
                        }
                    }
                }
            }
            SyncMessage::Receipts { receipts } => {
                if receipts.len() > limits::MAX_RECEIPTS_PER_MESSAGE {
                    return Err("too many receipts");
                }
            }
            SyncMessage::Ack { received } => {
                if received.len() > limits::MAX_ACK_IDS {
                    return Err("too many ack IDs");
                }
            }
            SyncMessage::Error { .. } => {}
        }
        Ok(())
    }
}

/// Head state for a single stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamHead {
    /// The stream identifier.
    pub stream_id: StreamId,
    /// Highest contiguous sequence number.
    pub head_seq: u64,
    /// Receipt ID at head_seq.
    pub head_receipt_id: ReceiptId,
}

/// Request for receipts from a specific stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptRequest {
    /// The stream to fetch from.
    pub stream_id: StreamId,
    /// Which sequence numbers are needed.
    pub seqs: SeqRange,
}

/// Specification of which sequence numbers are needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeqRange {
    /// A single sequence number.
    Single(u64),
    /// A contiguous range (inclusive on both ends).
    Range {
        /// First seq in range.
        start: u64,
        /// Last seq in range.
        end: u64,
    },
    /// An explicit list of sequence numbers.
    List(Vec<u64>),
}

impl SeqRange {
    /// Expand to a list of sequence numbers.
    pub fn to_vec(&self) -> Vec<u64> {
        match self {
            SeqRange::Single(seq) => vec![*seq],
            SeqRange::Range { start, end } => (*start..=*end).collect(),
            SeqRange::List(seqs) => seqs.clone(),
        }
    }

    /// Count how many sequence numbers this represents.
    pub fn count(&self) -> usize {
        match self {
            SeqRange::Single(_) => 1,
            SeqRange::Range { start, end } => {
                if end >= start {
                    (end - start + 1) as usize
                } else {
                    0
                }
            }
            SeqRange::List(seqs) => seqs.len(),
        }
    }
}

/// Error codes for sync protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum SyncErrorCode {
    /// Unknown/unspecified error.
    Unknown = 0,
    /// Protocol version mismatch.
    VersionMismatch = 1,
    /// Message too large.
    MessageTooLarge = 2,
    /// Invalid message format.
    InvalidMessage = 3,
    /// Rate limited.
    RateLimited = 4,
    /// Stream not found / not authorized.
    StreamNotFound = 5,
    /// Internal error on peer.
    InternalError = 6,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seq_range_single() {
        let range = SeqRange::Single(5);
        assert_eq!(range.to_vec(), vec![5]);
        assert_eq!(range.count(), 1);
    }

    #[test]
    fn test_seq_range_range() {
        let range = SeqRange::Range { start: 3, end: 7 };
        assert_eq!(range.to_vec(), vec![3, 4, 5, 6, 7]);
        assert_eq!(range.count(), 5);
    }

    #[test]
    fn test_seq_range_list() {
        let range = SeqRange::List(vec![1, 5, 9]);
        assert_eq!(range.to_vec(), vec![1, 5, 9]);
        assert_eq!(range.count(), 3);
    }

    #[test]
    fn test_message_limits_valid() {
        let msg = SyncMessage::Hello {
            node_id: NodeId([0u8; 32]),
            protocol_version: 0,
            streams_of_interest: vec![],
        };
        assert!(msg.validate_limits().is_ok());
    }

    #[test]
    fn test_message_limits_exceeded() {
        let msg = SyncMessage::Hello {
            node_id: NodeId([0u8; 32]),
            protocol_version: 0,
            streams_of_interest: vec![StreamId::ZERO; 200],
        };
        assert!(msg.validate_limits().is_err());
    }
}
