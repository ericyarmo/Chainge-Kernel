//! Canonical CBOR encoding for deterministic serialization.
//!
//! This module implements RFC 8949 Core Deterministic Encoding:
//! - Map keys sorted by encoded byte comparison
//! - Integers use smallest valid encoding
//! - Definite lengths only
//! - No floats (timestamps are i64 milliseconds)
//!
//! The canonical encoding is critical: it ensures that the same receipt
//! produces identical bytes (and thus identical hashes) across all platforms.

use ciborium::value::Value;
use std::io::Write;

use crate::crypto::{Blake3Hash, Ed25519PublicKey, Ed25519Signature};
use crate::error::CoreError;
use crate::receipt::{Receipt, ReceiptHeader, ReceiptKind};
use crate::stream::StreamId;
use crate::types::ReceiptId;

/// Header field keys (integer keys for compact encoding).
///
/// Keys 0-23 encode as single bytes in CBOR.
mod keys {
    pub const VERSION: u64 = 0;
    pub const AUTHOR: u64 = 1;
    pub const STREAM_ID: u64 = 2;
    pub const SEQ: u64 = 3;
    pub const TIMESTAMP: u64 = 4;
    pub const KIND: u64 = 5;
    pub const PREV_RECEIPT_ID: u64 = 6;
    pub const REFS: u64 = 7;
    pub const PAYLOAD_HASH: u64 = 8;
}

/// Encode a receipt header to canonical CBOR bytes.
pub fn canonical_header_bytes(header: &ReceiptHeader) -> Vec<u8> {
    let value = header_to_cbor_value(header);
    encode_cbor_value_canonical(&value)
}

/// Encode an entire receipt to canonical bytes.
///
/// Format: canonical_header || payload || signature
pub fn canonical_bytes(receipt: &Receipt) -> Vec<u8> {
    let mut buf = canonical_header_bytes(&receipt.header);
    buf.extend_from_slice(&receipt.payload);
    buf.extend_from_slice(&receipt.signature.0);
    buf
}

/// Construct the signed message (header || payload).
pub fn signed_message(receipt: &Receipt) -> Vec<u8> {
    let mut buf = canonical_header_bytes(&receipt.header);
    buf.extend_from_slice(&receipt.payload);
    buf
}

/// Construct the signed message from header and payload.
pub fn signed_message_from_parts(header: &ReceiptHeader, payload: &[u8]) -> Vec<u8> {
    let mut buf = canonical_header_bytes(header);
    buf.extend_from_slice(payload);
    buf
}

/// Convert a header to a CBOR Value (map with integer keys).
fn header_to_cbor_value(header: &ReceiptHeader) -> Value {
    // Build map entries in key order (already sorted 0-8)
    let mut entries = Vec::with_capacity(9);

    // 0: version
    entries.push((Value::Integer(keys::VERSION.into()), Value::Integer(header.version.into())));

    // 1: author
    entries.push((
        Value::Integer(keys::AUTHOR.into()),
        Value::Bytes(header.author.0.to_vec()),
    ));

    // 2: stream_id
    entries.push((
        Value::Integer(keys::STREAM_ID.into()),
        Value::Bytes(header.stream_id.0.to_vec()),
    ));

    // 3: seq
    entries.push((Value::Integer(keys::SEQ.into()), Value::Integer(header.seq.into())));

    // 4: timestamp
    entries.push((
        Value::Integer(keys::TIMESTAMP.into()),
        Value::Integer(header.timestamp.into()),
    ));

    // 5: kind
    entries.push((
        Value::Integer(keys::KIND.into()),
        Value::Integer(header.kind.to_u16().into()),
    ));

    // 6: prev_receipt_id (null or bytes)
    let prev_value = match &header.prev_receipt_id {
        Some(id) => Value::Bytes(id.0.to_vec()),
        None => Value::Null,
    };
    entries.push((Value::Integer(keys::PREV_RECEIPT_ID.into()), prev_value));

    // 7: refs (array of bytes)
    let refs_array: Vec<Value> = header.refs.iter().map(|r| Value::Bytes(r.0.to_vec())).collect();
    entries.push((Value::Integer(keys::REFS.into()), Value::Array(refs_array)));

    // 8: payload_hash
    entries.push((
        Value::Integer(keys::PAYLOAD_HASH.into()),
        Value::Bytes(header.payload_hash.0.to_vec()),
    ));

    Value::Map(entries)
}

