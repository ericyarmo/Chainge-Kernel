//! Canonical CBOR encoding for deterministic serialization.
//!
//! Receipts are encoded as CBOR (RFC 8949) with deterministic rules:
//! - Map keys: String keys, sorted by CBOR-encoded bytes
//! - Integers: Smallest valid encoding
//! - Lengths: Definite only
//! - refs: Always present (empty array if none), sorted, no duplicates
//!
//! **CRITICAL**: This encoding is FROZEN. Changes break all existing signatures.

use ciborium::value::Value;

use crate::crypto::{Author, Signature};
use crate::error::{Error, Result};
use crate::receipt::ReceiptId;

/// Domain separation prefix for signing.
pub const SIGN_DOMAIN: &[u8] = b"chainge/receipt-sig/v1";

/// Domain separation prefix for receipt ID.
pub const ID_DOMAIN: &[u8] = b"chainge/receipt-id/v1";

/// CBOR map key names.
mod keys {
    pub const AUTHOR: &str = "author";
    pub const PAYLOAD: &str = "payload";
    pub const REFS: &str = "refs";
    pub const SCHEMA: &str = "schema";
    pub const SIGNATURE: &str = "signature";
}

/// Encode receipt content to canonical CBOR bytes (for signing).
///
/// This encodes: { "author": bytes, "payload": bytes, "refs": [...], "schema": text }
/// Keys are sorted by CBOR-encoded bytes (RFC 8949 canonical).
/// refs is ALWAYS present (empty array if none).
pub fn canonical_content(
    author: &Author,
    schema: &str,
    refs: &[ReceiptId],
    payload: &[u8],
) -> Vec<u8> {
    // Build entries - refs is always present
    let entries = vec![
        (
            Value::Text(keys::REFS.to_string()),
            Value::Array(refs.iter().map(|r| Value::Bytes(r.0.to_vec())).collect()),
        ),
        (
            Value::Text(keys::AUTHOR.to_string()),
            Value::Bytes(author.0.to_vec()),
        ),
        (
            Value::Text(keys::SCHEMA.to_string()),
            Value::Text(schema.to_string()),
        ),
        (
            Value::Text(keys::PAYLOAD.to_string()),
            Value::Bytes(payload.to_vec()),
        ),
    ];

    let value = Value::Map(entries);
    encode_cbor_canonical(&value)
}

/// Build the message to sign (with domain separation).
pub fn sign_message(content_bytes: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(SIGN_DOMAIN.len() + content_bytes.len());
    msg.extend_from_slice(SIGN_DOMAIN);
    msg.extend_from_slice(content_bytes);
    msg
}

/// Encode full receipt to canonical CBOR bytes.
///
/// This is a VALID CBOR document containing all 5 fields including signature.
pub fn canonical_receipt(
    author: &Author,
    schema: &str,
    refs: &[ReceiptId],
    payload: &[u8],
    signature: &Signature,
) -> Vec<u8> {
    // Build entries - all 5 fields, sorted by CBOR key encoding
    let entries = vec![
        (
            Value::Text(keys::REFS.to_string()),
            Value::Array(refs.iter().map(|r| Value::Bytes(r.0.to_vec())).collect()),
        ),
        (
            Value::Text(keys::AUTHOR.to_string()),
            Value::Bytes(author.0.to_vec()),
        ),
        (
            Value::Text(keys::SCHEMA.to_string()),
            Value::Text(schema.to_string()),
        ),
        (
            Value::Text(keys::PAYLOAD.to_string()),
            Value::Bytes(payload.to_vec()),
        ),
        (
            Value::Text(keys::SIGNATURE.to_string()),
            Value::Bytes(signature.0.to_vec()),
        ),
    ];

    let value = Value::Map(entries);
    encode_cbor_canonical(&value)
}

