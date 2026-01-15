//! Cryptographic primitives: Ed25519 signing and SHA-256 hashing.
//!
//! **Why SHA-256?**
//! - Universal: hardware acceleration everywhere (Intel SHA-NI, ARM SHA)
//! - Interoperable: CIDv1/IPFS ecosystem, every language has it
//! - Cost of interoperability asymptotes to $0 with ubiquitous primitives

use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;

use crate::error::{Error, Result};

/// A 32-byte SHA-256 hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Hash(pub [u8; 32]);

impl Sha256Hash {
    /// Compute the SHA-256 hash of data.
    pub fn hash(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        Self(result.into())
    }

    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Convert to CIDv1 (dag-cbor, base32).
    ///
    /// Format: `b` + base32lower(0x01 || 0x71 || 0x12 || 0x20 || hash)
    pub fn to_cid(&self) -> String {
        let mut cid_bytes = Vec::with_capacity(36);
        cid_bytes.push(0x01); // CIDv1
        cid_bytes.push(0x71); // dag-cbor codec
        cid_bytes.push(0x12); // sha2-256 multihash
        cid_bytes.push(0x20); // 32 bytes
        cid_bytes.extend_from_slice(&self.0);

        // Base32 lower with 'b' prefix (multibase)
        format!("b{}", base32_encode(&cid_bytes))
    }
}

impl fmt::Debug for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SHA256({}...)", &self.to_hex()[..8])
    }
}

impl AsRef<[u8]> for Sha256Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Sha256Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A 32-byte Ed25519 public key (the author of a receipt).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Author(pub [u8; 32]);

impl Author {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s).map_err(|e| Error::MalformedReceipt(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(Error::InvalidPublicKey);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Verify a signature over a message.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let verifying_key =
            VerifyingKey::from_bytes(&self.0).map_err(|_| Error::InvalidPublicKey)?;
        let sig = DalekSignature::from_bytes(&signature.0);
        verifying_key
            .verify(message, &sig)
            .map_err(|_| Error::InvalidSignature)
    }
}

impl fmt::Debug for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Author({}...)", &self.to_hex()[..8])
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for Author {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Author {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// A 64-byte Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// Create from raw bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Get raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sig({}...)", &self.to_hex()[..8])
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 64]> for Signature {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

/// A keypair for signing receipts.
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

    /// Get the public key (author).
    pub fn author(&self) -> Author {
        Author(self.signing_key.verifying_key().to_bytes())
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.signing_key.sign(message);
        Signature(sig.to_bytes())
    }
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keypair({:?})", self.author())
    }
}

// RFC 4648 Base32 encoding (lowercase, no padding)
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits_in_buffer = 0;

    for &byte in data {
        buffer = (buffer << 8) | (byte as u64);
        bits_in_buffer += 8;

        while bits_in_buffer >= 5 {
            bits_in_buffer -= 5;
            let index = ((buffer >> bits_in_buffer) & 0x1f) as usize;
            result.push(ALPHABET[index] as char);
        }
    }

    if bits_in_buffer > 0 {
        let index = ((buffer << (5 - bits_in_buffer)) & 0x1f) as usize;
        result.push(ALPHABET[index] as char);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let keypair = Keypair::generate();
        let message = b"hello world";
        let signature = keypair.sign(message);

        // Valid signature should verify
        keypair.author().verify(message, &signature).unwrap();

        // Tampered message should fail
        let tampered = b"hello worlD";
        assert!(keypair.author().verify(tampered, &signature).is_err());
    }

    #[test]
    fn test_deterministic_from_seed() {
        let seed = [0x42u8; 32];
        let kp1 = Keypair::from_seed(&seed);
        let kp2 = Keypair::from_seed(&seed);
        assert_eq!(kp1.author(), kp2.author());
    }

    #[test]
    fn test_sha256_hash() {
        let h1 = Sha256Hash::hash(b"test");
        let h2 = Sha256Hash::hash(b"test");
        assert_eq!(h1, h2);

        let h3 = Sha256Hash::hash(b"different");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_cid_format() {
        // Known test vector
        let hash = Sha256Hash::hash(b"hello");
        let cid = hash.to_cid();

        // CID should start with 'b' (base32 multibase prefix)
        assert!(cid.starts_with('b'));

        // CID should be lowercase
        assert_eq!(cid, cid.to_lowercase());
    }

    #[test]
    fn test_base32_encode() {
        // Test vector from RFC 4648
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "my");
        assert_eq!(base32_encode(b"fo"), "mzxq");
        assert_eq!(base32_encode(b"foo"), "mzxw6");
        assert_eq!(base32_encode(b"foob"), "mzxw6yq");
        assert_eq!(base32_encode(b"fooba"), "mzxw6ytb");
        assert_eq!(base32_encode(b"foobar"), "mzxw6ytboi");
    }
}
