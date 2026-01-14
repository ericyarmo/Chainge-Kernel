//! Error types for the Chainge Kernel Core.

use thiserror::Error;

use crate::types::ReceiptId;

/// Core errors that can occur during receipt operations.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid signature")]
    InvalidSignature,

    #[error("invalid public key")]
    InvalidPublicKey,

    #[error("payload hash mismatch: expected {expected}, got {actual}")]
    PayloadHashMismatch { expected: String, actual: String },

    #[error("unsupported receipt version: {0}")]
    UnsupportedVersion(u8),

    #[error("malformed receipt: {0}")]
    MalformedReceipt(String),

    #[error("encoding error: {0}")]
    EncodingError(String),

    #[error("decoding error: {0}")]
    DecodingError(String),
}

/// Validation errors for receipt structure and signatures.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("signature verification failed")]
    SignatureFailed,

    #[error("payload hash does not match header")]
    PayloadHashMismatch,

    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),

    #[error("invalid sequence number: expected {expected}, got {got}")]
    InvalidSequence { expected: u64, got: u64 },

    #[error("invalid prev_receipt_id: expected {expected:?}, got {got:?}")]
    InvalidPrevReceipt {
        expected: Option<ReceiptId>,
        got: Option<ReceiptId>,
    },

    #[error("sequence conflict at seq {seq}: existing receipt {existing}, new receipt {new}")]
    SequenceConflict {
        seq: u64,
        existing: ReceiptId,
        new: ReceiptId,
    },

    #[error("stream is forked at seq {0}")]
    StreamForked(u64),

    #[error("refs array exceeds maximum length of 16")]
    TooManyRefs,

    #[error("tombstone must reference a receipt in refs[0]")]
    TombstoneMissingRef,

    #[error("receipt kind {0} is invalid")]
    InvalidKind(u16),

    #[error("structural error: {0}")]
    StructuralError(String),
}

impl From<CoreError> for ValidationError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::InvalidSignature | CoreError::InvalidPublicKey => {
                ValidationError::SignatureFailed
            }
            CoreError::PayloadHashMismatch { .. } => ValidationError::PayloadHashMismatch,
            CoreError::UnsupportedVersion(v) => ValidationError::UnsupportedVersion(v),
            CoreError::MalformedReceipt(msg) => ValidationError::StructuralError(msg),
            CoreError::EncodingError(msg) | CoreError::DecodingError(msg) => {
                ValidationError::StructuralError(msg)
            }
        }
    }
}
