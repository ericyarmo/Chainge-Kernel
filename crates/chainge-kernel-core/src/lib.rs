//! # Chainge Kernel Core
//!
//! Pure primitives for the Chainge Kernel: receipts, streams, and canonicalization.
//!
//! This crate contains no I/O, no storage, no networking. It is pure computation
//! over cryptographic data structures.
//!
//! ## Key Types
//!
//! - [`Receipt`] - The atomic unit of verifiable memory
//! - [`ReceiptId`] - Content-addressed identifier (Blake3 hash)
//! - [`StreamId`] - Identifier for an ordered log of receipts
//! - [`ReceiptKind`] - Discriminator for payload interpretation
//!
//! ## Canonicalization
//!
//! All receipts are encoded using deterministic CBOR. See [`canonical`] module.

pub mod canonical;
pub mod crypto;
pub mod error;
pub mod receipt;
pub mod stream;
pub mod types;
pub mod validation;

pub use canonical::{canonical_bytes, canonical_header_bytes};
pub use crypto::{Blake3Hash, Ed25519PublicKey, Ed25519Signature, Keypair};
pub use error::{CoreError, ValidationError};
pub use receipt::{Receipt, ReceiptBuilder, ReceiptHeader, ReceiptKind};
pub use stream::{StreamHealth, StreamId, StreamState};
pub use types::ReceiptId;
pub use validation::{validate_receipt, validate_receipt_structure};
