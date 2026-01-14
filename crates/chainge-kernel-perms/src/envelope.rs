//! Encrypted payload envelope.
//!
//! When a receipt payload needs to be encrypted, it is wrapped in an
//! EncryptedPayload envelope that includes the ciphertext and metadata.

use serde::{Deserialize, Serialize};

use crate::crypto::{EncryptionKey, EncryptionNonce};
use crate::error::{PermsError, Result};

/// Format identifier for encrypted payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EncryptionFormat {
    /// ChaCha20-Poly1305 with 256-bit key.
    ChaCha20Poly1305 = 1,
}

/// An encrypted payload envelope.
///
/// This structure wraps encrypted data and provides the metadata
/// needed to decrypt it (assuming the recipient has the key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// Encryption algorithm used.
    pub format: EncryptionFormat,

    /// Nonce used for encryption (unique per encryption).
    pub nonce: EncryptionNonce,

    /// The encrypted data (includes authentication tag).
    pub ciphertext: Vec<u8>,
}

impl EncryptedPayload {
    /// Encrypt plaintext with the given key.
    pub fn encrypt(plaintext: &[u8], key: &EncryptionKey) -> Result<Self> {
        let nonce = EncryptionNonce::generate();
        let ciphertext = key.encrypt(plaintext, &nonce)?;

        Ok(Self {
            format: EncryptionFormat::ChaCha20Poly1305,
            nonce,
            ciphertext,
        })
    }

    /// Decrypt with the given key.
    pub fn decrypt(&self, key: &EncryptionKey) -> Result<Vec<u8>> {
        match self.format {
            EncryptionFormat::ChaCha20Poly1305 => key.decrypt(&self.ciphertext, &self.nonce),
        }
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

    /// Check if these bytes look like an encrypted payload.
    ///
    /// Does a quick check of the CBOR structure without full parsing.
    pub fn is_encrypted(bytes: &[u8]) -> bool {
        // An encrypted payload starts with a CBOR map (0xa3 = map with 3 items)
        // This is a heuristic, not a guarantee
        bytes.first() == Some(&0xa3)
    }

    /// Get the size of the ciphertext.
    pub fn ciphertext_len(&self) -> usize {
        self.ciphertext.len()
    }
}

/// Builder for creating encrypted payloads with multiple recipients.
pub struct EncryptedPayloadBuilder {
    plaintext: Vec<u8>,
    content_key: EncryptionKey,
}

impl EncryptedPayloadBuilder {
    /// Start building an encrypted payload.
    pub fn new(plaintext: impl Into<Vec<u8>>) -> Self {
        Self {
            plaintext: plaintext.into(),
            content_key: EncryptionKey::generate(),
        }
    }

    /// Get the content encryption key.
    ///
    /// This key should be shared with recipients via KeyShare receipts.
    pub fn content_key(&self) -> &EncryptionKey {
        &self.content_key
    }

    /// Build the encrypted payload.
    pub fn build(self) -> Result<EncryptedPayload> {
        EncryptedPayload::encrypt(&self.plaintext, &self.content_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = EncryptionKey::generate();
        let plaintext = b"hello, encrypted world!";

        let envelope = EncryptedPayload::encrypt(plaintext, &key).unwrap();
        let decrypted = envelope.decrypt(&key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_envelope_serialization() {
        let key = EncryptionKey::generate();
        let envelope = EncryptedPayload::encrypt(b"test", &key).unwrap();

        let bytes = envelope.to_bytes();
        let recovered = EncryptedPayload::from_bytes(&bytes).unwrap();

        assert_eq!(envelope, recovered);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = EncryptionKey::generate();
        let key2 = EncryptionKey::generate();

        let envelope = EncryptedPayload::encrypt(b"secret", &key1).unwrap();

        assert!(envelope.decrypt(&key2).is_err());
    }

    #[test]
    fn test_builder_pattern() {
        let builder = EncryptedPayloadBuilder::new(b"my secret data".to_vec());
        let content_key = builder.content_key().as_bytes().clone();

        let envelope = builder.build().unwrap();

        // Decrypt with the content key
        let key = EncryptionKey::from_bytes(content_key);
        let decrypted = envelope.decrypt(&key).unwrap();

        assert_eq!(decrypted, b"my secret data");
    }
}
