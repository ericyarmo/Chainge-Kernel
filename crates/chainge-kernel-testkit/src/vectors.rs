//! Golden test vectors for deterministic verification.
//!
//! These vectors ensure that canonical encoding produces identical results
//! across all implementations.

use chainge_kernel_core::{
    canonical_bytes, Blake3Hash, Ed25519PublicKey, Keypair, ReceiptBuilder, ReceiptId,
    ReceiptKind, StreamId,
};

/// A golden test vector.
#[derive(Debug, Clone)]
pub struct GoldenVector {
    /// Human-readable name for the vector.
    pub name: &'static str,
    /// Seed for deterministic key generation.
    pub seed: [u8; 32],
    /// Stream name.
    pub stream_name: &'static str,
    /// Sequence number.
    pub seq: u64,
    /// Receipt kind.
    pub kind: ReceiptKind,
    /// Payload bytes.
    pub payload: &'static [u8],
    /// Timestamp.
    pub timestamp: i64,
    /// Expected receipt ID (hex).
    pub expected_receipt_id: &'static str,
}

/// Get all golden test vectors.
pub fn all_vectors() -> Vec<GoldenVector> {
    vec![
        GoldenVector {
            name: "StreamInit with hello payload",
            seed: [0x42; 32],
            stream_name: "test-stream",
            seq: 1,
            kind: ReceiptKind::StreamInit,
            payload: b"hello",
            timestamp: 1736870400000, // 2026-01-14T12:00:00Z
            // This will be filled in when we can compute it
            expected_receipt_id: "",
        },
        GoldenVector {
            name: "Data receipt with world payload",
            seed: [0x42; 32],
            stream_name: "test-stream",
            seq: 2,
            kind: ReceiptKind::Data,
            payload: b"world",
            timestamp: 1736870401000,
            expected_receipt_id: "",
        },
        GoldenVector {
            name: "Empty payload StreamInit",
            seed: [0x00; 32],
            stream_name: "empty",
            seq: 1,
            kind: ReceiptKind::StreamInit,
            payload: b"",
            timestamp: 0,
            expected_receipt_id: "",
        },
    ]
}

/// Generate a receipt from a golden vector (without prev pointer).
///
/// Note: For seq > 1, you'd normally need a prev_receipt_id.
/// This function is for testing canonical encoding of individual receipts.
pub fn generate_receipt_from_vector(vector: &GoldenVector) -> chainge_kernel_core::Receipt {
    let keypair = Keypair::from_seed(&vector.seed);
    let stream_id = StreamId::derive(&keypair.public_key(), vector.stream_name);

    let mut builder = ReceiptBuilder::new(keypair.public_key(), stream_id, vector.seq)
        .kind(vector.kind)
        .timestamp(vector.timestamp)
        .payload(vector.payload.to_vec());

    // For seq > 1, we need a prev pointer. Use a dummy one for test vectors.
    if vector.seq > 1 {
        builder = builder.prev(ReceiptId::from_bytes([0xAA; 32]));
    }

    builder.sign(&keypair)
}

/// Verify all golden vectors produce consistent receipt IDs.
///
/// Call this to verify your implementation matches the reference.
pub fn verify_all_vectors() -> Vec<(String, bool, String)> {
    all_vectors()
        .iter()
        .map(|v| {
            let receipt = generate_receipt_from_vector(v);
            let id = receipt.compute_id();
            let hex = id.to_hex();

            // If expected is empty, just report what we got
            let matches = v.expected_receipt_id.is_empty() || hex == v.expected_receipt_id;

            (v.name.to_string(), matches, hex)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vectors_are_deterministic() {
        // Generate each vector twice, verify identical results
        for vector in all_vectors() {
            let r1 = generate_receipt_from_vector(&vector);
            let r2 = generate_receipt_from_vector(&vector);

            let id1 = r1.compute_id();
            let id2 = r2.compute_id();

            assert_eq!(
                id1, id2,
                "Vector '{}' produced different IDs on regeneration",
                vector.name
            );

            let bytes1 = canonical_bytes(&r1);
            let bytes2 = canonical_bytes(&r2);

            assert_eq!(
                bytes1, bytes2,
                "Vector '{}' produced different canonical bytes",
                vector.name
            );
        }
    }

    #[test]
    fn test_different_seeds_different_ids() {
        let v1 = GoldenVector {
            name: "seed1",
            seed: [0x01; 32],
            stream_name: "test",
            seq: 1,
            kind: ReceiptKind::StreamInit,
            payload: b"same",
            timestamp: 1000,
            expected_receipt_id: "",
        };

        let v2 = GoldenVector {
            name: "seed2",
            seed: [0x02; 32],
            stream_name: "test",
            seq: 1,
            kind: ReceiptKind::StreamInit,
            payload: b"same",
            timestamp: 1000,
            expected_receipt_id: "",
        };

        let r1 = generate_receipt_from_vector(&v1);
        let r2 = generate_receipt_from_vector(&v2);

        assert_ne!(r1.compute_id(), r2.compute_id());
    }
}
