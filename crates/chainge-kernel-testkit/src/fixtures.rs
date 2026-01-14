//! Test fixtures and helpers.
//!
//! Common setup code for integration tests.

use chainge_kernel_core::{Keypair, ReceiptBuilder, ReceiptId, ReceiptKind, StreamId};
use chainge_kernel_store::MemoryStore;

/// A test fixture with a keypair and memory store.
pub struct TestFixture {
    pub keypair: Keypair,
    pub store: MemoryStore,
}

impl TestFixture {
    /// Create a new test fixture with a random keypair.
    pub fn new() -> Self {
        Self {
            keypair: Keypair::generate(),
            store: MemoryStore::new(),
        }
    }

    /// Create with a deterministic keypair from seed.
    pub fn with_seed(seed: [u8; 32]) -> Self {
        Self {
            keypair: Keypair::from_seed(&seed),
            store: MemoryStore::new(),
        }
    }

    /// Get the keypair's public key.
    pub fn public_key(&self) -> chainge_kernel_core::Ed25519PublicKey {
        self.keypair.public_key()
    }

    /// Derive a stream ID for a given name.
    pub fn stream_id(&self, name: &str) -> StreamId {
        StreamId::derive(&self.keypair.public_key(), name)
    }

    /// Create a StreamInit receipt.
    pub fn make_stream_init(
        &self,
        stream_name: &str,
        payload: &[u8],
    ) -> chainge_kernel_core::Receipt {
        let stream_id = self.stream_id(stream_name);
        ReceiptBuilder::new(self.keypair.public_key(), stream_id, 1)
            .kind(ReceiptKind::StreamInit)
            .timestamp(now_millis())
            .payload(payload.to_vec())
            .sign(&self.keypair)
    }

    /// Create a Data receipt.
    pub fn make_data(
        &self,
        stream_name: &str,
        seq: u64,
        prev: ReceiptId,
        payload: &[u8],
    ) -> chainge_kernel_core::Receipt {
        let stream_id = self.stream_id(stream_name);
        ReceiptBuilder::new(self.keypair.public_key(), stream_id, seq)
            .kind(ReceiptKind::Data)
            .timestamp(now_millis())
            .prev(prev)
            .payload(payload.to_vec())
            .sign(&self.keypair)
    }

    /// Create a Tombstone receipt.
    pub fn make_tombstone(
        &self,
        stream_name: &str,
        seq: u64,
        prev: ReceiptId,
        target: ReceiptId,
    ) -> chainge_kernel_core::Receipt {
        let stream_id = self.stream_id(stream_name);
        ReceiptBuilder::new(self.keypair.public_key(), stream_id, seq)
            .kind(ReceiptKind::Tombstone)
            .timestamp(now_millis())
            .prev(prev)
            .add_ref(target)
            .payload(vec![])
            .sign(&self.keypair)
    }
}

impl Default for TestFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// Create multiple test fixtures for multi-party tests.
pub fn multi_party_fixtures(count: usize) -> Vec<TestFixture> {
    (0..count)
        .map(|i| {
            let mut seed = [0u8; 32];
            seed[0] = i as u8;
            TestFixture::with_seed(seed)
        })
        .collect()
}

/// Get current time in milliseconds.
fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chainge_kernel_core::canonical_bytes;
    use chainge_kernel_store::Store;

    #[tokio::test]
    async fn test_fixture_stream_creation() {
        let fixture = TestFixture::new();
        let receipt = fixture.make_stream_init("test", b"hello");

        assert_eq!(receipt.seq(), 1);
        assert_eq!(receipt.kind(), ReceiptKind::StreamInit);
        assert!(receipt.is_stream_init());
    }

    #[tokio::test]
    async fn test_fixture_chain() {
        let fixture = TestFixture::new();

        let r1 = fixture.make_stream_init("test", b"init");
        let id1 = r1.compute_id();

        let r2 = fixture.make_data("test", 2, id1, b"data1");
        let id2 = r2.compute_id();

        let r3 = fixture.make_data("test", 3, id2, b"data2");

        // Verify chain
        assert_eq!(r2.header.prev_receipt_id, Some(id1));
        assert_eq!(r3.header.prev_receipt_id, Some(id2));
    }

    #[tokio::test]
    async fn test_multi_party() {
        let parties = multi_party_fixtures(3);

        // Each party has unique keys
        let pks: Vec<_> = parties.iter().map(|p| p.public_key()).collect();
        assert_ne!(pks[0], pks[1]);
        assert_ne!(pks[1], pks[2]);
        assert_ne!(pks[0], pks[2]);
    }
}
