//! Stream: an ordered, append-only log of receipts.
//!
//! A stream is owned by a single author and identified by (author, stream_name).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

use crate::crypto::{Blake3Hash, Ed25519PublicKey};
use crate::types::ReceiptId;

/// A 32-byte stream identifier.
///
/// Derived from Blake3(author || stream_name).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(pub [u8; 32]);

impl StreamId {
    /// Derive a stream ID from author and stream name.
    pub fn derive(author: &Ed25519PublicKey, stream_name: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"chainge-stream-v0:");
        hasher.update(&author.0);
        hasher.update(b":");
        hasher.update(stream_name.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// The zero stream ID (sentinel).
    pub const ZERO: Self = Self([0u8; 32]);
}

impl fmt::Debug for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StreamId({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for StreamId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for StreamId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// The health status of a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamHealth {
    /// Stream is healthy with contiguous receipts.
    Healthy,

    /// Stream has gaps (missing sequence numbers).
    HasGaps {
        /// The missing sequence numbers.
        missing: Vec<u64>,
    },

    /// Stream has forked (multiple receipts at same seq).
    Forked {
        /// The sequence number where fork was detected.
        at_seq: u64,
        /// The conflicting receipt IDs.
        receipts: Vec<ReceiptId>,
    },
}

impl StreamHealth {
    /// Check if the stream is healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(self, StreamHealth::Healthy)
    }

    /// Check if the stream has gaps.
    pub fn has_gaps(&self) -> bool {
        matches!(self, StreamHealth::HasGaps { .. })
    }

    /// Check if the stream is forked.
    pub fn is_forked(&self) -> bool {
        matches!(self, StreamHealth::Forked { .. })
    }
}

/// State of a stream, tracking head position and gaps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamState {
    /// The stream identifier.
    pub stream_id: StreamId,

    /// The author's public key.
    pub author: Ed25519PublicKey,

    /// The stream name (used to derive stream_id).
    pub stream_name: String,

    /// Highest contiguous sequence number we have.
    pub head_seq: u64,

    /// Receipt ID at head_seq.
    pub head_receipt_id: Option<ReceiptId>,

    /// Highest seq we've heard about (may have gaps before it).
    pub known_max_seq: u64,

    /// Missing sequence numbers.
    pub gaps: BTreeSet<u64>,

    /// Deterministic hash of replayed state.
    pub state_hash: Option<Blake3Hash>,

    /// Health status.
    pub health: StreamHealth,

    /// When this stream state was created (local time).
    pub created_at: i64,

    /// When this stream state was last updated (local time).
    pub updated_at: i64,
}

impl StreamState {
    /// Create a new stream state.
    pub fn new(author: Ed25519PublicKey, stream_name: String, now: i64) -> Self {
        let stream_id = StreamId::derive(&author, &stream_name);
        Self {
            stream_id,
            author,
            stream_name,
            head_seq: 0,
            head_receipt_id: None,
            known_max_seq: 0,
            gaps: BTreeSet::new(),
            state_hash: None,
            health: StreamHealth::Healthy,
            created_at: now,
            updated_at: now,
        }
    }

    /// Record a new receipt at the given sequence number.
    ///
    /// Returns whether the receipt was accepted (not a conflict).
    pub fn record_receipt(
        &mut self,
        seq: u64,
        receipt_id: ReceiptId,
        now: i64,
    ) -> RecordResult {
        self.updated_at = now;

        // Update known_max_seq
        if seq > self.known_max_seq {
            self.known_max_seq = seq;
        }

        // Case 1: Extends head contiguously
        if seq == self.head_seq + 1 {
            self.head_seq = seq;
            self.head_receipt_id = Some(receipt_id);
            // Remove from gaps in case this seq was previously marked as a gap
            self.gaps.remove(&seq);

            // Check if we can advance further with previously received receipts
            // (This would need to check storage - handled by caller)

            // Update health
            self.update_health();
            return RecordResult::Accepted;
        }

        // Case 2: Fills a known gap
        if self.gaps.remove(&seq) {
            self.update_health();
            return RecordResult::GapFilled;
        }

        // Case 3: Creates a gap (seq beyond head and not filling known gap)
        if seq > self.head_seq + 1 {
            // Add all missing seqs between head and this new seq to gaps
            for missing in (self.head_seq + 1)..seq {
                self.gaps.insert(missing);
            }
            self.update_health();
            return RecordResult::AcceptedWithGaps;
        }

        // Case 4: seq <= head_seq and not in gaps - duplicate
        RecordResult::Duplicate
    }

