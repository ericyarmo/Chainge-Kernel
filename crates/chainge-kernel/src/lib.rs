//! # Chainge Kernel
//!
//! Minimal cryptographic kernel for verifiable receipts.
//!
//! The kernel does ONE thing: store and verify signed receipts.
//!
//! ## Core Types
//!
//! - [`Receipt`] - An immutable, signed attestation
//! - [`ReceiptId`] - Content-addressed identifier (32 bytes)
//! - [`Author`] - Ed25519 public key (32 bytes)
//!
//! ## Core Invariants
//!
//! 1. **Content-addressable**: `receipt_id = sha256(content || signature)`
//! 2. **Author authenticity**: Signature verifies against author
//! 3. **Idempotent ingestion**: Same receipt → same ID → no-op
//! 4. **Causal ordering**: `refs` establish happened-before
//! 5. **Merge = union**: Sync converges by set union
//!
//! ## Example
//!
//! ```
//! use chainge_kernel::{Keypair, Receipt};
//!
//! let keypair = Keypair::generate();
//! let receipt = Receipt::new(
//!     &keypair,
//!     "civic.presence/v1",
//!     vec![],
//!     b"hello world".to_vec(),
//! ).unwrap();
//!
//! assert!(receipt.verify().is_ok());
//! ```

mod canonical;
mod crypto;
mod error;
mod receipt;
mod store;

pub use canonical::{canonical_content, canonical_receipt, sign_message, ID_DOMAIN, SIGN_DOMAIN};
pub use crypto::{Author, Keypair, Sha256Hash, Signature};
pub use error::{Error, Result};
pub use receipt::{Receipt, ReceiptId};
pub use store::{sync, InsertResult, MemoryStore, Store, SyncReport};

/// Maximum schema URI length in bytes.
pub const MAX_SCHEMA_LEN: usize = 256;

/// Maximum number of refs per receipt.
pub const MAX_REFS: usize = 128;

/// Maximum payload size in bytes (64 KB).
pub const MAX_PAYLOAD_LEN: usize = 65536;
