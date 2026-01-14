//! # Chainge Kernel Store
//!
//! Storage abstraction for the Chainge Kernel. Provides a trait-based interface
//! for receipt persistence with SQLite and in-memory implementations.
//!
//! ## Overview
//!
//! The store module abstracts receipt storage behind the [`Store`] trait,
//! allowing the kernel to be storage-agnostic. The primary implementation
//! is [`SqliteStore`], with [`MemoryStore`] for testing.
//!
//! ## Key Types
//!
//! - [`Store`] - The async trait for all storage operations
//! - [`SqliteStore`] - SQLite-based persistent storage
//! - [`MemoryStore`] - In-memory storage for tests
//! - [`InsertResult`] - Result of inserting a receipt
//! - [`Fork`] - Evidence of a fork in a stream
//!
//! ## Usage
//!
//! ```rust,no_run
//! use chainge_kernel_store::{SqliteStore, Store, InsertResult};
//! use chainge_kernel_core::{Receipt, canonical_bytes};
//!
//! async fn example() {
//!     // Open a SQLite database
//!     let store = SqliteStore::open("kernel.db").unwrap();
//!
//!     // Or use an in-memory database for testing
//!     let store = SqliteStore::open_memory().unwrap();
//!
//!     // Insert a receipt
//!     // let receipt: Receipt = ...;
//!     // let canonical = canonical_bytes(&receipt);
//!     // let result = store.insert_receipt(&receipt, &canonical).await.unwrap();
//! }
//! ```
//!
//! ## Design Notes
//!
//! - **Idempotent inserts**: Inserting the same receipt twice returns `AlreadyExists`
//! - **Conflict detection**: Different receipt at same position returns `Conflict`
//! - **Gap tracking**: Missing sequence numbers tracked for sync protocol
//! - **Fork detection**: Multiple receipts at same position recorded as evidence

pub mod error;
pub mod memory;
pub mod migration;
pub mod sqlite;
pub mod traits;

pub use error::{Result, StoreError};
pub use memory::MemoryStore;
pub use sqlite::SqliteStore;
pub use traits::{Fork, InsertResult, Store, StoreExt};