/// Encode a CBOR Value to canonical bytes.
///
/// This function ensures:
/// - Map keys are sorted by encoded byte comparison
/// - Integers use smallest encoding
/// - Definite lengths only
fn encode_cbor_value_canonical(value: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_value_to(&mut buf, value);
    buf
}

/// Recursively encode a CBOR value.
fn encode_value_to(buf: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Integer(i) => {
            encode_integer(buf, *i);
        }
        Value::Bytes(b) => {
            encode_bytes(buf, b);
        }
        Value::Text(s) => {
            encode_text(buf, s);
        }
        Value::Array(arr) => {
            encode_array(buf, arr);
        }
        Value::Map(entries) => {
            encode_map_canonical(buf, entries);
        }
        Value::Bool(b) => {
            buf.push(if *b { 0xf5 } else { 0xf4 });
        }
        Value::Null => {
            buf.push(0xf6);
        }
        Value::Float(_) => {
            panic!("floats not supported in canonical encoding");
        }
        _ => {
            panic!("unsupported CBOR value type");
        }
    }
}

/// Encode a CBOR integer (major types 0 and 1).
fn encode_integer(buf: &mut Vec<u8>, i: ciborium::value::Integer) {
    let n: i128 = i.into();

    if n >= 0 {
        // Major type 0: unsigned integer
        encode_uint(buf, 0, n as u64);
    } else {
        // Major type 1: negative integer
        // CBOR encodes -1 as 0, -2 as 1, etc.
        let abs = (-1 - n) as u64;
        encode_uint(buf, 1, abs);
    }
}

/// Encode an unsigned integer with the given major type.
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

/// Encode a byte string (major type 2).
fn encode_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    encode_uint(buf, 2, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}

