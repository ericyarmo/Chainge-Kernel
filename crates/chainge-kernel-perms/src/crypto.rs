//! Cryptographic utilities for the permissions module.
//!
//! Provides X25519 key agreement and ChaCha20-Poly1305 authenticated encryption.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret, StaticSecret};

use crate::error::{PermsError, Result};

/// An X25519 public key (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct X25519PublicKey(pub [u8; 32]);

impl X25519PublicKey {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to x25519-dalek PublicKey.
    pub fn to_dalek(&self) -> PublicKey {
        PublicKey::from(self.0)
    }
}

impl From<PublicKey> for X25519PublicKey {
    fn from(pk: PublicKey) -> Self {
        Self(*pk.as_bytes())
    }
}

/// An X25519 static secret key.
///
/// Unlike Ed25519, X25519 keys are only for key agreement, not signing.
pub struct X25519StaticSecret(StaticSecret);

impl X25519StaticSecret {
    /// Generate a new random secret.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(StaticSecret::from(bytes))
    }

    /// Create from seed bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(StaticSecret::from(bytes))
    }

    /// Derive the public key.
    pub fn public_key(&self) -> X25519PublicKey {
        X25519PublicKey::from(PublicKey::from(&self.0))
    }

    /// Perform key agreement with a peer's public key.
    pub fn diffie_hellman(&self, peer_public: &X25519PublicKey) -> SharedKey {
        let shared = self.0.diffie_hellman(&peer_public.to_dalek());
        SharedKey(*shared.as_bytes())
    }
}

/// A shared secret derived from X25519 key agreement.
#[derive(Clone)]
pub struct SharedKey([u8; 32]);

impl SharedKey {
    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive an encryption key from this shared secret.
    ///
    /// Uses HKDF-like derivation for domain separation.
    pub fn derive_encryption_key(&self, context: &[u8]) -> EncryptionKey {
        use blake3::Hasher;
        let mut hasher = Hasher::new_derive_key("chainge-perms-v0-encryption");
        hasher.update(&self.0);
        hasher.update(context);
        EncryptionKey(*hasher.finalize().as_bytes())
    }
}

/// A 256-bit symmetric encryption key for ChaCha20-Poly1305.
#[derive(Clone)]
pub struct EncryptionKey([u8; 32]);

impl EncryptionKey {
    /// Generate a new random key.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encrypt data with this key.
    pub fn encrypt(&self, plaintext: &[u8], nonce: &EncryptionNonce) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.0)
            .map_err(|e| PermsError::EncryptionError(e.to_string()))?;

        let nonce = Nonce::from_slice(&nonce.0);
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| PermsError::EncryptionError(e.to_string()))
    }

    /// Decrypt data with this key.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &EncryptionNonce) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.0)
            .map_err(|e| PermsError::DecryptionError(e.to_string()))?;

        let nonce = Nonce::from_slice(&nonce.0);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| PermsError::DecryptionError(e.to_string()))
    }
}

/// A 96-bit nonce for ChaCha20-Poly1305.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionNonce(pub [u8; 12]);

impl EncryptionNonce {
    /// Generate a new random nonce.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 12];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 12]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

/// Ephemeral key pair for one-time key agreement.
pub struct EphemeralKeyPair {
    secret: EphemeralSecret,
    public: X25519PublicKey,
}

impl EphemeralKeyPair {
    /// Generate a new ephemeral key pair.
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(rand::thread_rng());
        let public = X25519PublicKey::from(PublicKey::from(&secret));
        Self { secret, public }
    }

    /// Get the public key.
    pub fn public_key(&self) -> X25519PublicKey {
        self.public
    }

    /// Perform key agreement with a peer's public key.
    ///
    /// Consumes the ephemeral secret (can only be used once).
    pub fn diffie_hellman(self, peer_public: &X25519PublicKey) -> SharedKey {
        let shared = self.secret.diffie_hellman(&peer_public.to_dalek());
        SharedKey(*shared.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_key_agreement() {
        // Alice generates a static key
        let alice_secret = X25519StaticSecret::generate();
        let alice_public = alice_secret.public_key();

        // Bob generates a static key
        let bob_secret = X25519StaticSecret::generate();
        let bob_public = bob_secret.public_key();

        // Both derive the same shared secret
        let alice_shared = alice_secret.diffie_hellman(&bob_public);
        let bob_shared = bob_secret.diffie_hellman(&alice_public);

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_ephemeral_key_agreement() {
        // Bob has a static key
        let bob_secret = X25519StaticSecret::generate();
        let bob_public = bob_secret.public_key();

        // Alice creates an ephemeral key
        let alice_ephemeral = EphemeralKeyPair::generate();
        let alice_ephemeral_public = alice_ephemeral.public_key();

        // Alice derives shared secret
        let alice_shared = alice_ephemeral.diffie_hellman(&bob_public);

        // Bob derives shared secret from Alice's ephemeral public
        let bob_shared = bob_secret.diffie_hellman(&alice_ephemeral_public);

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = EncryptionKey::generate();
        let nonce = EncryptionNonce::generate();
        let plaintext = b"hello, world!";

        let ciphertext = key.encrypt(plaintext, &nonce).unwrap();
        assert_ne!(ciphertext, plaintext);

        let decrypted = key.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = EncryptionKey::generate();
        let key2 = EncryptionKey::generate();
        let nonce = EncryptionNonce::generate();

        let ciphertext = key1.encrypt(b"secret", &nonce).unwrap();

        // Wrong key should fail
        assert!(key2.decrypt(&ciphertext, &nonce).is_err());
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let shared = SharedKey([0x42; 32]);
        let context = b"test-context";

        let key1 = shared.derive_encryption_key(context);
        let key2 = shared.derive_encryption_key(context);

        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_key_derivation_different_contexts() {
        let shared = SharedKey([0x42; 32]);

        let key1 = shared.derive_encryption_key(b"context-a");
        let key2 = shared.derive_encryption_key(b"context-b");

        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }
}