/// Decode receipt from canonical CBOR bytes.
pub fn decode_receipt(
    bytes: &[u8],
) -> Result<(Author, String, Vec<ReceiptId>, Vec<u8>, Signature)> {
    // Parse CBOR
    let cursor = std::io::Cursor::new(bytes);
    let value: Value =
        ciborium::from_reader(cursor).map_err(|e| Error::DecodingError(e.to_string()))?;

    let map = match &value {
        Value::Map(m) => m,
        _ => return Err(Error::MalformedReceipt("expected map".into())),
    };

    // Helper to get value by string key
    let get = |key: &str| -> Option<&Value> {
        map.iter()
            .find(|(k, _)| matches!(k, Value::Text(s) if s == key))
            .map(|(_, v)| v)
    };

    // Parse author
    let author = match get(keys::AUTHOR) {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(b);
            Author::from_bytes(arr)
        }
        _ => return Err(Error::MalformedReceipt("invalid author".into())),
    };

    // Parse schema
    let schema = match get(keys::SCHEMA) {
        Some(Value::Text(s)) => s.clone(),
        _ => return Err(Error::MalformedReceipt("invalid schema".into())),
    };

    // Parse refs (must be present, may be empty)
    let refs = match get(keys::REFS) {
        Some(Value::Array(arr)) => {
            let mut refs = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    Value::Bytes(b) if b.len() == 32 => {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(b);
                        refs.push(ReceiptId::from_bytes(arr));
                    }
                    _ => return Err(Error::MalformedReceipt("invalid ref".into())),
                }
            }
            refs
        }
        _ => return Err(Error::MalformedReceipt("missing or invalid refs".into())),
    };

    // Parse payload
    let payload = match get(keys::PAYLOAD) {
        Some(Value::Bytes(b)) => b.clone(),
        _ => return Err(Error::MalformedReceipt("invalid payload".into())),
    };

    // Parse signature
    let signature = match get(keys::SIGNATURE) {
        Some(Value::Bytes(b)) if b.len() == 64 => {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(b);
            Signature::from_bytes(arr)
        }
        _ => return Err(Error::MalformedReceipt("invalid signature".into())),
    };

    Ok((author, schema, refs, payload, signature))
}

/// Encode a CBOR value to canonical bytes.
fn encode_cbor_canonical(value: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_value(&mut buf, value);
    buf
}

/// Recursively encode a CBOR value.
fn encode_value(buf: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Integer(i) => encode_integer(buf, *i),
        Value::Bytes(b) => encode_bytes(buf, b),
        Value::Text(s) => encode_text(buf, s),
        Value::Array(arr) => encode_array(buf, arr),
        Value::Map(entries) => encode_map(buf, entries),
        Value::Bool(b) => buf.push(if *b { 0xf5 } else { 0xf4 }),
        Value::Null => buf.push(0xf6),
        _ => panic!("unsupported CBOR value type"),
    }
}

fn encode_integer(buf: &mut Vec<u8>, i: ciborium::value::Integer) {
    let n: i128 = i.into();
    if n >= 0 {
        encode_uint(buf, 0, n as u64);
    } else {
        encode_uint(buf, 1, (-1 - n) as u64);
    }
}

