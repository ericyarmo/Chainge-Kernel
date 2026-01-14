//! In-memory implementation of the Store trait.
//!
//! This is primarily for testing. It has the same semantics as SQLite
//! but keeps everything in memory with no persistence.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::RwLock;

use async_trait::async_trait;
use bytes::Bytes;

use chainge_kernel_core::{
    canonical_bytes, Ed25519PublicKey, Receipt, ReceiptId, StreamId, StreamState,
};

use crate::error::Result;
use crate::traits::{Fork, InsertResult, Store};

/// In-memory store implementation.
///
/// All data is lost when the store is dropped. Thread-safe via RwLock.
pub struct MemoryStore {
    inner: RwLock<MemoryStoreInner>,
}

struct MemoryStoreInner {
    /// Receipts indexed by ID.
    receipts: HashMap<ReceiptId, StoredReceipt>,

    /// Position index: (stream_id, seq) -> receipt_id.
    positions: HashMap<(StreamId, u64), ReceiptId>,

    /// Stream states.
    streams: HashMap<StreamId, StreamState>,

    /// Gap tracking.
    gaps: HashMap<StreamId, BTreeSet<u64>>,

    /// Gap request times.
    gap_requests: HashMap<(StreamId, u64), i64>,

    /// Fork evidence.
    forks: HashMap<StreamId, Vec<Fork>>,
}

struct StoredReceipt {
    receipt: Receipt,
    canonical: Vec<u8>,
}

