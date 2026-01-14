//! The Kernel: unified API for the Chainge system.
//!
//! The Kernel brings together storage, sync, and permissions into a
//! cohesive interface for building applications.

use std::sync::Arc;

use chainge_kernel_core::{
    canonical_bytes, validate_receipt, Blake3Hash, Ed25519PublicKey, Keypair, Receipt,
    ReceiptBuilder, ReceiptId, ReceiptKind, StreamId, StreamState,
};
use chainge_kernel_perms::{PermissionState, GrantPayload, RevokePayload};
use chainge_kernel_store::{InsertResult, Store, StoreError};
use chainge_kernel_sync::{SyncConfig, SyncReport, SyncSession, Transport, NodeId};

use crate::error::{KernelError, Result};

/// Configuration for the Kernel.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Whether to validate receipts on ingest.
    pub validate_on_ingest: bool,
    /// Sync configuration.
    pub sync: SyncConfig,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            validate_on_ingest: true,
            sync: SyncConfig::default(),
        }
    }
}

/// The main Kernel struct.
///
/// Provides a unified API for:
/// - Creating and managing streams
/// - Appending receipts
/// - Querying receipts
/// - Syncing with peers
/// - Managing permissions
pub struct Kernel<S: Store> {
    /// The identity keypair for this kernel instance.
    keypair: Keypair,
    /// The storage backend.
    store: Arc<S>,
    /// Configuration.
    config: KernelConfig,
    /// Permission state (computed from receipts).
    permissions: PermissionState,
}

impl<S: Store> Kernel<S> {
    /// Create a new kernel instance.
    pub fn new(keypair: Keypair, store: S, config: KernelConfig) -> Self {
        Self {
            keypair,
            store: Arc::new(store),
            config,
            permissions: PermissionState::new(),
        }
    }

    /// Get the kernel's public key.
    pub fn public_key(&self) -> Ed25519PublicKey {
        self.keypair.public_key()
    }

