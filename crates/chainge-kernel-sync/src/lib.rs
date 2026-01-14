//! # Chainge Kernel Sync
//!
//! Sync protocol for converging receipt sets between nodes.
//!
//! ## Overview
//!
//! The sync module implements an anti-entropy protocol that allows
//! two nodes to converge to the same receipt set through an untrusted
//! relay/transport.
//!
//! ## Key Properties
//!
//! - **Idempotent**: All operations are idempotent
//! - **Commutative**: Message order doesn't affect final state
//! - **Resumable**: Sync can be interrupted and resumed
//! - **Bandwidth-efficient**: Only transfer receipts the peer needs
//!
//! ## Usage
//!
//! ```rust,no_run
//! use chainge_kernel_sync::{SyncSession, SyncConfig, Transport};
//! use chainge_kernel_store::SqliteStore;
//!
//! async fn example() {
//!     // Set up store and transport
//!     // let store = SqliteStore::open("kernel.db").unwrap();
//!     // let transport: impl Transport = ...;
//!
//!     // Create sync session
//!     // let config = SyncConfig::default();
//!     // let mut session = SyncSession::new(store, transport, config);
//!
//!     // Sync with a peer
//!     // let report = session.sync_with(&peer_id).await?;
//!     // println!("Synced {} streams", report.streams_synced.len());
//! }
//! ```
//!
//! ## Message Flow
//!
//! ```text
//! Node A                              Node B
//!   |-------- Hello ------------------>|
//!   |<------- Hello -------------------|
//!   |-------- StreamHeads ------------>|
//!   |<------- StreamHeads -------------|
//!   |<------- NeedReceipts ------------|
//!   |-------- NeedReceipts ----------->|
//!   |-------- Receipts --------------->|
//!   |<------- Receipts ----------------|
//!   |<------- Ack ---------------------|
//!   |-------- Ack -------------------->|
//! ```

pub mod convergence;
pub mod error;
pub mod messages;
pub mod protocol;
pub mod transport;

pub use convergence::{compute_stream_state_hash, verify_convergence, ConvergenceResult};
pub use error::{Result, SyncError};
pub use messages::{
    limits, NodeId, ReceiptRequest, SeqRange, StreamHead, SyncErrorCode, SyncMessage,
    PROTOCOL_VERSION,
};
pub use protocol::{SyncConfig, SyncReport, SyncSession};
pub use transport::{memory::MemoryNetwork, memory::MemoryTransport, Transport};