impl MemoryStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(MemoryStoreInner {
                receipts: HashMap::new(),
                positions: HashMap::new(),
                streams: HashMap::new(),
                gaps: HashMap::new(),
                gap_requests: HashMap::new(),
                forks: HashMap::new(),
            }),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn insert_receipt(&self, receipt: &Receipt, canonical: &[u8]) -> Result<InsertResult> {
        let mut inner = self.inner.write().unwrap();

        let receipt_id = receipt.compute_id();
        let stream_id = *receipt.stream_id();
        let seq = receipt.seq();

        // Check if exact receipt already exists
        if inner.receipts.contains_key(&receipt_id) {
            return Ok(InsertResult::AlreadyExists);
        }

        // Check for conflict at position
        if let Some(&existing_id) = inner.positions.get(&(stream_id, seq)) {
            return Ok(InsertResult::Conflict {
                existing: existing_id,
            });
        }

        // Insert
        inner.receipts.insert(
            receipt_id,
            StoredReceipt {
                receipt: receipt.clone(),
                canonical: canonical.to_vec(),
            },
        );
        inner.positions.insert((stream_id, seq), receipt_id);

        Ok(InsertResult::Inserted)
    }

    async fn get_receipt(&self, id: &ReceiptId) -> Result<Option<Receipt>> {
        let inner = self.inner.read().unwrap();
        Ok(inner.receipts.get(id).map(|sr| sr.receipt.clone()))
    }

    async fn get_receipt_by_position(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> Result<Option<Receipt>> {
        let inner = self.inner.read().unwrap();

        if let Some(&receipt_id) = inner.positions.get(&(*stream_id, seq)) {
            Ok(inner.receipts.get(&receipt_id).map(|sr| sr.receipt.clone()))
        } else {
            Ok(None)
        }
    }

    async fn get_receipts_range(
        &self,
        stream_id: &StreamId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Receipt>> {
        let inner = self.inner.read().unwrap();

        let mut receipts = Vec::new();
        for seq in start..=end {
            if let Some(&receipt_id) = inner.positions.get(&(*stream_id, seq)) {
                if let Some(sr) = inner.receipts.get(&receipt_id) {
                    receipts.push(sr.receipt.clone());
                }
            }
        }

        Ok(receipts)
    }

    async fn has_receipt(&self, id: &ReceiptId) -> Result<bool> {
        let inner = self.inner.read().unwrap();
        Ok(inner.receipts.contains_key(id))
    }

    async fn get_canonical_bytes(&self, id: &ReceiptId) -> Result<Option<Vec<u8>>> {
        let inner = self.inner.read().unwrap();
        Ok(inner.receipts.get(id).map(|sr| sr.canonical.clone()))
    }

    async fn get_stream_state(&self, stream_id: &StreamId) -> Result<Option<StreamState>> {
        let inner = self.inner.read().unwrap();
        Ok(inner.streams.get(stream_id).cloned())
    }

    async fn upsert_stream_state(&self, state: &StreamState) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.streams.insert(state.stream_id, state.clone());

        // Update gap tracking from state
        inner.gaps.insert(state.stream_id, state.gaps.clone());

        Ok(())
    }

    async fn list_streams(&self, author: Option<&Ed25519PublicKey>) -> Result<Vec<StreamId>> {
        let inner = self.inner.read().unwrap();

        let streams: Vec<StreamId> = if let Some(author) = author {
            inner
                .streams
                .values()
                .filter(|s| &s.author == author)
                .map(|s| s.stream_id)
                .collect()
        } else {
            inner.streams.keys().copied().collect()
        };

        Ok(streams)
    }

    async fn get_gaps(&self, stream_id: &StreamId) -> Result<Vec<u64>> {
        let inner = self.inner.read().unwrap();
        Ok(inner
            .gaps
            .get(stream_id)
            .map(|g| g.iter().copied().collect())
            .unwrap_or_default())
    }

    async fn add_gaps(&self, stream_id: &StreamId, seqs: &[u64]) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        let gaps = inner.gaps.entry(*stream_id).or_default();
        for &seq in seqs {
            gaps.insert(seq);
        }
        Ok(())
    }

    async fn remove_gap(&self, stream_id: &StreamId, seq: u64) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        if let Some(gaps) = inner.gaps.get_mut(stream_id) {
            gaps.remove(&seq);
        }
        Ok(())
    }

    async fn mark_gap_requested(&self, stream_id: &StreamId, seq: u64, at: i64) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.gap_requests.insert((*stream_id, seq), at);
        Ok(())
    }

    async fn record_fork(
        &self,
        stream_id: &StreamId,
        seq: u64,
        receipt_id: &ReceiptId,
    ) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        let forks = inner.forks.entry(*stream_id).or_default();

        // Check if this fork evidence already exists
        if !forks
            .iter()
            .any(|f| f.seq == seq && f.receipt_id == *receipt_id)
        {
            forks.push(Fork {
                stream_id: *stream_id,
                seq,
                receipt_id: *receipt_id,
                detected_at: now_millis(),
            });
        }

        Ok(())
    }

    async fn get_forks(&self, stream_id: &StreamId) -> Result<Vec<Fork>> {
        let inner = self.inner.read().unwrap();
        Ok(inner.forks.get(stream_id).cloned().unwrap_or_default())
    }

    async fn get_receipt_ids_since(
        &self,
        stream_id: &StreamId,
        after_seq: u64,
    ) -> Result<Vec<(u64, ReceiptId)>> {
        let inner = self.inner.read().unwrap();

        let mut pairs: Vec<(u64, ReceiptId)> = inner
            .positions
            .iter()
            .filter(|((sid, seq), _)| sid == stream_id && *seq > after_seq)
            .map(|((_, seq), rid)| (*seq, *rid))
            .collect();

        pairs.sort_by_key(|(seq, _)| *seq);
        Ok(pairs)
    }

    async fn get_all_stream_heads(&self) -> Result<Vec<(StreamId, u64, ReceiptId)>> {
        let inner = self.inner.read().unwrap();

        let heads: Vec<(StreamId, u64, ReceiptId)> = inner
            .streams
            .values()
            .filter_map(|s| {
                s.head_receipt_id
                    .map(|rid| (s.stream_id, s.head_seq, rid))
            })
            .collect();

        Ok(heads)
    }
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
    use chainge_kernel_core::{Keypair, ReceiptBuilder, ReceiptKind};

    fn make_test_receipt(keypair: &Keypair, seq: u64) -> Receipt {
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let mut builder = ReceiptBuilder::new(keypair.public_key(), stream_id, seq)
            .timestamp(1234567890000)
            .payload(format!("test payload {}", seq).into_bytes());

        if seq == 1 {
            builder = builder.kind(ReceiptKind::StreamInit);
        } else {
            builder = builder
                .kind(ReceiptKind::Data)
                .prev(ReceiptId::from_bytes([0xaa; 32]));
        }

        builder.sign(keypair)
    }

    #[tokio::test]
    async fn test_memory_store_basic() {
        let store = MemoryStore::new();
        let keypair = Keypair::generate();
        let receipt = make_test_receipt(&keypair, 1);
        let canonical = canonical_bytes(&receipt);
        let receipt_id = receipt.compute_id();

        // Insert
        let result = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(result, InsertResult::Inserted);

        // Get
        let retrieved = store.get_receipt(&receipt_id).await.unwrap().unwrap();
        assert_eq!(retrieved.seq(), 1);
    }

    #[tokio::test]
    async fn test_memory_store_idempotent() {
        let store = MemoryStore::new();
        let keypair = Keypair::generate();
        let receipt = make_test_receipt(&keypair, 1);
        let canonical = canonical_bytes(&receipt);

        let r1 = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(r1, InsertResult::Inserted);

        let r2 = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(r2, InsertResult::AlreadyExists);
    }
}