fn encode_uint(buf: &mut Vec<u8>, major: u8, n: u64) {
    let mt = major << 5;
    if n < 24 {
        buf.push(mt | (n as u8));
    } else if n <= 0xff {
        buf.push(mt | 24);
        buf.push(n as u8);
    } else if n <= 0xffff {
        buf.push(mt | 25);
        buf.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffffffff {
        buf.push(mt | 26);
        buf.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        buf.push(mt | 27);
        buf.extend_from_slice(&n.to_be_bytes());
    }
}

fn encode_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    encode_uint(buf, 2, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}

fn encode_text(buf: &mut Vec<u8>, s: &str) {
    encode_uint(buf, 3, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

fn encode_array(buf: &mut Vec<u8>, arr: &[Value]) {
    encode_uint(buf, 4, arr.len() as u64);
    for item in arr {
        encode_value(buf, item);
    }
}

fn encode_map(buf: &mut Vec<u8>, entries: &[(Value, Value)]) {
    // RFC 8949 canonical: sort keys by CBOR-encoded bytes
    let mut sorted: Vec<_> = entries
        .iter()
        .map(|(k, v)| {
            let mut key_bytes = Vec::new();
            encode_value(&mut key_bytes, k);
            (key_bytes, k, v)
        })
        .collect();

    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    encode_uint(buf, 5, sorted.len() as u64);
    for (_, k, v) in sorted {
        encode_value(buf, k);
        encode_value(buf, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    #[test]
    fn test_canonical_deterministic() {
        let author = Keypair::from_seed(&[0x42; 32]).author();
        let schema = "test/v1";
        let refs: Vec<ReceiptId> = vec![];
        let payload = b"hello";

        let b1 = canonical_content(&author, schema, &refs, payload);
        let b2 = canonical_content(&author, schema, &refs, payload);
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_roundtrip() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let author = keypair.author();
        let schema = "test/v1".to_string();
        let refs: Vec<ReceiptId> = vec![ReceiptId::from_bytes([0xab; 32])];
        let payload = b"hello world".to_vec();

        let content = canonical_content(&author, &schema, &refs, &payload);
        let sign_msg = sign_message(&content);
        let signature = keypair.sign(&sign_msg);
        let bytes = canonical_receipt(&author, &schema, &refs, &payload, &signature);

        let (dec_author, dec_schema, dec_refs, dec_payload, dec_sig) =
            decode_receipt(&bytes).unwrap();

        assert_eq!(author, dec_author);
        assert_eq!(schema, dec_schema);
        assert_eq!(refs, dec_refs);
        assert_eq!(payload, dec_payload);
        assert_eq!(signature, dec_sig);
    }

    #[test]
    fn test_roundtrip_empty_refs() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let author = keypair.author();
        let schema = "test/v1".to_string();
        let refs: Vec<ReceiptId> = vec![]; // Empty refs - still present in CBOR
        let payload = b"hello".to_vec();

        let content = canonical_content(&author, &schema, &refs, &payload);
        let sign_msg = sign_message(&content);
        let signature = keypair.sign(&sign_msg);
        let bytes = canonical_receipt(&author, &schema, &refs, &payload, &signature);

        let (dec_author, dec_schema, dec_refs, dec_payload, dec_sig) =
            decode_receipt(&bytes).unwrap();

        assert_eq!(author, dec_author);
        assert_eq!(schema, dec_schema);
        assert!(dec_refs.is_empty());
        assert_eq!(payload, dec_payload);
        assert_eq!(signature, dec_sig);
    }

    #[test]
    fn test_content_key_ordering() {
        // Content keys sorted by CBOR-encoded bytes (length prefix + UTF-8)
        // "refs" (4) < "author" (6) < "schema" (6, but 's' > 'a') < "payload" (7)
        let author = Keypair::from_seed(&[0x42; 32]).author();
        let schema = "test/v1";
        let refs = vec![ReceiptId::from_bytes([0xab; 32])];
        let payload = b"hello";

        let bytes = canonical_content(&author, schema, &refs, payload);

        // Parse and verify key order
        let cursor = std::io::Cursor::new(&bytes);
        let value: Value = ciborium::from_reader(cursor).unwrap();

        if let Value::Map(entries) = value {
            let keys: Vec<_> = entries
                .iter()
                .filter_map(|(k, _)| {
                    if let Value::Text(s) = k {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            // CBOR canonical: sorted by encoded bytes (length prefix first)
            // 0x64 "refs" < 0x66 "author" < 0x66 "schema" < 0x67 "payload"
            assert_eq!(keys, vec!["refs", "author", "schema", "payload"]);
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn test_receipt_key_ordering() {
        // Full receipt has 5 keys including signature
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let author = keypair.author();
        let schema = "test/v1";
        let refs = vec![];
        let payload = b"hello";

        let content = canonical_content(&author, schema, &refs, payload);
        let sign_msg = sign_message(&content);
        let signature = keypair.sign(&sign_msg);
        let bytes = canonical_receipt(&author, schema, &refs, payload, &signature);

        // Parse and verify key order
        let cursor = std::io::Cursor::new(&bytes);
        let value: Value = ciborium::from_reader(cursor).unwrap();

        if let Value::Map(entries) = value {
            let keys: Vec<_> = entries
                .iter()
                .filter_map(|(k, _)| {
                    if let Value::Text(s) = k {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            // "refs"(4) < "author"(6) < "schema"(6) < "payload"(7) < "signature"(9)
            assert_eq!(
                keys,
                vec!["refs", "author", "schema", "payload", "signature"]
            );
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn test_receipt_is_valid_cbor() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let author = keypair.author();
        let content = canonical_content(&author, "test/v1", &[], b"hello");
        let sign_msg = sign_message(&content);
        let signature = keypair.sign(&sign_msg);
        let bytes = canonical_receipt(&author, "test/v1", &[], b"hello", &signature);

        // Should parse as valid CBOR
        let cursor = std::io::Cursor::new(&bytes);
        let result: std::result::Result<Value, _> = ciborium::from_reader(cursor);
        assert!(result.is_ok(), "receipt_bytes must be valid CBOR");
    }

    #[test]
    fn test_domain_separation_changes_signature() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let author = keypair.author();
        let content = canonical_content(&author, "test/v1", &[], b"hello");

        // With domain separation
        let sign_msg = sign_message(&content);
        let sig_with_domain = keypair.sign(&sign_msg);

        // Without domain separation
        let sig_without_domain = keypair.sign(&content);

        // Must be different
        assert_ne!(
            sig_with_domain, sig_without_domain,
            "domain separation must change signature"
        );
    }
}
