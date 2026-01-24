# Chainge Kernel: Cryptographic Specification

**Version:** 1.0.0
**Status:** Frozen

---

## 1. Receipt Structure

A receipt is the atomic unit. Five fields, deterministically serialized:

```
Receipt := {
    author:    bytes[32]      // Ed25519 public key
    schema:    text           // UTF-8 schema identifier
    refs:      [bytes[32]]    // Array of receipt IDs (max 128, sorted)
    payload:   bytes          // Opaque application data (max 64KB)
    signature: bytes[64]      // Ed25519 signature
}
```

**Serialization:** RFC 8949 Canonical CBOR (deterministic). Map keys as integers:
- `0`: author
- `1`: schema
- `2`: refs
- `3`: payload
- `4`: signature

---

## 2. Cryptographic Primitives

| Function | Algorithm | Reference |
|----------|-----------|-----------|
| Signature | Ed25519 | RFC 8032 |
| Hash | SHA-256 | FIPS 180-4 |
| Encoding | Canonical CBOR | RFC 8949 §4.2 |

No novel cryptography. Standard primitives only.

---

## 3. Receipt ID Derivation

Content-addressed with domain separation:

```
signed_content := CBOR(author, schema, refs, payload)  // Fields 0-3
receipt_bytes  := CBOR(author, schema, refs, payload, signature)  // Fields 0-4
receipt_id     := SHA-256("chainge/receipt-id/v1" || receipt_bytes)
```

**Properties:**
- Same content → same ID (content addressing)
- Domain prefix prevents cross-protocol attacks
- ID computed over signed content (signature included)

---

## 4. Signature

Signs the content, not the receipt ID:

```
signed_content := CBOR(author, schema, refs, payload)
signature      := Ed25519_Sign(author_privkey, signed_content)
```

**Verification:**
```
valid := Ed25519_Verify(author, signed_content, signature)
```

---

## 5. Causality

Receipts form a DAG via `refs`. Each ref is a receipt ID.

```
Invariant: If B.refs contains A.receipt_id, then A happened-before B.

Verification: For each ref in receipt.refs:
    1. Fetch receipt with that ID
    2. Verify its signature
    3. (Optional) Verify its refs recursively
```

**Constraints:**
- Max 128 refs per receipt
- Refs must be sorted (lexicographic on bytes)
- Self-reference prohibited
- Cycles impossible (hash preimage resistance)

---

## 6. Relay Model

Relays store and sequence receipts. They add an unsigned envelope:

```
Envelope := {
    receipt_id: bytes[32]     // SHA-256 of receipt
    stream_id:  bytes[32]     // Derived from author/entity
    seq:        uint64        // Relay-local sequence number
    relay_ts:   uint64        // Relay timestamp (ms since epoch)
}
```

**Critical:** Envelope is NOT signed. Receipt is signed offline; relay assigns sequence atomically on receipt.

**Stream ID derivation:**
```
author_stream := SHA-256("stream:author:" || author_pubkey)
entity_stream := SHA-256("stream:entity:" || entity_id)
```

---

## 7. Security Properties

**What signatures guarantee:**
- Authenticity: Only holder of `author_privkey` could have signed
- Integrity: Any modification invalidates signature
- Non-repudiation: Author cannot deny signing (absent key compromise)

**What content-addressing guarantees:**
- Immutability: Changing content changes ID
- Deduplication: Same receipt submitted twice → same ID
- Verifiability: Anyone with receipt can compute and verify ID

**What refs guarantee:**
- Causal ordering: B refs A → A happened-before B
- Tamper evidence: Modifying A changes A's ID, breaking B's ref

---

## 8. Threat Model

| Actor | Can | Cannot |
|-------|-----|--------|
| Relay | Censor receipts, reorder within stream, lie about timestamps | Forge signatures, modify receipt content, fake receipt IDs |
| Attacker | Observe public receipts, attempt DoS | Break Ed25519, find SHA-256 collisions, forge without private key |
| Author | Sign any content, tombstone own receipts | Unsign published receipts, modify after publication |

**Relay honesty detection:**
```
If relay returns receipt R with claimed receipt_id:
    computed := SHA-256("chainge/receipt-id/v1" || R)
    assert computed == claimed_receipt_id  // Detects tampering
    assert Ed25519_Verify(R.author, R.content, R.signature)  // Detects forgery
```

---

## 9. Key Management (Storyband)

Hierarchical deterministic identity:

```
Root Key: Ed25519 keypair (Secure Enclave / StrongBox)
Device Key: Delegated via signed receipt (storyband.delegate/v1)
Context Key: HKDF(root_privkey, "context:" || org_id)
```

**Recovery:** K-of-N Shamir's Secret Sharing over root private key. Guardians hold shards; threshold reconstruction on social verification.

**DID Format:** `did:story:<base58(pubkey)>`

---

## 10. What's NOT in the Kernel

| Concern | Where it lives |
|---------|----------------|
| Timestamps | Envelope (relay) or payload (claim) |
| Permissions | Application layer |
| Encryption | Payload is opaque; E2EE at application layer |
| Identity/names | Application layer (Storyband spec) |
| Conflict resolution | Application layer |
| Trust scores | Computed by observers from graph |

The kernel is minimal. ~200 lines of code per implementation.

---

## 11. Test Vectors

Golden vectors ensure cross-implementation compatibility:

```
Input:
    author:  0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60
    schema:  "test/v1"
    refs:    []
    payload: 0x48656c6c6f  // "Hello"
    privkey: 0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60...

Expected:
    signature: 0x... (64 bytes)
    receipt_id: 0x... (32 bytes)
```

10 language implementations (Rust, Swift, TypeScript, Go, Python, Kotlin, Java, R, C#, JavaScript) pass identical vectors.

---

## 12. Summary

```
sign(privkey, content) → receipt
verify(receipt) → bool
id(receipt) → bytes[32]

Guarantees: authenticity, integrity, causality, content-addressing
Does not guarantee: timestamps, ordering, availability, privacy

Primitives: Ed25519 + SHA-256 + Canonical CBOR
Novel crypto: None
```

---

**Repository:** github.com/ericyarmo/Kernel
**Implementations:** Rust, Swift, TypeScript, Go, Python, Kotlin, Java, R, C#, JavaScript

