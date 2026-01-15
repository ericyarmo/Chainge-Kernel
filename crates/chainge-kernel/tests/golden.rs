//! Golden test vectors for cross-implementation verification.
//!
//! Every implementation of the Chainge Kernel must produce identical:
//! - content_bytes
//! - signed_message
//! - signature (deterministic Ed25519)
//! - receipt_bytes
//! - receipt_id
//! - cid

use chainge_kernel::{
    canonical_content, canonical_receipt, sign_message, Keypair, Receipt, ReceiptId,
    ID_DOMAIN, SIGN_DOMAIN,
};
use serde::{Deserialize, Serialize};

/// A single golden test vector.
#[derive(Debug, Serialize, Deserialize)]
pub struct GoldenVector {
    pub name: String,
    pub description: String,

    // Inputs
    pub author_seed: String,    // 32 bytes hex
    pub author_pk: String,      // 32 bytes hex (derived)
    pub schema: String,
    pub refs: Vec<String>,      // array of 32-byte hex
    pub payload: String,        // hex

    // Derived outputs (all hex except cid)
    pub content_bytes: String,
    pub signed_message: String, // SIGN_DOMAIN || content_bytes
    pub signature: String,      // 64 bytes
    pub receipt_bytes: String,
    pub receipt_id: String,     // 32 bytes
    pub cid: String,            // base32 string
}

/// Generate a golden vector from inputs.
fn generate_vector(
    name: &str,
    description: &str,
    seed: [u8; 32],
    schema: &str,
    refs: Vec<ReceiptId>,
    payload: &[u8],
) -> GoldenVector {
    let keypair = Keypair::from_seed(&seed);
    let author = keypair.author();

    let content_bytes = canonical_content(&author, schema, &refs, payload);
    let sign_msg = sign_message(&content_bytes);
    let signature = keypair.sign(&sign_msg);
    let receipt_bytes = canonical_receipt(&author, schema, &refs, payload, &signature);

    // Compute receipt_id with domain separation
    let mut id_input = Vec::new();
    id_input.extend_from_slice(ID_DOMAIN);
    id_input.extend_from_slice(&receipt_bytes);
    let receipt_id = chainge_kernel::Sha256Hash::hash(&id_input);

    // Compute CID (no domain separation)
    let cid_hash = chainge_kernel::Sha256Hash::hash(&receipt_bytes);

    GoldenVector {
        name: name.to_string(),
        description: description.to_string(),
        author_seed: hex::encode(seed),
        author_pk: hex::encode(author.0),
        schema: schema.to_string(),
        refs: refs.iter().map(|r| hex::encode(r.0)).collect(),
        payload: hex::encode(payload),
        content_bytes: hex::encode(&content_bytes),
        signed_message: hex::encode(&sign_msg),
        signature: hex::encode(signature.0),
        receipt_bytes: hex::encode(&receipt_bytes),
        receipt_id: hex::encode(receipt_id.0),
        cid: cid_hash.to_cid(),
    }
}

