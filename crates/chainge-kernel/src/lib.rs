//! # Chainge Kernel
//!
//! The unified API for the Chainge system - verifiable memory through
//! receipts, streams, and permissions.
//!
//! ## Overview
//!
//! The Chainge Kernel provides a portable, offline-first library for:
//!
//! - **Receipts**: Immutable, signed events that form the atomic unit of memory
//! - **Streams**: Ordered, append-only logs of receipts from a single author
//! - **Permissions**: Access control expressed as receipts (grant/revoke)
//! - **Sync**: Convergence of receipt sets across nodes via untrusted relays
//!
//! ## Key Concepts
//!
//! - **Receipt**: Immutable. Never edited. Changes are new receipts.
//! - **Stream**: Owned by a single author. Sequence numbers are monotonic.
//! - **Tombstone**: A receipt that supersedes a previous receipt.
//! - **Fork**: Detection when an author creates conflicting receipts.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use chainge_kernel::{Kernel, KernelConfig};
//! use chainge_kernel::core::Keypair;
//! use chainge_kernel::store::SqliteStore;
//!
//! async fn example() {
//!     // Create a keypair for this kernel instance
//!     let keypair = Keypair::generate();
//!
//!     // Open storage
//!     let store = SqliteStore::open("kernel.db").unwrap();
//!
//!     // Create the kernel
//!     let kernel = Kernel::new(keypair, store, KernelConfig::default());
//!
//!     // Create a stream
//!     let (stream_id, init_id) = kernel
//!         .create_stream("my-stream", b"initial payload")
//!         .await
//!         .unwrap();
//!
//!     // Append to the stream
//!     // let receipt_id = kernel
//!     //     .append(&stream_id, ReceiptKind::Data, b"more data")
//!     //     .await
//!     //     .unwrap();
//! }
//! ```
//!
//! ## Re-exports
//!
//! This crate re-exports the component crates for convenience:
//!
//! - `chainge_kernel::core` - Core primitives (Receipt, ReceiptId, etc.)
//! - `chainge_kernel::store` - Storage abstraction and SQLite
//! - `chainge_kernel::sync` - Sync protocol
//! - `chainge_kernel::perms` - Permissions and encryption

pub mod error;
pub mod kernel;

// Re-export component crates
pub use chainge_kernel_core as core;
pub use chainge_kernel_perms as perms;
pub use chainge_kernel_store as store;
pub use chainge_kernel_sync as sync;

// Re-export main types for convenience
pub use error::{KernelError, Result};
pub use kernel::{IngestResult, Kernel, KernelConfig};

// Re-export commonly used core types
pub use chainge_kernel_core::{
    Blake3Hash, Ed25519PublicKey, Ed25519Signature, Keypair, Receipt, ReceiptBuilder, ReceiptId,
    ReceiptKind, StreamHealth, StreamId, StreamState,
};
