//! Convergence verification for sync protocol.
//!
//! After syncing, nodes can verify they have converged to the same state
//! by computing deterministic state hashes.

use chainge_kernel_core::{Blake3Hash, Receipt, ReceiptId, StreamId};
use chainge_kernel_store::Store;

use crate::error::Result;

/// Compute a deterministic state hash for a stream.
///
/// The state hash is computed by replaying all receipts in order
/// and hashing their IDs. This allows two nodes to verify they
/// have the same receipts without exchanging the full set.
///
/// Algorithm:
/// 1. Get all receipts in seq order
/// 2. Hash: H = Blake3(H_prev || receipt_id)
/// 3. Return final H
pub async fn compute_stream_state_hash<S: Store>(
    store: &S,
    stream_id: &StreamId,
) -> Result<Option<Blake3Hash>> {
    let state = match store.get_stream_state(stream_id).await? {
        Some(s) => s,
        None => return Ok(None),
    };

    if state.head_seq == 0 {
        return Ok(None);
    }

    // Get all receipts from 1 to head_seq
    let receipts = store.get_receipts_range(stream_id, 1, state.head_seq).await?;

    if receipts.is_empty() {
        return Ok(None);
    }

    // Compute rolling hash
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chainge-state-v0:");
    hasher.update(stream_id.as_bytes());

    for receipt in receipts {
        let receipt_id = receipt.compute_id();
        hasher.update(&receipt_id.0);
    }

    Ok(Some(Blake3Hash(*hasher.finalize().as_bytes())))
}

/// Verify two nodes have converged on a stream.
///
/// Returns true if both nodes have the same:
/// 1. Head sequence number
/// 2. Head receipt ID
/// 3. State hash (computed from all receipts)
pub async fn verify_convergence<S: Store>(
    local_store: &S,
    stream_id: &StreamId,
    remote_head_seq: u64,
    remote_head_receipt_id: &ReceiptId,
    remote_state_hash: Option<&Blake3Hash>,
) -> Result<ConvergenceResult> {
    let local_state = match local_store.get_stream_state(stream_id).await? {
        Some(s) => s,
        None => {
            return Ok(ConvergenceResult::NotConverged {
                reason: "stream not found locally".into(),
            });
        }
    };

    // Check head seq
    if local_state.head_seq != remote_head_seq {
        return Ok(ConvergenceResult::NotConverged {
            reason: format!(
                "head_seq mismatch: local={}, remote={}",
                local_state.head_seq, remote_head_seq
            ),
        });
    }

    // Check head receipt ID
    if let Some(local_head_id) = local_state.head_receipt_id {
        if local_head_id != *remote_head_receipt_id {
            return Ok(ConvergenceResult::Forked {
                at_seq: local_state.head_seq,
                local_receipt_id: local_head_id,
                remote_receipt_id: *remote_head_receipt_id,
            });
        }
    } else {
        return Ok(ConvergenceResult::NotConverged {
            reason: "local head_receipt_id is None".into(),
        });
    }

    // Check state hash if provided
    if let Some(remote_hash) = remote_state_hash {
        let local_hash = compute_stream_state_hash(local_store, stream_id).await?;
        if let Some(local_h) = local_hash {
            if &local_h != remote_hash {
                return Ok(ConvergenceResult::NotConverged {
                    reason: "state hash mismatch".into(),
                });
            }
        } else {
            return Ok(ConvergenceResult::NotConverged {
                reason: "could not compute local state hash".into(),
            });
        }
    }

    Ok(ConvergenceResult::Converged)
}

/// Result of convergence verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvergenceResult {
    /// Both nodes have identical state.
    Converged,
    /// Nodes have not yet converged (may need more sync rounds).
    NotConverged { reason: String },
    /// A fork was detected (equivocation by author).
    Forked {
        at_seq: u64,
        local_receipt_id: ReceiptId,
        remote_receipt_id: ReceiptId,
    },
}

impl ConvergenceResult {
    /// Check if nodes have converged.
    pub fn is_converged(&self) -> bool {
        matches!(self, ConvergenceResult::Converged)
    }

    /// Check if a fork was detected.
    pub fn is_forked(&self) -> bool {
        matches!(self, ConvergenceResult::Forked { .. })
    }
}

/// Batch verification of multiple streams.
pub async fn verify_all_streams<S: Store>(
    local_store: &S,
    remote_heads: &[(StreamId, u64, ReceiptId)],
) -> Result<Vec<(StreamId, ConvergenceResult)>> {
    let mut results = Vec::with_capacity(remote_heads.len());

    for (stream_id, remote_seq, remote_id) in remote_heads {
        let result = verify_convergence(local_store, stream_id, *remote_seq, remote_id, None).await?;
        results.push((*stream_id, result));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chainge_kernel_core::{canonical_bytes, Keypair, ReceiptBuilder, ReceiptKind};
    use chainge_kernel_store::MemoryStore;

    async fn setup_test_stream(store: &MemoryStore, keypair: &Keypair, count: u64) -> Vec<ReceiptId> {
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let mut ids = Vec::new();
        let mut prev_id: Option<ReceiptId> = None;

        for seq in 1..=count {
            let mut builder = ReceiptBuilder::new(keypair.public_key(), stream_id, seq)
                .timestamp(1000000 + seq as i64)
                .payload(format!("payload {}", seq).into_bytes());

            if seq == 1 {
                builder = builder.kind(ReceiptKind::StreamInit);
            } else {
                builder = builder.kind(ReceiptKind::Data).prev(prev_id.unwrap());
            }

            let receipt = builder.sign(keypair);
            let canonical = canonical_bytes(&receipt);
            let receipt_id = receipt.compute_id();

            store.insert_receipt(&receipt, &canonical).await.unwrap();
            ids.push(receipt_id);
            prev_id = Some(receipt_id);
        }

        // Update stream state
        let mut state = chainge_kernel_core::StreamState::new(
            keypair.public_key(),
            "test".to_string(),
            1000000,
        );
        state.head_seq = count;
        state.head_receipt_id = prev_id;
        store.upsert_stream_state(&state).await.unwrap();

        ids
    }

    #[tokio::test]
    async fn test_state_hash_deterministic() {
        let store = MemoryStore::new();
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        setup_test_stream(&store, &keypair, 5).await;

        let hash1 = compute_stream_state_hash(&store, &stream_id).await.unwrap();
        let hash2 = compute_stream_state_hash(&store, &stream_id).await.unwrap();

        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_convergence_verified() {
        let store = MemoryStore::new();
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let ids = setup_test_stream(&store, &keypair, 3).await;

        let result = verify_convergence(
            &store,
            &stream_id,
            3,
            ids.last().unwrap(),
            None,
        )
        .await
        .unwrap();

        assert!(result.is_converged());
    }

    #[tokio::test]
    async fn test_convergence_head_mismatch() {
        let store = MemoryStore::new();
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let ids = setup_test_stream(&store, &keypair, 3).await;

        // Remote claims to have more
        let result = verify_convergence(
            &store,
            &stream_id,
            5,
            ids.last().unwrap(),
            None,
        )
        .await
        .unwrap();

        assert!(!result.is_converged());
    }
}
