//! # Chainge Kernel Permissions
//!
//! Permission receipts and encrypted payload sharing.
//!
//! ## Overview
//!
//! The permissions module implements access control as receipts. Instead of
//! mutable database flags, permissions are expressed as Grant and Revoke
//! receipts that are replayed to compute the current permission state.
//!
//! ## Key Concepts
//!
//! - **Grant**: A receipt that grants a permission to a recipient
//! - **Revoke**: A receipt that revokes a previous grant
//! - **KeyShare**: A receipt that shares an encryption key with a recipient
//! - **EncryptedPayload**: A wrapper for encrypted receipt payloads
//!
//! ## Encryption Model
//!
//! Encrypted content uses a two-layer key model:
//!
//! 1. **Content Key**: A symmetric key (ChaCha20-Poly1305) that encrypts the payload
//! 2. **Key Shares**: The content key is shared with recipients via X25519 ECDH
//!
//! This allows:
//! - Key rotation without re-encrypting content
//! - Adding new recipients without re-encrypting content
//! - Revocation by simply not sharing new keys
//!
//! ## Usage
//!
//! ```rust,no_run
//! use chainge_kernel_perms::{
//!     GrantPayload, PermissionScope, PermissionState,
//!     KeySharePayload, EncryptedPayload, EncryptedPayloadBuilder,
//!     X25519StaticSecret, EncryptionKey,
//! };
//!
//! // Create a grant
//! // let grant = GrantPayload::read_stream(recipient_pubkey, stream_id);
//!
//! // Build encrypted content
//! // let builder = EncryptedPayloadBuilder::new(plaintext);
//! // let content_key = builder.content_key().clone();
//! // let envelope = builder.build().unwrap();
//!
//! // Share key with recipient
//! // let keyshare = KeySharePayload::create(grant_id, &content_key, &recipient_x25519);
//! ```

pub mod crypto;
pub mod envelope;
pub mod error;
pub mod grant;
pub mod keyshare;
pub mod state;

pub use crypto::{
    EncryptionKey, EncryptionNonce, EphemeralKeyPair, SharedKey, X25519PublicKey,
    X25519StaticSecret,
};
pub use envelope::{EncryptedPayload, EncryptedPayloadBuilder, EncryptionFormat};
pub use error::{PermsError, Result};
pub use grant::{Conditions, GrantPayload, PermissionScope, RevokePayload};
pub use keyshare::KeySharePayload;
pub use state::{GrantState, PermissionState};
