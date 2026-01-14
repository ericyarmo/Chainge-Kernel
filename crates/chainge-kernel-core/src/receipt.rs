//! Receipt: the atomic unit of verifiable memory.
//!
//! A receipt is an immutable, signed event. Once created, it cannot be edited.
//! Changes are represented as new receipts.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::canonical::canonical_bytes;
use crate::crypto::{Blake3Hash, Ed25519PublicKey, Ed25519Signature, Keypair};
use crate::stream::StreamId;
use crate::types::ReceiptId;

/// The current receipt schema version.
pub const RECEIPT_VERSION: u8 = 0;

/// Maximum number of refs allowed in a receipt.
pub const MAX_REFS: usize = 16;

/// The kind of receipt, determining how the payload is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum ReceiptKind {
    // Core kinds (0x0000 - 0x00FF)
    /// Generic application data.
    Data = 0x0001,
    /// Marks a previous receipt as superseded.
    Tombstone = 0x0002,
    /// First receipt in a stream (seq=1).
    StreamInit = 0x0003,

    // Permission kinds (0x0100 - 0x01FF)
    /// Permission grant.
    Grant = 0x0100,
    /// Permission revocation.
    Revoke = 0x0101,
    /// Encrypted key material for recipient.
    KeyShare = 0x0102,

    // Sync kinds (0x0200 - 0x02FF)
    /// Checkpoint for sync protocol.
    Anchor = 0x0200,
}

impl ReceiptKind {
    /// Convert to u16 for serialization.
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// Try to parse from u16.
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0001 => Some(Self::Data),
            0x0002 => Some(Self::Tombstone),
            0x0003 => Some(Self::StreamInit),
            0x0100 => Some(Self::Grant),
            0x0101 => Some(Self::Revoke),
            0x0102 => Some(Self::KeyShare),
            0x0200 => Some(Self::Anchor),
            _ => None,
        }
    }

    /// Check if this is a core kind.
    pub fn is_core(self) -> bool {
        (self.to_u16() & 0xFF00) == 0x0000
    }

    /// Check if this is a permission kind.
    pub fn is_permission(self) -> bool {
        (self.to_u16() & 0xFF00) == 0x0100
    }

    /// Check if this is a sync kind.
    pub fn is_sync(self) -> bool {
        (self.to_u16() & 0xFF00) == 0x0200
    }
}

/// The header of a receipt, containing all metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptHeader {
    /// Schema version (currently 0).
    pub version: u8,

    /// The author's public key (32 bytes).
    pub author: Ed25519PublicKey,

    /// The stream this receipt belongs to (32 bytes).
    pub stream_id: StreamId,

    /// Sequence number within the stream (1-indexed).
    pub seq: u64,

    /// Author-claimed timestamp (Unix milliseconds). Untrusted.
    pub timestamp: i64,

    /// The kind of receipt.
    pub kind: ReceiptKind,

    /// Hash of the previous receipt in the stream (None if seq=1).
    pub prev_receipt_id: Option<ReceiptId>,

    /// References to other receipts (max 16).
    pub refs: Vec<ReceiptId>,

    /// Blake3 hash of the payload bytes.
    pub payload_hash: Blake3Hash,
}

impl ReceiptHeader {
    /// Create a new header for the first receipt in a stream.
    pub fn new_stream_init(
        author: Ed25519PublicKey,
        stream_id: StreamId,
        timestamp: i64,
        payload_hash: Blake3Hash,
    ) -> Self {
        Self {
            version: RECEIPT_VERSION,
            author,
            stream_id,
            seq: 1,
            timestamp,
            kind: ReceiptKind::StreamInit,
            prev_receipt_id: None,
            refs: Vec::new(),
            payload_hash,
        }
    }

    /// Create a new header for a data receipt.
    pub fn new_data(
        author: Ed25519PublicKey,
        stream_id: StreamId,
        seq: u64,
        timestamp: i64,
        prev_receipt_id: ReceiptId,
        payload_hash: Blake3Hash,
    ) -> Self {
        Self {
            version: RECEIPT_VERSION,
            author,
            stream_id,
            seq,
            timestamp,
            kind: ReceiptKind::Data,
            prev_receipt_id: Some(prev_receipt_id),
            refs: Vec::new(),
            payload_hash,
        }
    }

    /// Create a new tombstone header.
    pub fn new_tombstone(
        author: Ed25519PublicKey,
        stream_id: StreamId,
        seq: u64,
        timestamp: i64,
        prev_receipt_id: ReceiptId,
        tombstoned_receipt: ReceiptId,
    ) -> Self {
        Self {
            version: RECEIPT_VERSION,
            author,
            stream_id,
            seq,
            timestamp,
            kind: ReceiptKind::Tombstone,
            prev_receipt_id: Some(prev_receipt_id),
            refs: vec![tombstoned_receipt],
            payload_hash: Blake3Hash::hash(&[]), // Tombstones have empty payload
        }
    }
}

/// A complete receipt: header + payload + signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The receipt header.
    pub header: ReceiptHeader,

    /// The payload bytes (may be encrypted).
    pub payload: Bytes,

    /// Ed25519 signature over (canonical_header || payload).
    pub signature: Ed25519Signature,
}

