//! Proptest generators for property-based testing.

use proptest::prelude::*;

use chainge_kernel_core::{
    Blake3Hash, Ed25519PublicKey, Keypair, ReceiptBuilder, ReceiptId, ReceiptKind, StreamId,
};

/// Generate a random keypair.
pub fn keypair() -> impl Strategy<Value = Keypair> {
    any::<[u8; 32]>().prop_map(|seed| Keypair::from_seed(&seed))
}

/// Generate a random ReceiptId.
pub fn receipt_id() -> impl Strategy<Value = ReceiptId> {
    any::<[u8; 32]>().prop_map(ReceiptId::from_bytes)
}

/// Generate a random StreamId.
pub fn stream_id() -> impl Strategy<Value = StreamId> {
    any::<[u8; 32]>().prop_map(StreamId::from_bytes)
}

/// Generate a random Blake3Hash.
pub fn blake3_hash() -> impl Strategy<Value = Blake3Hash> {
    any::<[u8; 32]>().prop_map(Blake3Hash)
}

/// Generate a random Ed25519PublicKey.
pub fn public_key() -> impl Strategy<Value = Ed25519PublicKey> {
    keypair().prop_map(|kp| kp.public_key())
}

/// Generate a valid sequence number (1-indexed).
pub fn seq() -> impl Strategy<Value = u64> {
    1u64..=u64::MAX
}

/// Generate a reasonable timestamp.
pub fn timestamp() -> impl Strategy<Value = i64> {
    0i64..=i64::MAX / 2
}

/// Generate a ReceiptKind.
pub fn receipt_kind() -> impl Strategy<Value = ReceiptKind> {
    prop_oneof![
        Just(ReceiptKind::Data),
        Just(ReceiptKind::StreamInit),
        Just(ReceiptKind::Tombstone),
        Just(ReceiptKind::Grant),
        Just(ReceiptKind::Revoke),
        Just(ReceiptKind::KeyShare),
        Just(ReceiptKind::Anchor),
    ]
}

/// Generate payload bytes of specified max length.
pub fn payload(max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=max_len)
}

/// Generate a stream name.
pub fn stream_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,31}".prop_map(String::from)
}

/// Parameters for generating a receipt.
#[derive(Debug, Clone)]
pub struct ReceiptParams {
    pub keypair: Keypair,
    pub stream_name: String,
    pub seq: u64,
    pub kind: ReceiptKind,
    pub timestamp: i64,
    pub payload: Vec<u8>,
    pub prev_receipt_id: Option<ReceiptId>,
}

impl Arbitrary for ReceiptParams {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        (
            any::<[u8; 32]>(),  // seed
            stream_name(),
            1u64..=1000u64,     // seq
            receipt_kind(),
            0i64..=1_700_000_000_000i64,  // timestamp
            payload(1000),
            any::<Option<[u8; 32]>>(),
        )
            .prop_map(|(seed, name, seq, kind, ts, payload, prev)| ReceiptParams {
                keypair: Keypair::from_seed(&seed),
                stream_name: name,
                seq,
                kind,
                timestamp: ts,
                payload,
                prev_receipt_id: prev.map(ReceiptId::from_bytes),
            })
            .boxed()
    }
}

/// Generate a receipt from parameters.
pub fn receipt_from_params(params: &ReceiptParams) -> chainge_kernel_core::Receipt {
    let stream_id = StreamId::derive(&params.keypair.public_key(), &params.stream_name);

    let mut builder = ReceiptBuilder::new(params.keypair.public_key(), stream_id, params.seq)
        .kind(params.kind)
        .timestamp(params.timestamp)
        .payload(params.payload.clone());

    if let Some(prev) = params.prev_receipt_id {
        builder = builder.prev(prev);
    }

    builder.sign(&params.keypair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chainge_kernel_core::canonical_bytes;

    proptest! {
        #[test]
        fn test_receipt_id_deterministic(params: ReceiptParams) {
            let r1 = receipt_from_params(&params);
            let r2 = receipt_from_params(&params);

            prop_assert_eq!(r1.compute_id(), r2.compute_id());
        }

        #[test]
        fn test_canonical_bytes_deterministic(params: ReceiptParams) {
            let r1 = receipt_from_params(&params);
            let r2 = receipt_from_params(&params);

            let b1 = canonical_bytes(&r1);
            let b2 = canonical_bytes(&r2);

            prop_assert_eq!(b1, b2);
        }

        #[test]
        fn test_receipt_id_unique_with_different_payload(
            seed in any::<[u8; 32]>(),
            p1 in payload(100),
            p2 in payload(100),
        ) {
            prop_assume!(p1 != p2);

            let kp = Keypair::from_seed(&seed);
            let stream_id = StreamId::derive(&kp.public_key(), "test");

            let r1 = ReceiptBuilder::new(kp.public_key(), stream_id, 1)
                .kind(ReceiptKind::StreamInit)
                .timestamp(1000)
                .payload(p1)
                .sign(&kp);

            let r2 = ReceiptBuilder::new(kp.public_key(), stream_id, 1)
                .kind(ReceiptKind::StreamInit)
                .timestamp(1000)
                .payload(p2)
                .sign(&kp);

            prop_assert_ne!(r1.compute_id(), r2.compute_id());
        }
    }
}
