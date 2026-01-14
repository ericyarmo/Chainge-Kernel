//! Error types for the sync module.

use thiserror::Error;

use crate::messages::SyncErrorCode;

/// Errors that can occur during sync operations.
#[derive(Debug, Error)]
pub enum SyncError {
    /// Protocol version mismatch with peer.
    #[error("protocol version mismatch: local={local}, peer={peer}")]
    VersionMismatch { local: u8, peer: u8 },

    /// Message validation failed.
    #[error("invalid message: {0}")]
    InvalidMessage(String),

    /// Transport-level error.
    #[error("transport error: {0}")]
    TransportError(String),

    /// Store operation failed.
    #[error("store error: {0}")]
    StoreError(#[from] chainge_kernel_store::StoreError),

    /// Peer sent an error message.
    #[error("peer error ({code:?}): {message}")]
    PeerError { code: SyncErrorCode, message: String },

    /// Timeout waiting for peer.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Receipt validation failed.
    #[error("validation error: {0}")]
    ValidationError(#[from] chainge_kernel_core::ValidationError),

    /// Peer is not connected.
    #[error("peer not connected: {0}")]
    PeerNotConnected(String),

    /// Sync was cancelled.
    #[error("sync cancelled")]
    Cancelled,
}

/// Result type for sync operations.
pub type Result<T> = std::result::Result<T, SyncError>;