impl Receipt {
    /// Compute the receipt ID (Blake3 hash of canonical bytes).
    pub fn compute_id(&self) -> ReceiptId {
        let bytes = canonical_bytes(self);
        ReceiptId(Blake3Hash::hash(&bytes).0)
    }

    /// Get the author's public key.
    pub fn author(&self) -> &Ed25519PublicKey {
        &self.header.author
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> &StreamId {
        &self.header.stream_id
    }

    /// Get the sequence number.
    pub fn seq(&self) -> u64 {
        self.header.seq
    }

    /// Get the receipt kind.
    pub fn kind(&self) -> ReceiptKind {
        self.header.kind
    }

    /// Check if this is the first receipt in a stream.
    pub fn is_stream_init(&self) -> bool {
        self.header.kind == ReceiptKind::StreamInit && self.header.seq == 1
    }

    /// Check if this is a tombstone.
    pub fn is_tombstone(&self) -> bool {
        self.header.kind == ReceiptKind::Tombstone
    }

    /// Get the tombstoned receipt ID (if this is a tombstone).
    pub fn tombstoned_receipt(&self) -> Option<&ReceiptId> {
        if self.is_tombstone() {
            self.header.refs.first()
        } else {
            None
        }
    }
}

/// Builder for creating receipts.
pub struct ReceiptBuilder {
    author: Ed25519PublicKey,
    stream_id: StreamId,
    seq: u64,
    timestamp: i64,
    kind: ReceiptKind,
    prev_receipt_id: Option<ReceiptId>,
    refs: Vec<ReceiptId>,
    payload: Bytes,
}

impl ReceiptBuilder {
    /// Start building a receipt.
    pub fn new(author: Ed25519PublicKey, stream_id: StreamId, seq: u64) -> Self {
        Self {
            author,
            stream_id,
            seq,
            timestamp: 0,
            kind: ReceiptKind::Data,
            prev_receipt_id: None,
            refs: Vec::new(),
            payload: Bytes::new(),
        }
    }

    /// Set the timestamp.
    pub fn timestamp(mut self, ts: i64) -> Self {
        self.timestamp = ts;
        self
    }

    /// Set the kind.
    pub fn kind(mut self, kind: ReceiptKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set the previous receipt ID.
    pub fn prev(mut self, prev: ReceiptId) -> Self {
        self.prev_receipt_id = Some(prev);
        self
    }

    /// Add a reference.
    pub fn add_ref(mut self, r: ReceiptId) -> Self {
        self.refs.push(r);
        self
    }

    /// Set the payload.
    pub fn payload(mut self, p: impl Into<Bytes>) -> Self {
        self.payload = p.into();
        self
    }

    /// Build and sign the receipt.
    pub fn sign(self, keypair: &Keypair) -> Receipt {
        let payload_hash = Blake3Hash::hash(&self.payload);

        let header = ReceiptHeader {
            version: RECEIPT_VERSION,
            author: self.author,
            stream_id: self.stream_id,
            seq: self.seq,
            timestamp: self.timestamp,
            kind: self.kind,
            prev_receipt_id: self.prev_receipt_id,
            refs: self.refs,
            payload_hash,
        };

        // Sign: canonical_header || payload
        let header_bytes = crate::canonical::canonical_header_bytes(&header);
        let mut message = header_bytes;
        message.extend_from_slice(&self.payload);
        let signature = keypair.sign(&message);

        Receipt {
            header,
            payload: self.payload,
            signature,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receipt_kind_roundtrip() {
        for kind in [
            ReceiptKind::Data,
            ReceiptKind::Tombstone,
            ReceiptKind::StreamInit,
            ReceiptKind::Grant,
            ReceiptKind::Revoke,
            ReceiptKind::KeyShare,
            ReceiptKind::Anchor,
        ] {
            let value = kind.to_u16();
            let recovered = ReceiptKind::from_u16(value).unwrap();
            assert_eq!(kind, recovered);
        }
    }

    #[test]
    fn test_receipt_kind_categories() {
        assert!(ReceiptKind::Data.is_core());
        assert!(ReceiptKind::Tombstone.is_core());
        assert!(ReceiptKind::StreamInit.is_core());

        assert!(ReceiptKind::Grant.is_permission());
        assert!(ReceiptKind::Revoke.is_permission());
        assert!(ReceiptKind::KeyShare.is_permission());

        assert!(ReceiptKind::Anchor.is_sync());
    }

    #[test]
    fn test_receipt_builder() {
        let keypair = Keypair::generate();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1234567890000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        assert_eq!(receipt.seq(), 1);
        assert_eq!(receipt.kind(), ReceiptKind::StreamInit);
        assert_eq!(receipt.payload.as_ref(), b"hello");
        assert!(receipt.is_stream_init());
    }

    #[test]
    fn test_receipt_id_deterministic() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1234567890000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let id1 = receipt.compute_id();
        let id2 = receipt.compute_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_tombstone_receipt() {
        let keypair = Keypair::generate();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let tombstoned = ReceiptId::from_bytes([0xab; 32]);

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1234567890000)
            .kind(ReceiptKind::Tombstone)
            .prev(ReceiptId::from_bytes([0x11; 32]))
            .add_ref(tombstoned)
            .payload(b"".to_vec())
            .sign(&keypair);

        assert!(receipt.is_tombstone());
        assert_eq!(receipt.tombstoned_receipt(), Some(&tombstoned));
    }
}
