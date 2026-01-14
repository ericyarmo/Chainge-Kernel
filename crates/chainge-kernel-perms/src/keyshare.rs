//! Key sharing via X25519 key agreement.
//!
//! When granting read access to encrypted content, the grantor shares
//! the symmetric key with the recipient via a KeyShare receipt.

use serde::{Deserialize, Serialize};

use chainge_kernel_core::ReceiptId;

use crate::crypto::{
    EphemeralKeyPair, EncryptionKey, EncryptionNonce, X25519PublicKey, X25519StaticSecret,
};
use crate::error::{PermsError, Result};

/// Payload for a KeyShare receipt.
///
/// Used to share an encrypted symmetric key with a grant recipient.
/// The key is encrypted using X25519 ECDH + ChaCha20-Poly1305.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySharePayload {
    /// The grant this key share is for.
    pub grant_receipt_id: ReceiptId,

    /// Ephemeral X25519 public key (sender's side of ECDH).
    pub ephemeral_public: X25519PublicKey,

    /// The symmetric key, encrypted with the derived shared secret.
    pub encrypted_key: Vec<u8>,

    /// Nonce used for encryption.
    pub nonce: EncryptionNonce,
}

impl KeySharePayload {
    /// Create a new key share by encrypting a symmetric key for a recipient.
    ///
    /// # Arguments
    /// * `grant_receipt_id` - The grant this key share is associated with
    /// * `symmetric_key` - The key to share (e.g., content encryption key)
    /// * `recipient_public` - Recipient's X25519 public key
    ///
    /// # Returns
    /// A KeySharePayload that can be serialized into a receipt payload.
    pub fn create(
        grant_receipt_id: ReceiptId,
        symmetric_key: &EncryptionKey,
        recipient_public: &X25519PublicKey,
    ) -> Self {
        // Generate ephemeral key pair
        let ephemeral = EphemeralKeyPair::generate();
        let ephemeral_public = ephemeral.public_key();

        // Derive shared secret
        let shared = ephemeral.diffie_hellman(recipient_public);

        // Derive encryption key from shared secret
        let context = grant_receipt_id.0.as_slice();
        let wrap_key = shared.derive_encryption_key(context);

        // Encrypt the symmetric key
        let nonce = EncryptionNonce::generate();
        let encrypted_key = wrap_key
            .encrypt(symmetric_key.as_bytes(), &nonce)
            .expect("encryption should not fail with valid key");

        Self {
            grant_receipt_id,
            ephemeral_public,
            encrypted_key,
            nonce,
        }
    }

    /// Decrypt the shared key using the recipient's secret key.
    ///
    /// # Arguments
    /// * `recipient_secret` - The recipient's X25519 secret key
    ///
    /// # Returns
    /// The decrypted symmetric key.
    pub fn decrypt(&self, recipient_secret: &X25519StaticSecret) -> Result<EncryptionKey> {
        // Derive shared secret from sender's ephemeral public
        let shared = recipient_secret.diffie_hellman(&self.ephemeral_public);

        // Derive decryption key
        let context = self.grant_receipt_id.0.as_slice();
        let wrap_key = shared.derive_encryption_key(context);

        // Decrypt the symmetric key
        let key_bytes = wrap_key.decrypt(&self.encrypted_key, &self.nonce)?;

        // Convert to EncryptionKey
        if key_bytes.len() != 32 {
            return Err(PermsError::DecryptionError(format!(
                "invalid key length: expected 32, got {}",
                key_bytes.len()
            )));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);
        Ok(EncryptionKey::from_bytes(arr))
    }

    /// Serialize to CBOR bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).expect("CBOR serialization failed");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ciborium::from_reader(bytes).map_err(|e| PermsError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyshare_roundtrip() {
        // Recipient's key pair
        let recipient_secret = X25519StaticSecret::generate();
        let recipient_public = recipient_secret.public_key();

        // The symmetric key to share
        let content_key = EncryptionKey::generate();

        // Create key share
        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let keyshare = KeySharePayload::create(grant_id, &content_key, &recipient_public);

        // Recipient decrypts
        let decrypted = keyshare.decrypt(&recipient_secret).unwrap();

        assert_eq!(content_key.as_bytes(), decrypted.as_bytes());
    }

    #[test]
    fn test_keyshare_wrong_recipient_fails() {
        let recipient_secret = X25519StaticSecret::generate();
        let recipient_public = recipient_secret.public_key();
        let wrong_secret = X25519StaticSecret::generate();

        let content_key = EncryptionKey::generate();
        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let keyshare = KeySharePayload::create(grant_id, &content_key, &recipient_public);

        // Wrong recipient should fail
        assert!(keyshare.decrypt(&wrong_secret).is_err());
    }

    #[test]
    fn test_keyshare_serialization() {
        let recipient_secret = X25519StaticSecret::generate();
        let recipient_public = recipient_secret.public_key();
        let content_key = EncryptionKey::generate();
        let grant_id = ReceiptId::from_bytes([0x42; 32]);

        let keyshare = KeySharePayload::create(grant_id, &content_key, &recipient_public);
        let bytes = keyshare.to_bytes();
        let recovered = KeySharePayload::from_bytes(&bytes).unwrap();

        assert_eq!(keyshare, recovered);
    }
}