/// Generate all 10 golden vectors.
pub fn generate_all_vectors() -> Vec<GoldenVector> {
    vec![
        // Vector 1: Empty refs, empty payload
        generate_vector(
            "empty_refs_empty_payload",
            "Minimal receipt: no refs, no payload",
            [0x01; 32],
            "test/v1",
            vec![],
            &[],
        ),
        // Vector 2: Empty refs, non-empty payload
        generate_vector(
            "empty_refs_with_payload",
            "Receipt with payload but no refs",
            [0x02; 32],
            "civic.presence/v1",
            vec![],
            b"hello world",
        ),
        // Vector 3: One ref, non-empty payload
        generate_vector(
            "single_ref",
            "Receipt referencing one other receipt",
            [0x03; 32],
            "civic.countersign/v1",
            vec![ReceiptId::from_bytes([0xaa; 32])],
            b"{\"status\":\"confirmed\"}",
        ),
        // Vector 4: Two refs (sorted)
        generate_vector(
            "two_refs_sorted",
            "Receipt with two refs, properly sorted",
            [0x04; 32],
            "merge/v1",
            vec![
                ReceiptId::from_bytes([0x11; 32]), // smaller
                ReceiptId::from_bytes([0x22; 32]), // larger
            ],
            b"merged",
        ),
        // Vector 5: Many refs (8)
        generate_vector(
            "many_refs",
            "Receipt with 8 refs",
            [0x05; 32],
            "aggregate/v1",
            vec![
                ReceiptId::from_bytes([0x01; 32]),
                ReceiptId::from_bytes([0x02; 32]),
                ReceiptId::from_bytes([0x03; 32]),
                ReceiptId::from_bytes([0x04; 32]),
                ReceiptId::from_bytes([0x05; 32]),
                ReceiptId::from_bytes([0x06; 32]),
                ReceiptId::from_bytes([0x07; 32]),
                ReceiptId::from_bytes([0x08; 32]),
            ],
            b"aggregated",
        ),
        // Vector 6: Max schema length (256 bytes)
        generate_vector(
            "max_schema_length",
            "Schema at maximum allowed length (256 bytes)",
            [0x06; 32],
            &"x".repeat(256),
            vec![],
            b"max schema test",
        ),
        // Vector 7: Large payload (1KB)
        generate_vector(
            "large_payload",
            "Receipt with 1KB payload",
            [0x07; 32],
            "data/v1",
            vec![],
            &vec![0x42u8; 1024],
        ),
        // Vector 8: Binary payload (all byte values)
        generate_vector(
            "binary_payload",
            "Payload containing all 256 byte values",
            [0x08; 32],
            "binary/v1",
            vec![],
            &(0u8..=255).collect::<Vec<u8>>(),
        ),
        // Vector 9: Realistic civic payload
        generate_vector(
            "realistic_civic",
            "Realistic civic inspection payload (JSON)",
            [0x09; 32],
            "civic.inspection/v1",
            vec![],
            br#"{"entity":"Clayton High School","score":90,"violations":5,"timestamp_ms":1736870400000}"#,
        ),
        // Vector 10: Chain of receipts (B refs A)
        {
            // First create A
            let seed_a = [0x0a; 32];
            let keypair_a = Keypair::from_seed(&seed_a);
            let receipt_a = Receipt::new(&keypair_a, "chain/v1", vec![], b"first".to_vec()).unwrap();
            let id_a = receipt_a.id();

            // Then create B referencing A
            generate_vector(
                "chain_receipt",
                "Receipt B referencing receipt A (chain)",
                [0x0b; 32],
                "chain/v1",
                vec![id_a],
                b"second",
            )
        },
    ]
}

#[test]
fn test_generate_vectors() {
    let vectors = generate_all_vectors();
    assert_eq!(vectors.len(), 10);

    // Print vectors for inspection
    for v in &vectors {
        println!("=== {} ===", v.name);
        println!("  description: {}", v.description);
        println!("  author_pk: {}", v.author_pk);
        println!("  receipt_id: {}", v.receipt_id);
        println!("  cid: {}", v.cid);
        println!();
    }
}

#[test]
fn test_vectors_deterministic() {
    // Generate twice, must be identical
    let v1 = generate_all_vectors();
    let v2 = generate_all_vectors();

    for (a, b) in v1.iter().zip(v2.iter()) {
        assert_eq!(a.content_bytes, b.content_bytes, "content_bytes mismatch for {}", a.name);
        assert_eq!(a.signature, b.signature, "signature mismatch for {}", a.name);
        assert_eq!(a.receipt_bytes, b.receipt_bytes, "receipt_bytes mismatch for {}", a.name);
        assert_eq!(a.receipt_id, b.receipt_id, "receipt_id mismatch for {}", a.name);
        assert_eq!(a.cid, b.cid, "cid mismatch for {}", a.name);
    }
}

#[test]
fn test_vectors_verify() {
    // Ensure all generated receipts verify correctly
    let vectors = generate_all_vectors();

    for v in &vectors {
        let seed: [u8; 32] = hex::decode(&v.author_seed)
            .unwrap()
            .try_into()
            .unwrap();
        let keypair = Keypair::from_seed(&seed);

        let refs: Vec<ReceiptId> = v.refs
            .iter()
            .map(|r| {
                let bytes: [u8; 32] = hex::decode(r).unwrap().try_into().unwrap();
                ReceiptId::from_bytes(bytes)
            })
            .collect();

        let payload = hex::decode(&v.payload).unwrap();

        let receipt = Receipt::new(&keypair, &v.schema, refs, payload).unwrap();

        // Verify signature
        assert!(receipt.verify().is_ok(), "verify failed for {}", v.name);

        // Verify derived values match
        assert_eq!(
            hex::encode(receipt.to_bytes()),
            v.receipt_bytes,
            "receipt_bytes mismatch for {}",
            v.name
        );
        assert_eq!(
            receipt.id().to_hex(),
            v.receipt_id,
            "receipt_id mismatch for {}",
            v.name
        );
        assert_eq!(receipt.cid(), v.cid, "cid mismatch for {}", v.name);
    }
}

#[test]
fn print_golden_vectors_json() {
    let vectors = generate_all_vectors();

    #[derive(Serialize)]
    struct VectorFile {
        version: String,
        description: String,
        domain_sign: String,
        domain_id: String,
        vectors: Vec<GoldenVector>,
    }

    let file = VectorFile {
        version: "0.4.0".to_string(),
        description: "Golden test vectors for Chainge Kernel. Every implementation must produce identical outputs.".to_string(),
        domain_sign: String::from_utf8_lossy(SIGN_DOMAIN).to_string(),
        domain_id: String::from_utf8_lossy(ID_DOMAIN).to_string(),
        vectors,
    };

    let json = serde_json::to_string_pretty(&file).unwrap();
    println!("{}", json);
}

