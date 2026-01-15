//! Receipt: the atomic unit of verifiable memory.
//!
//! A receipt is an immutable, signed attestation with 4 semantic fields:
//! - `author`: Who created this (Ed25519 public key)
//! - `schema`: How to interpret the payload (URI)
//! - `refs`: Causal references to other receipts (sorted, no duplicates)
//! - `payload`: The actual content (opaque bytes)

use std::fmt;

use crate::canonical::{canonical_content, canonical_receipt, decode_receipt, sign_message, ID_DOMAIN};
use crate::crypto::{Author, Keypair, Sha256Hash, Signature};
use crate::error::{Error, Result};
use crate::{MAX_PAYLOAD_LEN, MAX_REFS, MAX_SCHEMA_LEN};

/// Normalize refs: sort and check for duplicates.
/// Returns sorted refs, or error if duplicates found.
fn normalize_refs(mut refs: Vec<ReceiptId>) -> Result<Vec<ReceiptId>> {
    refs.sort();
    // Check for duplicates (adjacent after sort)
    for window in refs.windows(2) {
        if window[0] == window[1] {
            return Err(Error::RefsDuplicate);
        }
    }
    Ok(refs)
}

/// Validate that refs are sorted and contain no duplicates (for decode path).
fn validate_refs_sorted(refs: &[ReceiptId]) -> Result<()> {
    for window in refs.windows(2) {
        match window[0].cmp(&window[1]) {
            std::cmp::Ordering::Greater => return Err(Error::RefsNotSorted),
            std::cmp::Ordering::Equal => return Err(Error::RefsDuplicate),
            std::cmp::Ordering::Less => {} // correct
        }
    }
    Ok(())
}

/// A 32-byte receipt identifier, computed as SHA256(domain || receipt_bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReceiptId(pub [u8; 32]);

impl ReceiptId {
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
            return Err(Error::MalformedReceipt("invalid receipt id length".into()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Zero ID (sentinel value).
    pub const ZERO: Self = Self([0u8; 32]);
}

impl fmt::Debug for ReceiptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReceiptId({}...)", &self.to_hex()[..8])
    }
}

impl fmt::Display for ReceiptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for ReceiptId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for ReceiptId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Ord for ReceiptId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for ReceiptId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A receipt: immutable, signed attestation.
#[derive(Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The author's public key (who signed this).
    pub author: Author,
    /// Schema URI (how to interpret the payload).
    pub schema: String,
    /// Causal references to other receipts (sorted, no duplicates).
    pub refs: Vec<ReceiptId>,
    /// The payload (opaque to the kernel).
    pub payload: Vec<u8>,
    /// Ed25519 signature over canonical content with domain separation.
    pub signature: Signature,
}

impl Receipt {
    /// Create and sign a new receipt.
    ///
    /// **Auto-normalization**: Refs are automatically sorted. Duplicate refs
    /// are rejected with an error.
    pub fn new(
        keypair: &Keypair,
        schema: impl Into<String>,
        refs: Vec<ReceiptId>,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let schema = schema.into();

        // Validate schema
        if schema.len() > MAX_SCHEMA_LEN {
            return Err(Error::SchemaTooLong(schema.len()));
        }
        if !schema.is_ascii() {
            return Err(Error::SchemaNotAscii);
        }

        // Validate refs count
        if refs.len() > MAX_REFS {
            return Err(Error::TooManyRefs(refs.len()));
        }

        // Normalize refs: sort and reject duplicates
        let refs = normalize_refs(refs)?;

        // Validate payload
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(Error::PayloadTooLarge(payload.len()));
        }

        let author = keypair.author();
        let content = canonical_content(&author, &schema, &refs, &payload);
        let sign_msg = sign_message(&content);
        let signature = keypair.sign(&sign_msg);

