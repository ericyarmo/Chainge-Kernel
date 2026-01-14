//! Cryptographic primitives for the Chainge Kernel.
//!
//! Wraps Ed25519 signing and Blake3 hashing with strong types.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::CoreError;

/// A 32-byte Blake3 hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Blake3Hash(pub [u8; 32]);

impl Blake3Hash {
    /// Compute the Blake3 hash of the given data.
    pub fn hash(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// The zero hash (sentinel value).
    pub const ZERO: Self = Self([0u8; 32]);
}

impl fmt::Debug for Blake3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blake3({})", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for Blake3Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Blake3Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A 32-byte Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ed25519PublicKey(pub [u8; 32]);

impl Ed25519PublicKey {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Verify a signature over a message.
    pub fn verify(&self, message: &[u8], signature: &Ed25519Signature) -> Result<(), CoreError> {
        let verifying_key =
            VerifyingKey::from_bytes(&self.0).map_err(|_| CoreError::InvalidPublicKey)?;

        let sig =
            Signature::from_bytes(&signature.0);

        verifying_key
            .verify(message, &sig)
            .map_err(|_| CoreError::InvalidSignature)
    }
}

impl fmt::Debug for Ed25519PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ed25519Pub({})", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for Ed25519PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Ed25519PublicKey {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A 64-byte Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ed25519Signature(pub [u8; 64]);

impl Ed25519Signature {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// The zero signature (invalid, used as placeholder).
    pub const ZERO: Self = Self([0u8; 64]);
}

impl fmt::Debug for Ed25519Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ed25519Sig({}...)", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for Ed25519Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 64]> for Ed25519Signature {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

/// A keypair for signing receipts.
///
/// This wraps ed25519-dalek's SigningKey.
#[derive(Clone)]
pub struct Keypair {
    signing_key: SigningKey,
}

impl Keypair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let signing_key = SigningKey::generate(&mut rng);
        Self { signing_key }
    }

    /// Create from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        Self { signing_key }
    }

    /// Get the public key.
    pub fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey(self.signing_key.verifying_key().to_bytes())
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Ed25519Signature {
        let sig = self.signing_key.sign(message);
        Ed25519Signature(sig.to_bytes())
    }

    /// Get the raw seed bytes (secret key material).
    pub fn seed(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keypair({:?})", self.public_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_sign_verify() {
        let keypair = Keypair::generate();
        let message = b"hello world";
        let signature = keypair.sign(message);

        // Valid signature should verify
        keypair
            .public_key()
            .verify(message, &signature)
            .expect("valid signature should verify");

        // Tampered message should fail
        let tampered = b"hello worlD";
        assert!(keypair.public_key().verify(tampered, &signature).is_err());
    }

    #[test]
    fn test_keypair_deterministic_from_seed() {
        let seed = [0x42u8; 32];
        let kp1 = Keypair::from_seed(&seed);
        let kp2 = Keypair::from_seed(&seed);
        assert_eq!(kp1.public_key(), kp2.public_key());
    }

    #[test]
    fn test_blake3_hash() {
        let data = b"test data";
        let h1 = Blake3Hash::hash(data);
        let h2 = Blake3Hash::hash(data);
        assert_eq!(h1, h2);

        let different = b"different data";
        let h3 = Blake3Hash::hash(different);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_public_key_hex_roundtrip() {
        let keypair = Keypair::generate();
        let pk = keypair.public_key();
        let hex = pk.to_hex();
        let recovered = Ed25519PublicKey::from_hex(&hex).unwrap();
        assert_eq!(pk, recovered);
    }
}
