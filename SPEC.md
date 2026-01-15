# Chainge Kernel Specification

**Version**: 0.5.0
**Date**: 2026-01-15

The kernel is the minimal cryptographic primitive for verifiable memory. It does ONE thing: store and verify signed receipts.

This is the **atom** of memory. Everything else emerges from receipts and their relationships.

---

## 1. The Receipt

A receipt is an immutable, signed attestation. Once created, it cannot be modified.

```
Receipt {
    author:    [u8; 32]      // Ed25519 public key (RFC 8032)
    schema:    String        // URI (ASCII only, ≤256 bytes)
    refs:      Vec<[u8; 32]> // Receipt IDs (kernel normalizes order)
    payload:   Bytes         // Opaque content (≤64KB)
    signature: [u8; 64]      // Ed25519 signature
}
```

**That's it.** Four semantic fields plus a signature.

### Field Definitions

- **author**: 32-byte Ed25519 public key as defined in RFC 8032. Not X25519, not DID, not DER-encoded.
- **schema**: ASCII string identifying payload interpretation. No non-ASCII bytes.
- **refs**: Array of 32-byte `receipt_id` values. Kernel normalizes to sorted order; rejects duplicates.
- **payload**: Opaque bytes. Kernel does not interpret.
- **signature**: 64-byte Ed25519 signature.

---

## 2. Canonical Encoding

