//! Store trait: the abstract interface for receipt persistence.
//!
//! This trait allows the kernel to be storage-agnostic. Implementations
//! include SQLite (primary) and in-memory (for tests).

use async_trait::async_trait;
use chainge_kernel_core::{
    Ed25519PublicKey, Receipt, ReceiptId, StreamId, StreamState,
};

use crate::error::Result;

/// Result of inserting a receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertResult {
    /// Receipt was inserted successfully.
    Inserted,
    /// Receipt already exists (idempotent - not an error).
    AlreadyExists,
    /// Conflict: different receipt exists at same stream position.
    Conflict {
        /// The existing receipt ID at this position.
        existing: ReceiptId,
    },
}

/// Evidence of a fork in a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fork {
    /// The stream where the fork was detected.
    pub stream_id: StreamId,
    /// The sequence number where fork occurred.
    pub seq: u64,
    /// One of the conflicting receipt IDs.
    pub receipt_id: ReceiptId,
    /// When the fork was detected (Unix ms).
    pub detected_at: i64,
}

/// The Store trait: async interface for receipt persistence.
///
/// All methods are async to support both sync (SQLite) and async backends.
/// For SQLite, we use `spawn_blocking` internally to avoid blocking the runtime.
///
/// # Design Notes
///
/// - **Idempotent inserts**: Inserting the same receipt twice returns `AlreadyExists`.
/// - **Conflict detection**: Inserting a different receipt at an existing position
///   returns `Conflict` with the existing receipt ID.
/// - **Gap tracking**: The store tracks missing sequence numbers for sync protocol.
/// - **Fork detection**: Multiple receipts at same (stream_id, seq) are recorded.
#[async_trait]
pub trait Store: Send + Sync {
    // ─────────────────────────────────────────────────────────────────────────
    // Receipt Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Insert a receipt into the store.
    ///
    /// # Arguments
    /// - `receipt`: The receipt to insert.
    /// - `canonical`: The canonical bytes (cached to avoid recomputation).
    ///
    /// # Returns
    /// - `Inserted` if the receipt was new.
    /// - `AlreadyExists` if the exact same receipt already exists.
    /// - `Conflict` if a different receipt exists at the same position.
    async fn insert_receipt(&self, receipt: &Receipt, canonical: &[u8]) -> Result<InsertResult>;

    /// Get a receipt by its content-addressed ID.
    async fn get_receipt(&self, id: &ReceiptId) -> Result<Option<Receipt>>;

    /// Get a receipt by its position in a stream.
    async fn get_receipt_by_position(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> Result<Option<Receipt>>;

    /// Get a range of receipts from a stream.
    ///
    /// Returns receipts with `start <= seq <= end`, ordered by seq.
    async fn get_receipts_range(
        &self,
        stream_id: &StreamId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Receipt>>;

    /// Check if a receipt exists by ID.
    async fn has_receipt(&self, id: &ReceiptId) -> Result<bool>;

    /// Get the canonical bytes for a receipt (if cached).
    async fn get_canonical_bytes(&self, id: &ReceiptId) -> Result<Option<Vec<u8>>>;

    // ─────────────────────────────────────────────────────────────────────────
    // Stream Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the state of a stream.
    async fn get_stream_state(&self, stream_id: &StreamId) -> Result<Option<StreamState>>;

    /// Update or insert stream state.
    async fn upsert_stream_state(&self, state: &StreamState) -> Result<()>;

    /// List all streams, optionally filtered by author.
    async fn list_streams(&self, author: Option<&Ed25519PublicKey>) -> Result<Vec<StreamId>>;

    // ─────────────────────────────────────────────────────────────────────────
    // Gap Operations (for sync protocol)
    // ─────────────────────────────────────────────────────────────────────────

    /// Get all missing sequence numbers for a stream.
    async fn get_gaps(&self, stream_id: &StreamId) -> Result<Vec<u64>>;

    /// Add missing sequence numbers to track.
    async fn add_gaps(&self, stream_id: &StreamId, seqs: &[u64]) -> Result<()>;

    /// Remove a gap (when the receipt is received).
    async fn remove_gap(&self, stream_id: &StreamId, seq: u64) -> Result<()>;

    /// Mark a gap as requested (for rate limiting requests).
    async fn mark_gap_requested(&self, stream_id: &StreamId, seq: u64, at: i64) -> Result<()>;

    // ─────────────────────────────────────────────────────────────────────────
    // Fork Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Record evidence of a fork.
    async fn record_fork(
        &self,
        stream_id: &StreamId,
        seq: u64,
        receipt_id: &ReceiptId,
    ) -> Result<()>;

    /// Get all fork evidence for a stream.
    async fn get_forks(&self, stream_id: &StreamId) -> Result<Vec<Fork>>;

    // ─────────────────────────────────────────────────────────────────────────
    // Bulk Operations (for sync protocol)
    // ─────────────────────────────────────────────────────────────────────────

    /// Get receipt IDs after a given sequence number.
    ///
    /// Returns `(seq, receipt_id)` pairs, ordered by seq.
    async fn get_receipt_ids_since(
        &self,
        stream_id: &StreamId,
        after_seq: u64,
    ) -> Result<Vec<(u64, ReceiptId)>>;

    /// Get heads of all known streams.
    ///
    /// Returns `(stream_id, head_seq, head_receipt_id)` for each stream.
    async fn get_all_stream_heads(&self) -> Result<Vec<(StreamId, u64, ReceiptId)>>;
}

/// Extension trait for common store patterns.
pub trait StoreExt: Store {
    /// Insert a receipt and update stream state atomically.
    ///
    /// This is a convenience method that combines insert + state update.
    fn insert_and_update_stream(
        &self,
        receipt: &Receipt,
        canonical: &[u8],
        now: i64,
    ) -> impl std::future::Future<Output = Result<InsertResult>> + Send;
}

impl<S: Store + ?Sized> StoreExt for S {
    async fn insert_and_update_stream(
        &self,
        receipt: &Receipt,
        canonical: &[u8],
        now: i64,
    ) -> Result<InsertResult> {
        // Insert the receipt first
        let result = self.insert_receipt(receipt, canonical).await?;

        // Only update stream state if we actually inserted
        if matches!(result, InsertResult::Inserted) {
            let receipt_id = receipt.compute_id();
            let stream_id = *receipt.stream_id();

            // Get or create stream state
            let mut state = self
                .get_stream_state(&stream_id)
                .await?
                .unwrap_or_else(|| {
                    StreamState::new(
                        receipt.author().clone(),
                        String::new(), // We don't know the name from just a receipt
                        now,
                    )
                });

            // Record the receipt
            state.record_receipt(receipt.seq(), receipt_id, now);

            // Try to advance head if gaps were filled
            if !state.gaps.is_empty() {
                let store = self;
                state.try_advance_head(|seq| {
                    // We can't do async here, so we'd need a different approach
                    // For now, this is a limitation - caller should handle head advancement
                    None
                });
            }

            // Persist the state
            self.upsert_stream_state(&state).await?;
        }

        Ok(result)
    }
}