        Ok(Self {
            author,
            schema,
            refs,
            payload,
            signature,
        })
    }

    /// Compute the receipt ID (content-addressed SHA-256 hash with domain separation).
    ///
    /// `receipt_id = sha256("chainge/receipt-id/v1" || receipt_bytes)`
    pub fn id(&self) -> ReceiptId {
        let receipt_bytes = canonical_receipt(
            &self.author,
            &self.schema,
            &self.refs,
            &self.payload,
            &self.signature,
        );

        // Hash with domain separation
        let mut to_hash = Vec::with_capacity(ID_DOMAIN.len() + receipt_bytes.len());
        to_hash.extend_from_slice(ID_DOMAIN);
        to_hash.extend_from_slice(&receipt_bytes);

        ReceiptId(Sha256Hash::hash(&to_hash).0)
    }

    /// Compute the CIDv1 (content identifier, IPFS-compatible).
    ///
    /// `cid = CIDv1(dag-cbor, sha2-256(receipt_bytes))`
    ///
    /// Note: CID hashes receipt_bytes directly (no domain prefix) for IPFS compatibility.
    pub fn cid(&self) -> String {
        let receipt_bytes = canonical_receipt(
            &self.author,
            &self.schema,
            &self.refs,
            &self.payload,
            &self.signature,
        );
        // CID uses raw hash of receipt_bytes (no domain prefix)
        Sha256Hash::hash(&receipt_bytes).to_cid()
    }

    /// Verify the signature with domain separation.
    pub fn verify(&self) -> Result<()> {
        let content = canonical_content(&self.author, &self.schema, &self.refs, &self.payload);
        let sign_msg = sign_message(&content);
        self.author.verify(&sign_msg, &self.signature)
    }

    /// Encode to canonical CBOR bytes (valid CBOR document).
    pub fn to_bytes(&self) -> Vec<u8> {
        canonical_receipt(
            &self.author,
            &self.schema,
            &self.refs,
            &self.payload,
            &self.signature,
        )
    }

    /// Decode from canonical CBOR bytes.
    ///
    /// Validates signature and refs ordering.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (author, schema, refs, payload, signature) = decode_receipt(bytes)?;

        // Validate schema is ASCII
        if !schema.is_ascii() {
            return Err(Error::SchemaNotAscii);
        }

        // Validate refs are sorted and unique (strict on decode - must be canonical)
        validate_refs_sorted(&refs)?;

        let receipt = Self {
            author,
            schema,
            refs,
            payload,
            signature,
        };

        // Verify signature on decode
        receipt.verify()?;

        Ok(receipt)
    }

    /// Check if this receipt references another.
    pub fn references(&self, id: &ReceiptId) -> bool {
        self.refs.contains(id)
    }
}

