//! Receipt validation: signature verification and structural checks.

use crate::canonical::{canonical_header_bytes, signed_message};
use crate::crypto::Blake3Hash;
use crate::error::ValidationError;
use crate::receipt::{Receipt, ReceiptKind, MAX_REFS, RECEIPT_VERSION};

/// Validate a receipt's structure (without checking stream context).
///
/// This performs:
/// - Version check
/// - Payload hash verification
/// - Signature verification
/// - Structural rules (refs count, tombstone requirements)
pub fn validate_receipt(receipt: &Receipt) -> Result<(), ValidationError> {
    // 1. Check version
    if receipt.header.version != RECEIPT_VERSION {
        return Err(ValidationError::UnsupportedVersion(receipt.header.version));
    }

    // 2. Verify payload hash
    let computed_hash = Blake3Hash::hash(&receipt.payload);
    if computed_hash != receipt.header.payload_hash {
        return Err(ValidationError::PayloadHashMismatch);
    }

    // 3. Check refs count
    if receipt.header.refs.len() > MAX_REFS {
        return Err(ValidationError::TooManyRefs);
    }

    // 4. Tombstone must have a ref
    if receipt.header.kind == ReceiptKind::Tombstone && receipt.header.refs.is_empty() {
        return Err(ValidationError::TombstoneMissingRef);
    }

    // 5. StreamInit must have seq=1 and no prev
    if receipt.header.kind == ReceiptKind::StreamInit {
        if receipt.header.seq != 1 {
            return Err(ValidationError::InvalidSequence {
                expected: 1,
                got: receipt.header.seq,
            });
        }
        if receipt.header.prev_receipt_id.is_some() {
            return Err(ValidationError::InvalidPrevReceipt {
                expected: None,
                got: receipt.header.prev_receipt_id,
            });
        }
    }

    // 6. Non-init receipts (seq > 1) must have prev_receipt_id
    if receipt.header.seq > 1 && receipt.header.prev_receipt_id.is_none() {
        return Err(ValidationError::StructuralError(
            "seq > 1 requires prev_receipt_id".into(),
        ));
    }

    // 7. Verify signature
    let message = signed_message(receipt);
    receipt
        .header
        .author
        .verify(&message, &receipt.signature)
        .map_err(|_| ValidationError::SignatureFailed)?;

    Ok(())
}

/// Validate receipt structure without signature verification.
///
/// Useful for checking structure before signature verification,
/// or when the receipt is known to be valid (e.g., from trusted storage).
pub fn validate_receipt_structure(receipt: &Receipt) -> Result<(), ValidationError> {
    // Version check
    if receipt.header.version != RECEIPT_VERSION {
        return Err(ValidationError::UnsupportedVersion(receipt.header.version));
    }

    // Payload hash
    let computed_hash = Blake3Hash::hash(&receipt.payload);
    if computed_hash != receipt.header.payload_hash {
        return Err(ValidationError::PayloadHashMismatch);
    }

    // Refs count
    if receipt.header.refs.len() > MAX_REFS {
        return Err(ValidationError::TooManyRefs);
    }

    // Tombstone ref requirement
    if receipt.header.kind == ReceiptKind::Tombstone && receipt.header.refs.is_empty() {
        return Err(ValidationError::TombstoneMissingRef);
    }

    // StreamInit requirements
    if receipt.header.kind == ReceiptKind::StreamInit {
        if receipt.header.seq != 1 {
            return Err(ValidationError::InvalidSequence {
                expected: 1,
                got: receipt.header.seq,
            });
        }
        if receipt.header.prev_receipt_id.is_some() {
            return Err(ValidationError::InvalidPrevReceipt {
                expected: None,
                got: receipt.header.prev_receipt_id,
            });
        }
    }

    // seq > 1 requires prev
    if receipt.header.seq > 1 && receipt.header.prev_receipt_id.is_none() {
        return Err(ValidationError::StructuralError(
            "seq > 1 requires prev_receipt_id".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{Ed25519Signature, Keypair};
    use crate::receipt::ReceiptBuilder;
    use crate::stream::StreamId;
    use crate::types::ReceiptId;

    fn make_test_keypair() -> Keypair {
        Keypair::from_seed(&[0x42; 32])
    }

    #[test]
    fn test_valid_stream_init() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        assert!(validate_receipt(&receipt).is_ok());
    }

    #[test]
    fn test_valid_data_receipt() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let prev_id = ReceiptId::from_bytes([0xab; 32]);

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1736870400000)
            .kind(ReceiptKind::Data)
            .prev(prev_id)
            .payload(b"world".to_vec())
            .sign(&keypair);

        assert!(validate_receipt(&receipt).is_ok());
    }

    #[test]
    fn test_invalid_signature() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let mut receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        // Tamper with signature
        receipt.signature = Ed25519Signature::from_bytes([0xff; 64]);

        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::SignatureFailed)));
    }

    #[test]
    fn test_payload_hash_mismatch() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let mut receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        // Tamper with payload
        receipt.payload = b"tampered".to_vec().into();

        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::PayloadHashMismatch)));
    }

    #[test]
    fn test_stream_init_wrong_seq() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 5)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let result = validate_receipt(&receipt);
        assert!(matches!(
            result,
            Err(ValidationError::InvalidSequence {
                expected: 1,
                got: 5
            })
        ));
    }

    #[test]
    fn test_stream_init_with_prev() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let prev_id = ReceiptId::from_bytes([0xab; 32]);

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .prev(prev_id)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::InvalidPrevReceipt { .. })));
    }

    #[test]
    fn test_tombstone_missing_ref() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let prev_id = ReceiptId::from_bytes([0xab; 32]);

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1736870400000)
            .kind(ReceiptKind::Tombstone)
            .prev(prev_id)
            .payload(b"".to_vec())
            .sign(&keypair);

        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::TombstoneMissingRef)));
    }

    #[test]
    fn test_valid_tombstone() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let prev_id = ReceiptId::from_bytes([0xab; 32]);
        let tombstoned = ReceiptId::from_bytes([0xcd; 32]);

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1736870400000)
            .kind(ReceiptKind::Tombstone)
            .prev(prev_id)
            .add_ref(tombstoned)
            .payload(b"".to_vec())
            .sign(&keypair);

        assert!(validate_receipt(&receipt).is_ok());
    }

    #[test]
    fn test_too_many_refs() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let prev_id = ReceiptId::from_bytes([0xab; 32]);

        let mut builder = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1736870400000)
            .kind(ReceiptKind::Data)
            .prev(prev_id)
            .payload(b"hello".to_vec());

        // Add 17 refs (exceeds MAX_REFS of 16)
        for i in 0..17 {
            builder = builder.add_ref(ReceiptId::from_bytes([i; 32]));
        }

        let receipt = builder.sign(&keypair);
        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::TooManyRefs)));
    }

    #[test]
    fn test_seq_gt_1_without_prev() {
        let keypair = make_test_keypair();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 2)
            .timestamp(1736870400000)
            .kind(ReceiptKind::Data)
            // No prev()
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let result = validate_receipt(&receipt);
        assert!(matches!(result, Err(ValidationError::StructuralError(_))));
    }
}