    /// Attempt to advance the head by consuming filled gaps.
    ///
    /// Call this after filling gaps to see if head can advance.
    /// Returns the new head_seq if it advanced.
    pub fn try_advance_head(&mut self, get_receipt_at: impl Fn(u64) -> Option<ReceiptId>) -> Option<u64> {
        let original_head = self.head_seq;

        while self.head_seq < self.known_max_seq {
            let next_seq = self.head_seq + 1;
            if let Some(receipt_id) = get_receipt_at(next_seq) {
                if !self.gaps.contains(&next_seq) {
                    self.head_seq = next_seq;
                    self.head_receipt_id = Some(receipt_id);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if self.head_seq > original_head {
            self.update_health();
            Some(self.head_seq)
        } else {
            None
        }
    }

    /// Mark the stream as forked.
    pub fn mark_forked(&mut self, at_seq: u64, receipts: Vec<ReceiptId>, now: i64) {
        self.health = StreamHealth::Forked { at_seq, receipts };
        self.updated_at = now;
    }

    /// Update health based on current gaps.
    fn update_health(&mut self) {
        // Don't overwrite fork status
        if self.is_forked() {
            return;
        }

        if self.gaps.is_empty() {
            self.health = StreamHealth::Healthy;
        } else {
            self.health = StreamHealth::HasGaps {
                missing: self.gaps.iter().copied().collect(),
            };
        }
    }

    /// Check if stream is healthy.
    pub fn is_healthy(&self) -> bool {
        self.health.is_healthy()
    }

    /// Check if stream is forked.
    pub fn is_forked(&self) -> bool {
        self.health.is_forked()
    }

    /// Get the list of missing sequence numbers.
    pub fn missing_seqs(&self) -> Vec<u64> {
        self.gaps.iter().copied().collect()
    }
}

/// Result of recording a receipt to stream state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordResult {
    /// Receipt was accepted and extends head.
    Accepted,
    /// Receipt was accepted but created/maintained gaps.
    AcceptedWithGaps,
    /// Receipt filled a gap.
    GapFilled,
    /// Receipt is a duplicate (already have this seq).
    Duplicate,
}

/// Event emitted during stream replay.
#[derive(Debug, Clone)]
pub struct ReplayEvent {
    /// The receipt being replayed.
    pub receipt_id: ReceiptId,

    /// Sequence number.
    pub seq: u64,

    /// Whether this receipt has been tombstoned.
    pub is_tombstoned: bool,

    /// The receipt ID of the tombstone (if tombstoned).
    pub tombstoned_by: Option<ReceiptId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    #[test]
    fn test_stream_id_derivation() {
        let keypair = Keypair::generate();
        let id1 = StreamId::derive(&keypair.public_key(), "test-stream");
        let id2 = StreamId::derive(&keypair.public_key(), "test-stream");
        assert_eq!(id1, id2);

        let id3 = StreamId::derive(&keypair.public_key(), "other-stream");
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_stream_id_different_authors() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();

        let id1 = StreamId::derive(&kp1.public_key(), "shared-name");
        let id2 = StreamId::derive(&kp2.public_key(), "shared-name");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_stream_state_contiguous_append() {
        let keypair = Keypair::generate();
        let mut state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);

        let r1 = ReceiptId::from_bytes([1; 32]);
        let r2 = ReceiptId::from_bytes([2; 32]);
        let r3 = ReceiptId::from_bytes([3; 32]);

        assert_eq!(state.record_receipt(1, r1, 1001), RecordResult::Accepted);
        assert_eq!(state.head_seq, 1);
        assert!(state.is_healthy());

        assert_eq!(state.record_receipt(2, r2, 1002), RecordResult::Accepted);
        assert_eq!(state.head_seq, 2);

        assert_eq!(state.record_receipt(3, r3, 1003), RecordResult::Accepted);
        assert_eq!(state.head_seq, 3);
    }

    #[test]
    fn test_stream_state_gap_detection() {
        let keypair = Keypair::generate();
        let mut state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);

        let r1 = ReceiptId::from_bytes([1; 32]);
        let r5 = ReceiptId::from_bytes([5; 32]);

        state.record_receipt(1, r1, 1001);
        assert_eq!(state.head_seq, 1);

        // Skip to 5, creating gaps at 2, 3, 4
        assert_eq!(
            state.record_receipt(5, r5, 1002),
            RecordResult::AcceptedWithGaps
        );
        assert_eq!(state.head_seq, 1); // Head doesn't advance
        assert_eq!(state.known_max_seq, 5);
        assert_eq!(state.missing_seqs(), vec![2, 3, 4]);
        assert!(state.health.has_gaps());
    }

    #[test]
    fn test_stream_state_gap_filling() {
        let keypair = Keypair::generate();
        let mut state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);

        state.record_receipt(1, ReceiptId::from_bytes([1; 32]), 1001);
        state.record_receipt(5, ReceiptId::from_bytes([5; 32]), 1002);

        // Fill gap at 3
        assert_eq!(
            state.record_receipt(3, ReceiptId::from_bytes([3; 32]), 1003),
            RecordResult::GapFilled
        );
        assert_eq!(state.missing_seqs(), vec![2, 4]);

        // Fill gap at 2
        state.record_receipt(2, ReceiptId::from_bytes([2; 32]), 1004);
        assert_eq!(state.missing_seqs(), vec![4]);

        // Fill gap at 4
        state.record_receipt(4, ReceiptId::from_bytes([4; 32]), 1005);
        assert!(state.missing_seqs().is_empty());
        assert!(state.is_healthy());
    }

    #[test]
    fn test_stream_state_duplicate() {
        let keypair = Keypair::generate();
        let mut state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);

        let r1 = ReceiptId::from_bytes([1; 32]);
        state.record_receipt(1, r1, 1001);

        // Same seq, same id = duplicate
        assert_eq!(state.record_receipt(1, r1, 1002), RecordResult::Duplicate);
    }

    #[test]
    fn test_stream_state_fork() {
        let keypair = Keypair::generate();
        let mut state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);

        let r1a = ReceiptId::from_bytes([1; 32]);
        let r1b = ReceiptId::from_bytes([2; 32]);

        state.mark_forked(1, vec![r1a, r1b], 1001);
        assert!(state.is_forked());

        match &state.health {
            StreamHealth::Forked { at_seq, receipts } => {
                assert_eq!(*at_seq, 1);
                assert_eq!(receipts.len(), 2);
            }
            _ => panic!("expected forked"),
        }
    }

    #[test]
    fn test_stream_id_hex_roundtrip() {
        let keypair = Keypair::generate();
        let id = StreamId::derive(&keypair.public_key(), "test");
        let hex = id.to_hex();
        let recovered = StreamId::from_hex(&hex).unwrap();
        assert_eq!(id, recovered);
    }
}