Receipts are encoded as **DAG-CBOR** (IPLD's deterministic CBOR subset).

### DAG-CBOR Constraints

- No floats
- No indefinite lengths
- Map keys are text strings, sorted by CBOR-encoded byte order
- No duplicate map keys
- **No extra keys** (reject unknown keys)

### Content Bytes (for signing)

```
content_bytes = dag_cbor({
    "author":  bytes(32),
    "payload": bytes,
    "refs":    [bytes(32), ...],  // normalized: sorted, no duplicates
    "schema":  text
})
```

**Exactly 4 keys.** Reject if any key is missing or extra keys present.

### Full Receipt Bytes

```
receipt_bytes = dag_cbor({
    "author":    bytes(32),
    "payload":   bytes,
    "refs":      [bytes(32), ...],
    "schema":    text,
    "signature": bytes(64)
})
```

**Exactly 5 keys.** Reject if any key is missing or extra keys present.

### CBOR Key Ordering

Keys are sorted by the **bytewise lexicographic order** of their CBOR-encoded key bytes (RFC 8949 deterministic encoding).

**Do not hardcode key order. Always compute order from encoded bytes.**

### Refs Normalization

The kernel **normalizes** refs before signing and encoding:

1. Sort lexicographically by bytes
2. Remove duplicates
3. If duplicates were present → **reject** (Error: duplicate refs)

This ensures deterministic signatures regardless of caller-provided order.

**CRITICAL**: This encoding is FROZEN. Changes break all existing signatures.

---

## 3. Signature with Domain Separation

The signature covers the content bytes with a domain separation prefix:

```
signed_message = "chainge/receipt-sig/v1" || content_bytes
signature = ed25519_sign(author_private_key, signed_message)
```

### Domain Prefix Encoding

Domain prefixes are the **raw UTF-8 bytes** of the ASCII string literal, concatenated directly:
- No null terminator
- No length prefix
- No separators

Example: `"chainge/receipt-sig/v1"` is exactly 22 bytes: `63 68 61 69 6e 67 65 2f 72 65 63 65 69 70 74 2d 73 69 67 2f 76 31`

---

## 4. Receipt ID with Domain Separation

The receipt ID is computed from the full receipt bytes:

```
receipt_id = sha256("chainge/receipt-id/v1" || receipt_bytes)
```

The ID is **never stored** in the receipt itself—it's always computed.

Domain prefix `"chainge/receipt-id/v1"` is exactly 21 bytes.

---

## 5. CID (Content Identifier)

For IPFS/dag-cbor interoperability, receipts have a CIDv1.

### Construction

```
digest     = sha256(receipt_bytes)
multihash  = 0x12 || 0x20 || digest      // sha2-256, 32 bytes
cid_bytes  = 0x01 || 0x71 || multihash   // CIDv1, dag-cbor codec
cid        = "b" + base32lower(cid_bytes) // multibase prefix
```

### Byte-by-byte

| Field | Value | Meaning |
|-------|-------|---------|
| `0x01` | CID version | CIDv1 |
| `0x71` | Codec | dag-cbor |
| `0x12` | Hash function | sha2-256 |
| `0x20` | Hash length | 32 bytes |
| `digest` | 32 bytes | SHA-256 of receipt_bytes |

**Note**: CID hashes `receipt_bytes` directly (no domain prefix) for IPFS verifiability. The `receipt_id` includes domain separation; the CID does not.

---

## 6. Receipt Validity

A receipt is valid if and only if ALL conditions hold:

| Check | Requirement |
|-------|-------------|
| schema | ASCII only, length ≤ 256 bytes |
| author | Exactly 32 bytes |
| signature | Exactly 64 bytes |
| refs | Length ≤ 128 entries (after dedup) |
| refs entries | Each exactly 32 bytes |
| refs | No duplicates (reject if duplicates found) |
| receipt_bytes | Valid DAG-CBOR with exactly required keys |
| content_bytes | Valid DAG-CBOR with exactly required keys |
| signature | Verifies: `ed25519_verify(author, "chainge/receipt-sig/v1" || content_bytes, signature)` |

**Kernel normalizes refs order. Kernel rejects duplicate refs.**

---

## 7. Core Invariants

These are the ONLY guarantees the kernel provides:

### Invariant 1: Content-Addressable Identity
```
receipt_id = sha256("chainge/receipt-id/v1" || receipt_bytes)
```
Same content, same signature → same ID. Always. Everywhere.

### Invariant 2: Author Authenticity
```
ed25519_verify(author, "chainge/receipt-sig/v1" || content_bytes, signature) == true
```
The signature proves authorship. This is the trust anchor.

### Invariant 3: Idempotent Ingestion
```
insert(receipt) → Inserted | AlreadyExists
```
Inserting the same receipt twice is a no-op.

### Invariant 4: Causal Ordering via Refs
```
If B.refs contains A.receipt_id, then A happened-before B.
```
This is the ONLY ordering primitive. No timestamps. No sequences.

### Invariant 5: Convergent Sync
```
sync(node_a, node_b) → receipts flow until both have superset
```
Two nodes willing to share everything converge to the same set.
Selective sharing (filtering by schema, author, etc.) is application policy.

---

## 8. Trust Physics

**Attestation is not verification.**

An attestation says: "I, author X, claim Y."
- The signature proves authorship
- The signature does NOT prove Y is true

Verification asks:
- "Is the signature valid?" (cryptographic - the kernel answers this)
- "Do I trust author X?" (social - application layer)
- "Is claim Y consistent?" (logic - application layer)

**Trust is built by gravity:**

```
Single attestation = claim
Multiple independent attestations = consensus
Countersigned attestation = witnessed truth
```

The kernel provides attestation infrastructure. Applications build trust by:
- Countersigns from trusted parties
- Consistency across many independent attestations
- Reputation accumulated over receipt history

**Offline-first:** Signing happens offline. Verification happens offline. Only sync requires network.

**Memory → $0:** Storage and bandwidth are cheap. Design assumes abundance:
- Keep everything (append-only)
- Replicate widely (convergent merge)
- Don't optimize for space (simplicity > compression)

---

## 9. What the Kernel Does NOT Do

| Concern | Where It Lives | Why Not Kernel |
|---------|----------------|----------------|
| Timestamps | Payload | Untrusted, application claim |
| Sequences | Conventions | Ordering is domain-specific |
| Streams | Conventions | refs[0] = prev by convention |
| Tombstones | Conventions | Deletion is semantic |
| Head pointers | Conventions | Mutable state is application |
| Permissions | Conventions | Access control is domain-specific |
| Schema registry | External | Kernel is payload-agnostic |
| Entity resolution | Application | Names are messy |
| Encryption | Payload | Kernel stores opaque bytes |

See **CONVENTIONS.md** for blessed patterns.

---

## 10. Refs: The Primitive for Relationships

Refs are how receipts relate to each other. The kernel stores them; applications interpret them.

**Kernel behavior:**
- Refs are normalized to sorted order (deterministic encoding)
- Duplicate refs are rejected
- Refs entries are `receipt_id` values (32 bytes each)
- Always present in CBOR (empty array `[]` if none)
- Maximum 128 entries

**Relationship patterns (see CONVENTIONS.md):**
- Chain: B.refs = [A] means B follows A
- Countersign: B.refs = [A] means B witnesses A
- Merge: C.refs = [A, B] means C combines A and B
- Tombstone: T.refs = [X] means T deletes X

---

## 11. Cryptographic Choices

| Purpose | Algorithm | Why |
|---------|-----------|-----|
| Hashing | SHA-256 | Universal: hardware acceleration, IPFS/CID compat |
| Signing | Ed25519 (RFC 8032) | Small signatures, fast verify, deterministic |
| Encoding | DAG-CBOR | IPLD-compatible deterministic CBOR |
| CID | CIDv1 + dag-cbor | IPFS ecosystem interoperability |

**Why SHA-256 over Blake3?**
- Hardware acceleration everywhere (Intel SHA-NI, ARM SHA)
- CIDv1/IPFS ecosystem uses SHA-256
- Every language has SHA-256
- Cost of interoperability → $0 with ubiquitous primitives

---

## 12. Limits

| Field | Limit | Rationale |
|-------|-------|-----------|
| schema | 256 bytes (ASCII) | URI should be short, ASCII only |
| refs | 128 entries | Sufficient for deep DAGs |
| payload | 64 KB | Larger content should be chunked |

---

## 13. Store Interface

The kernel's only storage interface:

```rust
trait Store {
    fn insert(&self, receipt: &Receipt) -> Result<InsertResult>;
    fn get(&self, id: &ReceiptId) -> Result<Option<Receipt>>;
    fn has(&self, id: &ReceiptId) -> Result<bool>;
    fn by_author(&self, author: &Author) -> Result<Vec<Receipt>>;
    fn refs_to(&self, id: &ReceiptId) -> Result<Vec<Receipt>>;
}

enum InsertResult {
    Inserted,
    AlreadyExists,
}
```

**No merge conflicts**: Receipts are immutable and content-addressed. Missing refs are allowed (the DAG may have gaps until synced).

---

## 14. Privacy Model

The kernel is agnostic to privacy. Payloads are opaque bytes.

**Public data**: Payload is plaintext JSON/CBOR.
**Private data**: Payload is encrypted before signing.

The kernel can hold:
- Fully public civic data
- E2EE medical records
- Encrypted personal journals

Privacy is application layer. The kernel just stores signed bytes.

---

## 15. Design Principles

1. **Minimal**: 4 fields + signature. Everything else is conventions or applications.
2. **Immutable**: Receipts never change. New receipts reference old ones.
3. **Content-addressed**: Identity is derived from content, not assigned.
4. **Offline-first**: Create and verify receipts without network.
5. **Trust-transparent**: Signatures reveal who attested. Trust is computed.
6. **Interoperable**: SHA-256, DAG-CBOR, CIDv1 = universal primitives.
7. **Append-only**: Memory is cheap. Keep everything.
8. **Deterministic**: Kernel normalizes input (refs order) for consistent signatures.

---

## 16. Ecosystem

| Document | Purpose |
|----------|---------|
| **SPEC.md** (this) | Kernel specification (frozen) |
| **CONVENTIONS.md** | Blessed patterns: tombstone, head, delegate, chain |
| **SYNC.md** | Sync protocol specification (future) |
| **SCHEMAS.md** | Schema registry conventions (future) |

---

*This spec is the constitution. All implementations must honor these invariants.*

*For higher-level patterns, see CONVENTIONS.md.*