    /// Get the store reference.
    pub fn store(&self) -> &S {
        &self.store
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Stream Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a new stream.
    ///
    /// Creates the stream and appends a StreamInit receipt.
    pub async fn create_stream(&self, name: &str, payload: &[u8]) -> Result<(StreamId, ReceiptId)> {
        let stream_id = StreamId::derive(&self.keypair.public_key(), name);

        // Check if stream already exists
        if let Some(state) = self.store.get_stream_state(&stream_id).await? {
            if state.head_seq > 0 {
                return Err(KernelError::StreamExists(stream_id));
            }
        }

        // Create StreamInit receipt
        let receipt = ReceiptBuilder::new(self.keypair.public_key(), stream_id, 1)
            .kind(ReceiptKind::StreamInit)
            .timestamp(now_millis())
            .payload(payload.to_vec())
            .sign(&self.keypair);

        let receipt_id = self.ingest_local(&receipt).await?;
        Ok((stream_id, receipt_id))
    }

    /// Append a receipt to a stream.
    pub async fn append(
        &self,
        stream_id: &StreamId,
        kind: ReceiptKind,
        payload: &[u8],
    ) -> Result<ReceiptId> {
        // Get current stream state
        let state = self
            .store
            .get_stream_state(stream_id)
            .await?
            .ok_or_else(|| KernelError::StreamNotFound(*stream_id))?;

        let prev_receipt_id = state
            .head_receipt_id
            .ok_or_else(|| KernelError::StreamNotFound(*stream_id))?;

        let seq = state.head_seq + 1;

        // Build and sign receipt
        let receipt = ReceiptBuilder::new(self.keypair.public_key(), *stream_id, seq)
            .kind(kind)
            .timestamp(now_millis())
            .prev(prev_receipt_id)
            .payload(payload.to_vec())
            .sign(&self.keypair);

        self.ingest_local(&receipt).await
    }

    /// Append a tombstone receipt.
    pub async fn tombstone(
        &self,
        stream_id: &StreamId,
        target_receipt_id: ReceiptId,
    ) -> Result<ReceiptId> {
        let state = self
            .store
            .get_stream_state(stream_id)
            .await?
            .ok_or_else(|| KernelError::StreamNotFound(*stream_id))?;

        let prev_receipt_id = state
            .head_receipt_id
            .ok_or_else(|| KernelError::StreamNotFound(*stream_id))?;

        let seq = state.head_seq + 1;

        let receipt = ReceiptBuilder::new(self.keypair.public_key(), *stream_id, seq)
            .kind(ReceiptKind::Tombstone)
            .timestamp(now_millis())
            .prev(prev_receipt_id)
            .add_ref(target_receipt_id)
            .payload(vec![])
            .sign(&self.keypair);

        self.ingest_local(&receipt).await
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Query Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Get a receipt by ID.
    pub async fn get_receipt(&self, id: &ReceiptId) -> Result<Option<Receipt>> {
        Ok(self.store.get_receipt(id).await?)
    }

    /// Get a receipt by stream position.
    pub async fn get_receipt_at(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> Result<Option<Receipt>> {
        Ok(self.store.get_receipt_by_position(stream_id, seq).await?)
    }

    /// Get a range of receipts from a stream.
    pub async fn get_receipts(
        &self,
        stream_id: &StreamId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Receipt>> {
        Ok(self.store.get_receipts_range(stream_id, start, end).await?)
    }

    /// Get the state of a stream.
    pub async fn stream_state(&self, stream_id: &StreamId) -> Result<Option<StreamState>> {
        Ok(self.store.get_stream_state(stream_id).await?)
    }

    /// List all known streams.
    pub async fn list_streams(&self) -> Result<Vec<StreamId>> {
        Ok(self.store.list_streams(None).await?)
    }

    /// List streams owned by a specific author.
    pub async fn list_streams_by(&self, author: &Ed25519PublicKey) -> Result<Vec<StreamId>> {
        Ok(self.store.list_streams(Some(author)).await?)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Ingest Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Ingest a receipt from external source.
    ///
    /// Validates and stores the receipt if valid.
    pub async fn ingest(&self, receipt: &Receipt) -> Result<IngestResult> {
        // Validate if configured
        if self.config.validate_on_ingest {
            validate_receipt(receipt)?;
        }

        let canonical = canonical_bytes(receipt);
        let receipt_id = receipt.compute_id();

        match self.store.insert_receipt(receipt, &canonical).await? {
            InsertResult::Inserted => {
                self.update_stream_state(receipt).await?;
                Ok(IngestResult::Accepted(receipt_id))
            }
            InsertResult::AlreadyExists => Ok(IngestResult::Duplicate),
            InsertResult::Conflict { existing } => {
                // Record fork evidence
                self.store
                    .record_fork(receipt.stream_id(), receipt.seq(), &receipt_id)
                    .await?;
                Ok(IngestResult::Conflict { existing })
            }
        }
    }

    /// Ingest a locally created receipt (skip external validation).
    async fn ingest_local(&self, receipt: &Receipt) -> Result<ReceiptId> {
        let canonical = canonical_bytes(receipt);
        let receipt_id = receipt.compute_id();

        match self.store.insert_receipt(receipt, &canonical).await? {
            InsertResult::Inserted => {
                self.update_stream_state(receipt).await?;
                Ok(receipt_id)
            }
            InsertResult::AlreadyExists => Ok(receipt_id),
            InsertResult::Conflict { existing } => Err(KernelError::Conflict {
                stream_id: *receipt.stream_id(),
                seq: receipt.seq(),
                existing,
            }),
        }
    }

    /// Update stream state after inserting a receipt.
    async fn update_stream_state(&self, receipt: &Receipt) -> Result<()> {
        let stream_id = *receipt.stream_id();
        let receipt_id = receipt.compute_id();

        let mut state = self
            .store
            .get_stream_state(&stream_id)
            .await?
            .unwrap_or_else(|| {
                StreamState::new(receipt.author().clone(), String::new(), now_millis())
            });

        state.record_receipt(receipt.seq(), receipt_id, now_millis());

        // Try to advance head if we have contiguous receipts
        let store = self.store.clone();
        state.try_advance_head(|seq| {
            // We can't do async in this closure, but the state machine
            // still works - it will just need manual head advancement
            None
        });

        self.store.upsert_stream_state(&state).await?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Sync Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Sync with a peer via the given transport.
    pub async fn sync<T: Transport>(
        &self,
        transport: T,
        peer: &NodeId,
    ) -> Result<SyncReport> {
        let store = Arc::clone(&self.store);
        let mut session = SyncSession::new(
            StoreWrapper(store),
            transport,
            self.config.sync.clone(),
        );

        Ok(session.sync_with(peer).await?)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Permission Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Grant permission to a recipient.
    pub async fn grant(
        &self,
        perms_stream_id: &StreamId,
        payload: GrantPayload,
    ) -> Result<ReceiptId> {
        let payload_bytes = payload.to_bytes();
        self.append(perms_stream_id, ReceiptKind::Grant, &payload_bytes)
            .await
    }

    /// Revoke a previous grant.
    pub async fn revoke(
        &self,
        perms_stream_id: &StreamId,
        grant_receipt_id: ReceiptId,
        reason: Option<&str>,
    ) -> Result<ReceiptId> {
        let payload = RevokePayload {
            grant_receipt_id,
            reason: reason.map(String::from),
        };
        let payload_bytes = payload.to_bytes();
        self.append(perms_stream_id, ReceiptKind::Revoke, &payload_bytes)
            .await
    }

    /// Get the permission state.
    pub fn permissions(&self) -> &PermissionState {
        &self.permissions
    }

    /// Rebuild permission state from a stream.
    pub async fn rebuild_permissions(&mut self, stream_id: &StreamId) -> Result<()> {
        let state = self.store.get_stream_state(stream_id).await?;
        if let Some(s) = state {
            let receipts = self.store.get_receipts_range(stream_id, 1, s.head_seq).await?;
            for receipt in receipts {
                self.permissions.apply_receipt(&receipt)?;
            }
        }
        Ok(())
    }
}

/// Wrapper to make Arc<S> implement Store.
struct StoreWrapper<S: Store>(Arc<S>);

#[async_trait::async_trait]
impl<S: Store> Store for StoreWrapper<S> {
    async fn insert_receipt(
        &self,
        receipt: &Receipt,
        canonical: &[u8],
    ) -> std::result::Result<InsertResult, StoreError> {
        self.0.insert_receipt(receipt, canonical).await
    }

    async fn get_receipt(&self, id: &ReceiptId) -> std::result::Result<Option<Receipt>, StoreError> {
        self.0.get_receipt(id).await
    }

    async fn get_receipt_by_position(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> std::result::Result<Option<Receipt>, StoreError> {
        self.0.get_receipt_by_position(stream_id, seq).await
    }

    async fn get_receipts_range(
        &self,
        stream_id: &StreamId,
        start: u64,
        end: u64,
    ) -> std::result::Result<Vec<Receipt>, StoreError> {
        self.0.get_receipts_range(stream_id, start, end).await
    }

    async fn has_receipt(&self, id: &ReceiptId) -> std::result::Result<bool, StoreError> {
        self.0.has_receipt(id).await
    }

    async fn get_canonical_bytes(
        &self,
        id: &ReceiptId,
    ) -> std::result::Result<Option<Vec<u8>>, StoreError> {
        self.0.get_canonical_bytes(id).await
    }

    async fn get_stream_state(
        &self,
        stream_id: &StreamId,
    ) -> std::result::Result<Option<StreamState>, StoreError> {
        self.0.get_stream_state(stream_id).await
    }

    async fn upsert_stream_state(&self, state: &StreamState) -> std::result::Result<(), StoreError> {
        self.0.upsert_stream_state(state).await
    }

    async fn list_streams(
        &self,
        author: Option<&Ed25519PublicKey>,
    ) -> std::result::Result<Vec<StreamId>, StoreError> {
        self.0.list_streams(author).await
    }

    async fn get_gaps(&self, stream_id: &StreamId) -> std::result::Result<Vec<u64>, StoreError> {
        self.0.get_gaps(stream_id).await
    }

    async fn add_gaps(
        &self,
        stream_id: &StreamId,
        seqs: &[u64],
    ) -> std::result::Result<(), StoreError> {
        self.0.add_gaps(stream_id, seqs).await
    }

    async fn remove_gap(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> std::result::Result<(), StoreError> {
        self.0.remove_gap(stream_id, seq).await
    }

    async fn mark_gap_requested(
        &self,
        stream_id: &StreamId,
        seq: u64,
        at: i64,
    ) -> std::result::Result<(), StoreError> {
        self.0.mark_gap_requested(stream_id, seq, at).await
    }

    async fn record_fork(
        &self,
        stream_id: &StreamId,
        seq: u64,
        receipt_id: &ReceiptId,
    ) -> std::result::Result<(), StoreError> {
        self.0.record_fork(stream_id, seq, receipt_id).await
    }

    async fn get_forks(
        &self,
        stream_id: &StreamId,
    ) -> std::result::Result<Vec<chainge_kernel_store::Fork>, StoreError> {
        self.0.get_forks(stream_id).await
    }

    async fn get_receipt_ids_since(
        &self,
        stream_id: &StreamId,
        after_seq: u64,
    ) -> std::result::Result<Vec<(u64, ReceiptId)>, StoreError> {
        self.0.get_receipt_ids_since(stream_id, after_seq).await
    }

    async fn get_all_stream_heads(
        &self,
    ) -> std::result::Result<Vec<(StreamId, u64, ReceiptId)>, StoreError> {
        self.0.get_all_stream_heads().await
    }
}

/// Result of ingesting a receipt.
#[derive(Debug, Clone)]
pub enum IngestResult {
    /// Receipt was accepted and stored.
    Accepted(ReceiptId),
    /// Receipt was already in store (idempotent).
    Duplicate,
    /// Conflict with existing receipt at same position.
    Conflict { existing: ReceiptId },
}

/// Get current time in milliseconds.
fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis() as i64
}