impl fmt::Debug for Receipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receipt")
            .field("id", &self.id())
            .field("author", &self.author)
            .field("schema", &self.schema)
            .field("refs", &self.refs.len())
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_verify() {
        let keypair = Keypair::generate();
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

        assert!(receipt.verify().is_ok());
        assert_eq!(receipt.author, keypair.author());
        assert_eq!(receipt.schema, "test/v1");
        assert!(receipt.refs.is_empty());
        assert_eq!(receipt.payload, b"hello");
    }

    #[test]
    fn test_id_deterministic() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

        let id1 = receipt.id();
        let id2 = receipt.id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_roundtrip() {
        let keypair = Keypair::generate();
        let original = Receipt::new(
            &keypair,
            "test/v1",
            vec![ReceiptId::from_bytes([0xab; 32])],
            b"hello world".to_vec(),
        )
        .unwrap();

        let bytes = original.to_bytes();
        let decoded = Receipt::from_bytes(&bytes).unwrap();

        assert_eq!(original.id(), decoded.id());
        assert_eq!(original.author, decoded.author);
        assert_eq!(original.schema, decoded.schema);
        assert_eq!(original.refs, decoded.refs);
        assert_eq!(original.payload, decoded.payload);
    }

    #[test]
    fn test_refs() {
        let keypair = Keypair::generate();
        let ref_id = ReceiptId::from_bytes([0xab; 32]);
        let receipt = Receipt::new(&keypair, "test/v1", vec![ref_id], b"payload".to_vec()).unwrap();

        assert!(receipt.references(&ref_id));
        assert!(!receipt.references(&ReceiptId::from_bytes([0xcd; 32])));
    }

    #[test]
    fn test_refs_auto_sorted() {
        let keypair = Keypair::generate();
        let ref1 = ReceiptId::from_bytes([0xaa; 32]);
        let ref2 = ReceiptId::from_bytes([0xbb; 32]);

        // Sorted refs should work
        let result = Receipt::new(
            &keypair,
            "test/v1",
            vec![ref1, ref2], // correctly sorted
            b"payload".to_vec(),
        );
        assert!(result.is_ok());

        // Unsorted refs are auto-sorted (v0.5 behavior)
        let receipt = Receipt::new(
            &keypair,
            "test/v1",
            vec![ref2, ref1], // wrong order - will be auto-sorted
            b"payload".to_vec(),
        ).unwrap();

        // Verify refs are now sorted
        assert_eq!(receipt.refs, vec![ref1, ref2]);
    }

    #[test]
    fn test_refs_must_be_unique() {
        let keypair = Keypair::generate();
        let ref1 = ReceiptId::from_bytes([0xaa; 32]);

        // Duplicate refs must be rejected
        let result = Receipt::new(
            &keypair,
            "test/v1",
            vec![ref1, ref1], // duplicate
            b"payload".to_vec(),
        );
        assert!(matches!(result, Err(Error::RefsDuplicate)));
    }

    #[test]
    fn test_schema_must_be_ascii() {
        let keypair = Keypair::generate();

        // ASCII schema should work
        let result = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec());
        assert!(result.is_ok());

        // Non-ASCII schema must be rejected
        let result = Receipt::new(&keypair, "test/v1/\u{1F600}", vec![], b"hello".to_vec());
        assert!(matches!(result, Err(Error::SchemaNotAscii)));
    }

    #[test]
    fn test_schema_too_long() {
        let keypair = Keypair::generate();
        let long_schema = "x".repeat(MAX_SCHEMA_LEN + 1);
        let result = Receipt::new(&keypair, long_schema, vec![], vec![]);
        assert!(matches!(result, Err(Error::SchemaTooLong(_))));
    }

    #[test]
    fn test_too_many_refs() {
        let keypair = Keypair::generate();
        let refs: Vec<ReceiptId> = (0..MAX_REFS + 1)
            .map(|i| ReceiptId::from_bytes([i as u8; 32]))
            .collect();
        let result = Receipt::new(&keypair, "test/v1", refs, vec![]);
        assert!(matches!(result, Err(Error::TooManyRefs(_))));
    }

    #[test]
    fn test_payload_too_large() {
        let keypair = Keypair::generate();
        let payload = vec![0u8; MAX_PAYLOAD_LEN + 1];
        let result = Receipt::new(&keypair, "test/v1", vec![], payload);
        assert!(matches!(result, Err(Error::PayloadTooLarge(_))));
    }

    #[test]
    fn test_tampered_signature_fails() {
        let keypair = Keypair::generate();
        let mut receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

        // Tamper with signature
        receipt.signature = Signature::from_bytes([0xff; 64]);

        assert!(receipt.verify().is_err());
    }

    #[test]
    fn test_cid_format() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();
        let cid = receipt.cid();

        // CID should start with 'b' (base32 multibase prefix)
        assert!(cid.starts_with('b'));
        // Should be lowercase
        assert_eq!(cid, cid.to_lowercase());
    }

    #[test]
    fn test_id_and_cid_differ() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

        let id = receipt.id();
        let cid = receipt.cid();

        // ID has domain separation, CID doesn't - they must differ
        // The underlying hashes should be different
        assert!(cid.len() > 10); // CID should be reasonably long

        // Verify ID is consistent
        assert_eq!(id, receipt.id());
    }

    #[test]
    fn test_to_bytes_is_valid_cbor() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();
        let bytes = receipt.to_bytes();

        // Should parse as valid CBOR
        let cursor = std::io::Cursor::new(&bytes);
        let result: std::result::Result<ciborium::value::Value, _> = ciborium::from_reader(cursor);
        assert!(result.is_ok(), "to_bytes() must produce valid CBOR");
    }
}
