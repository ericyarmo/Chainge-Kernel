//! Error types for the permissions module.

use thiserror::Error;

/// Errors that can occur during permission operations.
#[derive(Debug, Error)]
pub enum PermsError {
    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Grant not found.
    #[error("grant not found: {0}")]
    GrantNotFound(String),

    /// Grant has been revoked.
    #[error("grant has been revoked: {0}")]
    GrantRevoked(String),

    /// Grant has expired.
    #[error("grant has expired: {0}")]
    GrantExpired(String),

    /// Invalid grant payload.
    #[error("invalid grant payload: {0}")]
    InvalidGrant(String),

    /// Encryption error.
    #[error("encryption error: {0}")]
    EncryptionError(String),

    /// Decryption error.
    #[error("decryption error: {0}")]
    DecryptionError(String),

    /// Key derivation error.
    #[error("key derivation error: {0}")]
    KeyDerivationError(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Core error.
    #[error("core error: {0}")]
    CoreError(#[from] chainge_kernel_core::CoreError),
}

/// Result type for permission operations.
pub type Result<T> = std::result::Result<T, PermsError>;
