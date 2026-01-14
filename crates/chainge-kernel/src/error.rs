//! Error types for the Kernel.

use chainge_kernel_core::{ReceiptId, StreamId, ValidationError};
use chainge_kernel_perms::PermsError;
use chainge_kernel_store::StoreError;
use chainge_kernel_sync::SyncError;
use thiserror::Error;

/// Errors that can occur during Kernel operations.
#[derive(Debug, Error)]
pub enum KernelError {
    /// Validation error.
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Storage error.
    #[error("storage error: {0}")]
    Store(#[from] StoreError),

    /// Sync error.
    #[error("sync error: {0}")]
    Sync(#[from] SyncError),

    /// Permission error.
    #[error("permission error: {0}")]
    Permission(#[from] PermsError),

    /// Stream not found.
    #[error("stream not found: {0:?}")]
    StreamNotFound(StreamId),

    /// Stream already exists.
    #[error("stream already exists: {0:?}")]
    StreamExists(StreamId),

    /// Receipt not found.
    #[error("receipt not found: {0}")]
    ReceiptNotFound(String),

    /// Conflict detected (different receipt at same position).
    #[error("conflict at stream {stream_id:?} seq {seq}: existing receipt {existing}")]
    Conflict {
        stream_id: StreamId,
        seq: u64,
        existing: ReceiptId,
    },

    /// Not authorized.
    #[error("not authorized: {0}")]
    NotAuthorized(String),

    /// Invalid operation.
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}

/// Result type for Kernel operations.
pub type Result<T> = std::result::Result<T, KernelError>;