// =============================================================================
// REJECTION TEST VECTORS
// These test that invalid inputs are properly rejected.
// =============================================================================

#[test]
fn test_auto_sort_refs() {
    let keypair = Keypair::from_seed(&[0x42; 32]);
    let ref1 = ReceiptId::from_bytes([0xaa; 32]);
    let ref2 = ReceiptId::from_bytes([0xbb; 32]);

    // Wrong order: bb before aa - should be auto-sorted (v0.5 behavior)
    let receipt = Receipt::new(&keypair, "test/v1", vec![ref2, ref1], b"payload".to_vec())
        .expect("unsorted refs should be auto-sorted");

    // Verify refs are now sorted
    assert_eq!(receipt.refs, vec![ref1, ref2], "refs must be sorted after normalization");
}

#[test]
fn test_reject_duplicate_refs() {
    let keypair = Keypair::from_seed(&[0x42; 32]);
    let ref1 = ReceiptId::from_bytes([0xaa; 32]);

    // Duplicate ref
    let result = Receipt::new(&keypair, "test/v1", vec![ref1, ref1], b"payload".to_vec());
    assert!(
        matches!(result, Err(chainge_kernel::Error::RefsDuplicate)),
        "must reject duplicate refs"
    );
}

#[test]
fn test_reject_non_ascii_schema() {
    let keypair = Keypair::from_seed(&[0x42; 32]);

    // Unicode emoji in schema
    let result = Receipt::new(&keypair, "test/v1/\u{1F600}", vec![], b"payload".to_vec());
    assert!(
        matches!(result, Err(chainge_kernel::Error::SchemaNotAscii)),
        "must reject non-ASCII schema"
    );

    // Unicode character
    let result = Receipt::new(&keypair, "test/v1/\u{00E9}", vec![], b"payload".to_vec());
    assert!(
        matches!(result, Err(chainge_kernel::Error::SchemaNotAscii)),
        "must reject non-ASCII schema"
    );
}

#[test]
fn test_reject_schema_too_long() {
    let keypair = Keypair::from_seed(&[0x42; 32]);

    // 257 bytes (one over limit)
    let long_schema = "x".repeat(257);
    let result = Receipt::new(&keypair, long_schema, vec![], b"payload".to_vec());
    assert!(
        matches!(result, Err(chainge_kernel::Error::SchemaTooLong(_))),
        "must reject schema > 256 bytes"
    );
}

#[test]
fn test_reject_too_many_refs() {
    let keypair = Keypair::from_seed(&[0x42; 32]);

    // 129 refs (one over limit of 128)
    let refs: Vec<ReceiptId> = (0u8..=128)
        .map(|i| ReceiptId::from_bytes([i; 32]))
        .collect();
    let result = Receipt::new(&keypair, "test/v1", refs, b"payload".to_vec());
    assert!(
        matches!(result, Err(chainge_kernel::Error::TooManyRefs(_))),
        "must reject > 128 refs"
    );
}

#[test]
fn test_reject_payload_too_large() {
    let keypair = Keypair::from_seed(&[0x42; 32]);

    // 64KB + 1 byte
    let payload = vec![0u8; 65537];
    let result = Receipt::new(&keypair, "test/v1", vec![], payload);
    assert!(
        matches!(result, Err(chainge_kernel::Error::PayloadTooLarge(_))),
        "must reject payload > 64KB"
    );
}

#[test]
fn test_reject_invalid_signature() {
    let keypair = Keypair::from_seed(&[0x42; 32]);
    let mut receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

    // Tamper with signature
    receipt.signature = chainge_kernel::Signature::from_bytes([0xff; 64]);

    assert!(
        matches!(receipt.verify(), Err(chainge_kernel::Error::InvalidSignature)),
        "must reject invalid signature"
    );
}

#[test]
fn test_domain_prefix_exact_bytes() {
    // Verify domain prefixes are exactly as specified
    assert_eq!(SIGN_DOMAIN, b"chainge/receipt-sig/v1");
    assert_eq!(SIGN_DOMAIN.len(), 22);

    assert_eq!(ID_DOMAIN, b"chainge/receipt-id/v1");
    assert_eq!(ID_DOMAIN.len(), 21);

    // Verify they're raw ASCII bytes with no null terminator
    assert!(SIGN_DOMAIN.iter().all(|&b| b != 0));
    assert!(ID_DOMAIN.iter().all(|&b| b != 0));
}