/// Encode a text string (major type 3).
fn encode_text(buf: &mut Vec<u8>, s: &str) {
    encode_uint(buf, 3, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

/// Encode an array (major type 4).
fn encode_array(buf: &mut Vec<u8>, arr: &[Value]) {
    encode_uint(buf, 4, arr.len() as u64);
    for item in arr {
        encode_value_to(buf, item);
    }
}

/// Encode a map canonically (major type 5).
///
/// Keys are sorted by their encoded byte comparison.
fn encode_map_canonical(buf: &mut Vec<u8>, entries: &[(Value, Value)]) {
    // Encode all keys first to sort by encoded bytes
    let mut key_value_pairs: Vec<(Vec<u8>, &Value)> = entries
        .iter()
        .map(|(k, v)| {
            let mut key_buf = Vec::new();
            encode_value_to(&mut key_buf, k);
            (key_buf, v)
        })
        .collect();

    // Sort by encoded key bytes (lexicographic)
    key_value_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Write map header
    encode_uint(buf, 5, key_value_pairs.len() as u64);

    // Write sorted key-value pairs
    for (key_bytes, value) in key_value_pairs {
        buf.extend_from_slice(&key_bytes);
        encode_value_to(buf, value);
    }
}

/// Decode a receipt from canonical bytes.
pub fn decode_receipt(bytes: &[u8]) -> Result<Receipt, CoreError> {
    // Minimum size: header (variable) + 64 byte signature
    if bytes.len() < 64 {
        return Err(CoreError::MalformedReceipt("too short".into()));
    }

    // Parse CBOR header
    let cursor = std::io::Cursor::new(bytes);
    let value: Value =
        ciborium::from_reader(cursor).map_err(|e| CoreError::DecodingError(e.to_string()))?;

    let header = cbor_value_to_header(&value)?;

    // Calculate header length by re-encoding
    let header_bytes = canonical_header_bytes(&header);
    let header_len = header_bytes.len();

    // Extract payload and signature
    let remaining = &bytes[header_len..];
    if remaining.len() < 64 {
        return Err(CoreError::MalformedReceipt(
            "insufficient bytes for signature".into(),
        ));
    }

    // Payload is everything except the last 64 bytes
    let payload_len = remaining.len() - 64;
    let payload = remaining[..payload_len].to_vec();
    let sig_bytes: [u8; 64] = remaining[payload_len..]
        .try_into()
        .map_err(|_| CoreError::MalformedReceipt("invalid signature length".into()))?;

    Ok(Receipt {
        header,
        payload: payload.into(),
        signature: Ed25519Signature(sig_bytes),
    })
}

/// Convert a CBOR Value (map) back to a ReceiptHeader.
fn cbor_value_to_header(value: &Value) -> Result<ReceiptHeader, CoreError> {
    let map = match value {
        Value::Map(m) => m,
        _ => return Err(CoreError::MalformedReceipt("expected map".into())),
    };

    // Helper to get a value by integer key
    let get = |key: u64| -> Option<&Value> {
        map.iter()
            .find(|(k, _)| matches!(k, Value::Integer(i) if (*i).into(): i128 == key as i128))
            .map(|(_, v)| v)
    };

    // Parse version
    let version = match get(keys::VERSION) {
        Some(Value::Integer(i)) => {
            let n: i128 = (*i).into();
            n as u8
        }
        _ => return Err(CoreError::MalformedReceipt("missing version".into())),
    };

    // Parse author
    let author = match get(keys::AUTHOR) {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(b);
            Ed25519PublicKey(arr)
        }
        _ => return Err(CoreError::MalformedReceipt("invalid author".into())),
    };

    // Parse stream_id
    let stream_id = match get(keys::STREAM_ID) {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(b);
            StreamId(arr)
        }
        _ => return Err(CoreError::MalformedReceipt("invalid stream_id".into())),
    };

    // Parse seq
    let seq = match get(keys::SEQ) {
        Some(Value::Integer(i)) => {
            let n: i128 = (*i).into();
            n as u64
        }
        _ => return Err(CoreError::MalformedReceipt("missing seq".into())),
    };

    // Parse timestamp
    let timestamp = match get(keys::TIMESTAMP) {
        Some(Value::Integer(i)) => {
            let n: i128 = (*i).into();
            n as i64
        }
        _ => return Err(CoreError::MalformedReceipt("missing timestamp".into())),
    };

    // Parse kind
    let kind = match get(keys::KIND) {
        Some(Value::Integer(i)) => {
            let n: i128 = (*i).into();
            ReceiptKind::from_u16(n as u16)
                .ok_or_else(|| CoreError::MalformedReceipt(format!("invalid kind: {}", n)))?
        }
        _ => return Err(CoreError::MalformedReceipt("missing kind".into())),
    };

    // Parse prev_receipt_id
    let prev_receipt_id = match get(keys::PREV_RECEIPT_ID) {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(b);
            Some(ReceiptId(arr))
        }
        Some(Value::Null) => None,
        None => None,
        _ => return Err(CoreError::MalformedReceipt("invalid prev_receipt_id".into())),
    };

    // Parse refs
    let refs = match get(keys::REFS) {
        Some(Value::Array(arr)) => {
            let mut refs = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    Value::Bytes(b) if b.len() == 32 => {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(b);
                        refs.push(ReceiptId(arr));
                    }
                    _ => {
                        return Err(CoreError::MalformedReceipt("invalid ref".into()));
                    }
                }
            }
            refs
        }
        None => Vec::new(),
        _ => return Err(CoreError::MalformedReceipt("invalid refs".into())),
    };

    // Parse payload_hash
    let payload_hash = match get(keys::PAYLOAD_HASH) {
        Some(Value::Bytes(b)) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(b);
            Blake3Hash(arr)
        }
        _ => return Err(CoreError::MalformedReceipt("invalid payload_hash".into())),
    };

    Ok(ReceiptHeader {
        version,
        author,
        stream_id,
        seq,
        timestamp,
        kind,
        prev_receipt_id,
        refs,
        payload_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;
    use crate::receipt::ReceiptBuilder;

    #[test]
    fn test_canonical_encoding_deterministic() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let bytes1 = canonical_bytes(&receipt);
        let bytes2 = canonical_bytes(&receipt);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_canonical_header_deterministic() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let h1 = canonical_header_bytes(&receipt.header);
        let h2 = canonical_header_bytes(&receipt.header);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_integer_encoding() {
        // Test smallest encoding for various integer sizes
        let mut buf = Vec::new();

        // 0-23: single byte
        encode_uint(&mut buf, 0, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        encode_uint(&mut buf, 0, 23);
        assert_eq!(buf, vec![0x17]);

        // 24-255: two bytes
        buf.clear();
        encode_uint(&mut buf, 0, 24);
        assert_eq!(buf, vec![0x18, 24]);

        buf.clear();
        encode_uint(&mut buf, 0, 255);
        assert_eq!(buf, vec![0x18, 255]);

        // 256-65535: three bytes
        buf.clear();
        encode_uint(&mut buf, 0, 256);
        assert_eq!(buf, vec![0x19, 0x01, 0x00]);

        buf.clear();
        encode_uint(&mut buf, 0, 65535);
        assert_eq!(buf, vec![0x19, 0xff, 0xff]);
    }

    #[test]
    fn test_receipt_roundtrip() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello world".to_vec())
            .sign(&keypair);

        let bytes = canonical_bytes(&receipt);
        let decoded = decode_receipt(&bytes).unwrap();

        assert_eq!(receipt.header.version, decoded.header.version);
        assert_eq!(receipt.header.author, decoded.header.author);
        assert_eq!(receipt.header.stream_id, decoded.header.stream_id);
        assert_eq!(receipt.header.seq, decoded.header.seq);
        assert_eq!(receipt.header.timestamp, decoded.header.timestamp);
        assert_eq!(receipt.header.kind, decoded.header.kind);
        assert_eq!(receipt.header.prev_receipt_id, decoded.header.prev_receipt_id);
        assert_eq!(receipt.header.refs, decoded.header.refs);
        assert_eq!(receipt.header.payload_hash, decoded.header.payload_hash);
        assert_eq!(receipt.payload, decoded.payload);
        assert_eq!(receipt.signature, decoded.signature);
    }

    #[test]
    fn test_receipt_id_from_canonical_bytes() {
        let keypair = Keypair::from_seed(&[0x42; 32]);
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let receipt = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1736870400000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"hello".to_vec())
            .sign(&keypair);

        let id1 = receipt.compute_id();

        // Compute ID manually from canonical bytes
        let bytes = canonical_bytes(&receipt);
        let id2 = ReceiptId(Blake3Hash::hash(&bytes).0);

        assert_eq!(id1, id2);
    }

    #[test]
    fn test_map_key_ordering() {
        // Ensure integer keys are sorted correctly
        let mut buf = Vec::new();
        let entries = vec![
            (Value::Integer(8.into()), Value::Integer(80.into())),
            (Value::Integer(0.into()), Value::Integer(0.into())),
            (Value::Integer(5.into()), Value::Integer(50.into())),
        ];
        encode_map_canonical(&mut buf, &entries);

        // Map header (3 entries)
        assert_eq!(buf[0], 0xa3);
        // Keys should be in order: 0, 5, 8
        assert_eq!(buf[1], 0x00); // key 0
        assert_eq!(buf[2], 0x00); // value 0
        assert_eq!(buf[3], 0x05); // key 5
        assert_eq!(buf[4], 0x18); // value 50 (>23)
        assert_eq!(buf[5], 50);
        assert_eq!(buf[6], 0x08); // key 8
        assert_eq!(buf[7], 0x18); // value 80 (>23)
        assert_eq!(buf[8], 80);
    }
}
