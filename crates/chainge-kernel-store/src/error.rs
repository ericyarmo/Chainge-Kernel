//! Error types for the store module.

use thiserror::Error;

/// Errors that can occur during store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Database error from SQLite.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Receipt serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Receipt not found.
    #[error("receipt not found: {0}")]
    NotFound(String),

    /// Conflict detected (e.g., different receipt at same position).
    #[error("conflict at stream {stream_id} seq {seq}: existing receipt {existing}")]
    Conflict {
        stream_id: String,
        seq: u64,
        existing: String,
    },

    /// Stream is forked.
    #[error("stream {0} is forked")]
    Forked(String),

    /// Invalid data in storage.
    #[error("invalid data: {0}")]
    InvalidData(String),

    /// Migration error.
    #[error("migration error: {0}")]
    Migration(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;
