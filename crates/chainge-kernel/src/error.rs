//! Error types for the kernel.

use thiserror::Error;

/// Kernel error type.
#[derive(Debug, Error)]
pub enum Error {
    /// Schema URI exceeds maximum length.
    #[error("schema too long: {0} bytes (max {max})", max = crate::MAX_SCHEMA_LEN)]
    SchemaTooLong(usize),

    /// Schema contains non-ASCII characters.
    #[error("schema must be ASCII only")]
    SchemaNotAscii,

    /// Too many refs.
    #[error("too many refs: {0} (max {max})", max = crate::MAX_REFS)]
    TooManyRefs(usize),

    /// Refs are not sorted.
    #[error("refs must be sorted lexicographically")]
    RefsNotSorted,

    /// Refs contain duplicates.
    #[error("refs must not contain duplicates")]
    RefsDuplicate,

    /// Payload exceeds maximum size.
    #[error("payload too large: {0} bytes (max {max})", max = crate::MAX_PAYLOAD_LEN)]
    PayloadTooLarge(usize),

    /// Invalid signature.
    #[error("invalid signature")]
    InvalidSignature,

    /// Invalid public key.
    #[error("invalid public key")]
    InvalidPublicKey,

    /// Malformed receipt bytes.
    #[error("malformed receipt: {0}")]
    MalformedReceipt(String),

    /// CBOR decoding error.
    #[error("decoding error: {0}")]
    DecodingError(String),

    /// Storage error.
    #[error("storage error: {0}")]
    StorageError(String),
}

/// Result type for kernel operations.
pub type Result<T> = std::result::Result<T, Error>;
