# Chainge Kernel Specification v0

**Project Kickoff: January 14, 2026**

This document defines the physics of verifiable memory: receipts, streams, and permissions.

---

## Table of Contents

1. [Receipt Schema v0](#1-receipt-schema-v0)
2. [Canonicalization Spec v0](#2-canonicalization-spec-v0)
3. [Stream Model v0](#3-stream-model-v0)
4. [Storage Schema](#4-storage-schema)
5. [Sync Protocol v0](#5-sync-protocol-v0)
6. [Permissions Model v0](#6-permissions-model-v0)
7. [Workspace Layout](#7-workspace-layout)
8. [Definition of Done](#8-definition-of-done)

---

## 1. Receipt Schema v0

A Receipt is the atomic unit of verifiable memory. It is immutable by definition.

### 1.1 Receipt Structure

```
Receipt {
    header: ReceiptHeader,
    payload: Bytes,           // Opaque application payload (may be encrypted)
    signature: Ed25519Signature,
}

ReceiptHeader {
    version: u8,              // Schema version, currently 0
    author: Ed25519PublicKey, // 32 bytes
    stream_id: StreamId,      // 32 bytes (derived, see §3)
    seq: u64,                 // Sequence number within stream (1-indexed)
    timestamp: i64,           // Unix milliseconds, author-claimed (untrusted)
    kind: ReceiptKind,        // Discriminator for payload interpretation
    prev_receipt_id: Option<ReceiptId>, // Hash of previous receipt in stream (None if seq=1)
    refs: Vec<ReceiptId>,     // Optional references to other receipts (max 16)
    payload_hash: Blake3Hash, // 32 bytes, hash of payload bytes
}

ReceiptId = Blake3(canonical_bytes(Receipt))  // 32 bytes
```

### 1.2 ReceiptKind Enum

```rust
#[repr(u16)]
enum ReceiptKind {
    // Core kinds (0x0000 - 0x00FF)
    Data = 0x0001,            // Generic application data
    Tombstone = 0x0002,       // Supersedes a previous receipt
    StreamInit = 0x0003,      // First receipt in a stream (seq=1)

    // Permission kinds (0x0100 - 0x01FF)
    Grant = 0x0100,           // Permission grant
    Revoke = 0x0101,          // Permission revocation
    KeyShare = 0x0102,        // Encrypted key material for recipient

    // Sync kinds (0x0200 - 0x02FF)
    Anchor = 0x0200,          // Checkpoint for sync protocol

    // Application-defined (0x1000+)
    Custom(u16),
}
```

### 1.3 What Is Signed

The signature covers the **canonical bytes of the header concatenated with the payload**:

```
signed_message = canonical_bytes(header) || payload
signature = Ed25519_Sign(author_private_key, signed_message)
```

The signature is NOT included in the signed message (obviously).
The `receipt_id` is computed over the complete receipt including signature:

```
receipt_id = Blake3(canonical_bytes(header) || payload || signature)
```

This ensures the receipt_id commits to a specific signature, preventing signature malleability issues.

### 1.4 Strong Types

```rust
// All IDs are newtypes, not raw [u8; 32]
pub struct ReceiptId([u8; 32]);
pub struct StreamId([u8; 32]);
pub struct Blake3Hash([u8; 32]);
pub struct Ed25519PublicKey([u8; 32]);
pub struct Ed25519Signature([u8; 64]);
```

---

## 2. Canonicalization Spec v0

Deterministic encoding is the foundation. Two implementations given the same Receipt MUST produce byte-identical output.

### 2.1 Encoding Format: Canonical CBOR (RFC 8949 Core Deterministic)

We use CBOR with the following restrictions:

1. **Map keys**: Sorted by byte comparison of encoded keys (not string comparison)
2. **Integers**: Smallest valid encoding (no leading zeros in additional bytes)
3. **Lengths**: Definite length only (no streaming/indefinite)
4. **Floats**: Not used in v0 (timestamps are i64 milliseconds)
5. **Tags**: Only well-defined CBOR tags allowed; none in v0
6. **Duplicates**: Map keys MUST NOT be duplicated

### 2.2 Field Ordering for Header

When encoding `ReceiptHeader` as a CBOR map, keys are assigned fixed integer identifiers:

```
Key assignments (canonical order by integer encoding):
  0: version        (u8)
  1: author         (bytes, 32)
  2: stream_id      (bytes, 32)
  3: seq            (u64)
  4: timestamp      (i64)
  5: kind           (u16)
  6: prev_receipt_id (bytes, 32 | null)
  7: refs           (array of bytes)
  8: payload_hash   (bytes, 32)
```

CBOR integer keys encode as: 0-23 in 1 byte. So keys 0-8 encode as single bytes 0x00-0x08.

### 2.3 Canonical Bytes Function

```rust
fn canonical_bytes(receipt: &Receipt) -> Vec<u8> {
    let mut buf = Vec::new();

    // Encode header as CBOR map with integer keys
    buf.extend(encode_header_cbor(&receipt.header));

    // Payload is raw bytes (already part of the receipt)
    buf.extend(&receipt.payload);

    // Signature is 64 bytes
    buf.extend(&receipt.signature.0);

    buf
}

fn canonical_header_bytes(header: &ReceiptHeader) -> Vec<u8> {
    encode_header_cbor(header)
}
```

### 2.4 Sample Canonical Bytes

#### Example 1: StreamInit Receipt

```
Receipt {
    header: {
        version: 0,
        author: 0x0102...1f20 (32 bytes, placeholder),
        stream_id: 0x2122...3f40 (32 bytes),
        seq: 1,
        timestamp: 1736870400000 (2026-01-14T12:00:00Z),
        kind: StreamInit (0x0003),
        prev_receipt_id: None,
        refs: [],
        payload_hash: Blake3(b"hello"),
    },
    payload: b"hello",
    signature: 0x...64 bytes...
}
```

**Canonical Header CBOR (hex):**
```
A9                        # map(9)
  00                      # key: 0 (version)
  00                      # value: 0
  01                      # key: 1 (author)
  58 20                   # bytes(32)
    0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
  02                      # key: 2 (stream_id)
  58 20                   # bytes(32)
    2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40
  03                      # key: 3 (seq)
  01                      # value: 1
  04                      # key: 4 (timestamp)
  1B 00000193E5A46000    # value: 1736870400000
  05                      # key: 5 (kind)
  19 0003                # value: 3 (u16)
  06                      # key: 6 (prev_receipt_id)
  F6                      # null
  07                      # key: 7 (refs)
  80                      # array(0)
  08                      # key: 8 (payload_hash)
  58 20                   # bytes(32)
    <blake3("hello")>
```

#### Example 2: Data Receipt (seq=2, with prev)

```
Receipt {
    header: {
        version: 0,
        author: <same>,
        stream_id: <same>,
        seq: 2,
        timestamp: 1736870401000,
        kind: Data (0x0001),
        prev_receipt_id: Some(<receipt_id of seq=1>),
        refs: [],
        payload_hash: Blake3(b"world"),
    },
    payload: b"world",
    signature: ...
}
```

#### Example 3: Tombstone Receipt

```
Receipt {
    header: {
        version: 0,
        author: <same>,
        stream_id: <same>,
        seq: 3,
        timestamp: 1736870402000,
        kind: Tombstone (0x0002),
        prev_receipt_id: Some(<receipt_id of seq=2>),
        refs: [<receipt_id of the tombstoned receipt>],  // What we're tombstoning
        payload_hash: Blake3(b""),
    },
    payload: b"",  // Tombstone has empty payload
    signature: ...
}
```

### 2.5 Verification Algorithm

```rust
fn verify_receipt(receipt: &Receipt) -> Result<(), ValidationError> {
    // 1. Check version
    if receipt.header.version != 0 {
        return Err(ValidationError::UnsupportedVersion);
    }

    // 2. Verify payload_hash matches actual payload
    let computed_hash = blake3::hash(&receipt.payload);
    if computed_hash.as_bytes() != &receipt.header.payload_hash.0 {
        return Err(ValidationError::PayloadHashMismatch);
    }

    // 3. Construct signed message
    let header_bytes = canonical_header_bytes(&receipt.header);
    let signed_message = [header_bytes.as_slice(), receipt.payload.as_slice()].concat();

    // 4. Verify Ed25519 signature
    let public_key = ed25519_dalek::PublicKey::from_bytes(&receipt.header.author.0)?;
    let signature = ed25519_dalek::Signature::from_bytes(&receipt.signature.0)?;
    public_key.verify(&signed_message, &signature)?;

    Ok(())
}
```

---

## 3. Stream Model v0

A Stream is an ordered, append-only log of receipts from a single author.

### 3.1 Stream Identity

```rust
// StreamId is derived from author + stream_name
// This allows an author to have multiple named streams
fn derive_stream_id(author: &Ed25519PublicKey, stream_name: &str) -> StreamId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"chainge-stream-v0:");
    hasher.update(&author.0);
    hasher.update(b":");
    hasher.update(stream_name.as_bytes());
    StreamId(hasher.finalize().into())
}
```

A stream is uniquely identified by `(author, stream_name)` → `StreamId`.

### 3.2 Sequence Numbers

- **1-indexed**: First receipt in a stream has `seq = 1`
- **Monotonically increasing**: No gaps in a valid stream from author's perspective
- **Immutable mapping**: Once a receipt claims `(stream_id, seq)`, no other receipt may claim that position
- **Hash chain**: Each receipt (seq > 1) MUST include `prev_receipt_id` pointing to the receipt at seq-1

### 3.3 Stream State

```rust
struct StreamState {
    stream_id: StreamId,
    author: Ed25519PublicKey,
    head_seq: u64,                    // Highest contiguous seq we have
    head_receipt_id: ReceiptId,       // receipt_id at head_seq
    known_max_seq: u64,               // Highest seq we've heard about (may have gaps)
    gaps: BTreeSet<u64>,              // Missing sequence numbers
    state_hash: Blake3Hash,           // Deterministic hash of replayed state
}
```

### 3.4 Gap Detection

When ingesting a receipt with `seq = N`:
1. If `N == head_seq + 1`: Advance head, no gap
2. If `N > head_seq + 1`: Record gaps for `head_seq + 1 .. N`
3. If `N <= head_seq`: Check if we already have this seq
   - If same `receipt_id`: Idempotent, ignore
   - If different: CONFLICT - this is a fork/equivocation

### 3.5 Fork Detection (Equivocation)

If an author produces two different receipts with the same `(stream_id, seq)`, this is **equivocation**. The kernel MUST:

1. Store both receipts (evidence)
2. Mark the stream as **forked**
3. Refuse to advance the stream state past the fork point
4. Report the fork to the application layer

```rust
enum StreamHealth {
    Healthy,
    HasGaps { missing: Vec<u64> },
    Forked { at_seq: u64, receipts: Vec<ReceiptId> },
}
```

### 3.6 Tombstones and Supersession

A Tombstone receipt "marks" a previous receipt as superseded. Rules:

1. Tombstone MUST reference the tombstoned receipt in `refs[0]`
2. Tombstoned receipt MUST be in the same stream
3. Tombstoned receipt MUST have lower seq than the tombstone
4. Tombstones are themselves immutable (you cannot un-tombstone)
5. Tombstone of a tombstone is allowed (for undo workflows)

**Replay semantics**: When replaying a stream, a tombstoned receipt is still processed but flagged. Applications decide how to handle.

```rust
struct ReplayEvent {
    receipt: Receipt,
    is_tombstoned: bool,
    tombstoned_by: Option<ReceiptId>,
}
```

---

## 4. Storage Schema

### 4.1 SQLite Tables

```sql
-- Core receipt storage
CREATE TABLE receipts (
    receipt_id BLOB PRIMARY KEY,      -- 32 bytes
    stream_id BLOB NOT NULL,          -- 32 bytes
    seq INTEGER NOT NULL,             -- sequence number
    author BLOB NOT NULL,             -- 32 bytes, Ed25519 public key
    timestamp INTEGER NOT NULL,       -- Unix ms
    kind INTEGER NOT NULL,            -- ReceiptKind as u16
    prev_receipt_id BLOB,             -- 32 bytes, nullable
    refs BLOB NOT NULL,               -- CBOR array of receipt_ids
    payload_hash BLOB NOT NULL,       -- 32 bytes
    payload BLOB NOT NULL,            -- raw payload bytes
    signature BLOB NOT NULL,          -- 64 bytes
    canonical_bytes BLOB NOT NULL,    -- cached canonical encoding
    ingested_at INTEGER NOT NULL,     -- local timestamp of ingestion
    verified INTEGER NOT NULL DEFAULT 0,  -- 0=unverified, 1=valid, -1=invalid

    UNIQUE(stream_id, seq)
);

-- Stream state tracking
CREATE TABLE streams (
    stream_id BLOB PRIMARY KEY,
    author BLOB NOT NULL,
    stream_name TEXT NOT NULL,
    head_seq INTEGER NOT NULL DEFAULT 0,
    head_receipt_id BLOB,
    known_max_seq INTEGER NOT NULL DEFAULT 0,
    state_hash BLOB,
    health INTEGER NOT NULL DEFAULT 0,  -- 0=healthy, 1=gaps, 2=forked
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Gap tracking for sync
CREATE TABLE stream_gaps (
    stream_id BLOB NOT NULL,
    missing_seq INTEGER NOT NULL,
    requested_at INTEGER,              -- when we last requested this
    PRIMARY KEY (stream_id, missing_seq)
);

-- Fork evidence
CREATE TABLE forks (
    stream_id BLOB NOT NULL,
    seq INTEGER NOT NULL,
    receipt_id BLOB NOT NULL,
    detected_at INTEGER NOT NULL,
    PRIMARY KEY (stream_id, seq, receipt_id)
);

-- Indexes
CREATE INDEX idx_receipts_stream_seq ON receipts(stream_id, seq);
CREATE INDEX idx_receipts_author ON receipts(author);
CREATE INDEX idx_receipts_kind ON receipts(kind);
CREATE INDEX idx_receipts_timestamp ON receipts(timestamp);
CREATE INDEX idx_receipts_refs ON receipts(refs);  -- for finding tombstones
```

### 4.2 Store Trait

```rust
#[async_trait]
pub trait Store: Send + Sync {
    // Receipt operations
    async fn insert_receipt(&self, receipt: &Receipt, canonical: &[u8]) -> Result<InsertResult>;
    async fn get_receipt(&self, id: &ReceiptId) -> Result<Option<Receipt>>;
    async fn get_receipt_by_position(&self, stream_id: &StreamId, seq: u64) -> Result<Option<Receipt>>;
    async fn get_receipts_range(&self, stream_id: &StreamId, start: u64, end: u64) -> Result<Vec<Receipt>>;
    async fn has_receipt(&self, id: &ReceiptId) -> Result<bool>;

    // Stream operations
    async fn get_stream_state(&self, stream_id: &StreamId) -> Result<Option<StreamState>>;
    async fn update_stream_state(&self, state: &StreamState) -> Result<()>;
    async fn list_streams(&self, author: Option<&Ed25519PublicKey>) -> Result<Vec<StreamId>>;

    // Gap operations
    async fn get_gaps(&self, stream_id: &StreamId) -> Result<Vec<u64>>;
    async fn add_gaps(&self, stream_id: &StreamId, seqs: &[u64]) -> Result<()>;
    async fn remove_gap(&self, stream_id: &StreamId, seq: u64) -> Result<()>;

    // Fork operations
    async fn record_fork(&self, stream_id: &StreamId, seq: u64, receipt_id: &ReceiptId) -> Result<()>;
    async fn get_forks(&self, stream_id: &StreamId) -> Result<Vec<Fork>>;

    // Bulk operations for sync
    async fn get_receipt_ids_since(&self, stream_id: &StreamId, after_seq: u64) -> Result<Vec<(u64, ReceiptId)>>;
    async fn get_all_stream_heads(&self) -> Result<Vec<(StreamId, u64, ReceiptId)>>;
}

pub enum InsertResult {
    Inserted,
    AlreadyExists,
    Conflict { existing: ReceiptId },
}
```

---

## 5. Sync Protocol v0

The sync protocol allows two nodes to converge to the same receipt set through an untrusted relay.

### 5.1 Design Principles

1. **Idempotent**: Replaying any message has no side effects
2. **Commutative**: Message order doesn't affect final state
3. **Resumable**: Sync can be interrupted and resumed
4. **Bandwidth-efficient**: Don't resend receipts the peer has

### 5.2 Message Types

```rust
enum SyncMessage {
    // Discovery phase
    Hello {
        node_id: NodeId,
        protocol_version: u8,
        streams_of_interest: Vec<StreamId>,  // Optional filter
    },

    // State advertisement
    StreamHeads {
        heads: Vec<StreamHead>,
    },

    // Request missing data
    NeedReceipts {
        requests: Vec<ReceiptRequest>,
    },

    // Provide data
    Receipts {
        receipts: Vec<Receipt>,
    },

    // Acknowledgment
    Ack {
        received: Vec<ReceiptId>,
    },

    // Error
    Error {
        code: SyncErrorCode,
        message: String,
    },
}

struct StreamHead {
    stream_id: StreamId,
    head_seq: u64,
    head_receipt_id: ReceiptId,
}

struct ReceiptRequest {
    stream_id: StreamId,
    seqs: SeqRange,  // Can be single, range, or list
}

enum SeqRange {
    Single(u64),
    Range { start: u64, end: u64 },
    List(Vec<u64>),
}
```

### 5.3 Sync Algorithm (Anti-Entropy)

```
Node A                              Node B
  |                                    |
  |-------- Hello ------------------>  |
  |<------- Hello --------------------|
  |                                    |
  |-------- StreamHeads ------------->  |
  |<------- StreamHeads --------------|
  |                                    |
  |  [Compare heads, compute needs]    |
  |                                    |
  |<------- NeedReceipts -------------|
  |-------- NeedReceipts ------------>  |
  |                                    |
  |-------- Receipts ---------------->  |
  |<------- Receipts -----------------|
  |                                    |
  |<------- Ack ----------------------|
  |-------- Ack --------------------->  |
  |                                    |
  [Repeat until converged]
```

### 5.4 Convergence Rules

1. **Receipt set convergence**: After sync, both nodes have the union of receipts for all synced streams.

2. **Head convergence**: For each stream:
   - If no forks: Both nodes have identical `(head_seq, head_receipt_id)`
   - If fork detected: Both nodes are aware of the fork

3. **State hash convergence**: After replay, both nodes compute identical `state_hash` for each stream.

### 5.5 Idempotency Rules

| Message | Idempotency |
|---------|-------------|
| Hello | Safe to repeat |
| StreamHeads | Safe to repeat (advertises current state) |
| NeedReceipts | Safe to repeat (just a request) |
| Receipts | Ingestion is idempotent by receipt_id |
| Ack | Safe to repeat (informational) |

### 5.6 Handling Untrusted Transport

The relay may:
- **Reorder messages**: Protocol handles out-of-order delivery
- **Duplicate messages**: All operations are idempotent
- **Drop messages**: Timeout + retry handles this
- **Inject invalid receipts**: Signature verification rejects them
- **Delay indefinitely**: Application-level timeout, can resume later

---

## 6. Permissions Model v0

Permissions are receipts. Access control is computed by replaying permission events.

### 6.1 Permission Receipts

```rust
// Grant permission
struct GrantPayload {
    recipient: Ed25519PublicKey,      // Who is granted access
    scope: PermissionScope,           // What access is granted
    conditions: Option<Conditions>,    // Optional constraints
    encrypted_key: Option<Bytes>,      // Encrypted key material (for read access)
}

// Revoke permission
struct RevokePayload {
    grant_receipt_id: ReceiptId,      // The grant being revoked
    reason: Option<String>,
}

enum PermissionScope {
    ReadStream { stream_id: StreamId },
    ReadReceipt { receipt_id: ReceiptId },
    WriteStream { stream_id: StreamId },  // Delegate append rights
    Admin { stream_id: StreamId },         // Full control
}

struct Conditions {
    expires_at: Option<i64>,          // Unix ms
    max_uses: Option<u32>,
}
```

### 6.2 Permission State

Permission state is computed by replay:

```rust
struct PermissionState {
    grants: HashMap<(Ed25519PublicKey, PermissionScope), GrantState>,
}

struct GrantState {
    grant_receipt_id: ReceiptId,
    granted_at: i64,
    conditions: Option<Conditions>,
    revoked: bool,
    revoked_at: Option<i64>,
    revoke_receipt_id: Option<ReceiptId>,
}

impl PermissionState {
    fn can_read(&self, principal: &Ed25519PublicKey, stream_id: &StreamId, now: i64) -> bool {
        // Check for valid, non-revoked, non-expired grant
    }
}
```

### 6.3 Encrypted Payload Sharing

For sharing encrypted data:

1. **Sender** encrypts payload with a symmetric key (ChaCha20-Poly1305)
2. **Key Sharing** uses X25519 key agreement:
   - Sender creates ephemeral X25519 keypair
   - Sender derives shared secret with recipient's X25519 public key
   - Sender encrypts symmetric key with derived key
   - KeyShare receipt contains: `(ephemeral_public, encrypted_symmetric_key)`

```rust
struct KeySharePayload {
    grant_receipt_id: ReceiptId,           // Links to the grant
    ephemeral_public: X25519PublicKey,     // 32 bytes
    encrypted_key: Bytes,                   // ChaCha20-Poly1305 encrypted symmetric key
    nonce: [u8; 12],
}
```

### 6.4 Envelope Format for Encrypted Payloads

```rust
struct EncryptedPayload {
    format: u8,                    // 0 = ChaCha20-Poly1305
    nonce: [u8; 12],
    ciphertext: Bytes,             // Encrypted actual payload + Poly1305 tag
    key_shares: Vec<ReceiptId>,    // References to KeyShare receipts
}
```

### 6.5 Permission Verification Flow

```
1. Alice creates stream S, publishes receipts with encrypted payloads
2. Alice creates Grant receipt: (Bob can read S)
3. Alice creates KeyShare receipt for Bob: (encrypted symmetric key)
4. Bob receives Grant + KeyShare via sync
5. Bob decrypts symmetric key using his X25519 private key
6. Bob decrypts payloads in stream S
7. Alice creates Revoke receipt for Bob's grant
8. Bob can still read existing data (has key), but:
   - Future payloads use new keys
   - New KeyShare NOT issued to Bob
```

---

## 7. Workspace Layout

```
chainge-kernel/
├── Cargo.toml                    # Workspace manifest
├── SPEC_V0.md                    # This document
├── README.md
├── LICENSE
│
├── crates/
│   ├── chainge-kernel-core/      # Pure primitives
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── receipt.rs        # Receipt struct, ReceiptId, ReceiptKind
│   │       ├── stream.rs         # StreamId, StreamState, gap detection
│   │       ├── canonical.rs      # Canonicalization (CBOR encoding)
│   │       ├── crypto.rs         # Ed25519, Blake3 wrappers
│   │       ├── validation.rs     # Receipt verification
│   │       ├── types.rs          # Strong type definitions
│   │       └── error.rs
│   │
│   ├── chainge-kernel-store/     # Storage abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs         # Store trait
│   │       ├── sqlite.rs         # SQLite implementation
│   │       ├── memory.rs         # In-memory implementation (for tests)
│   │       └── migration.rs      # Schema migrations
│   │
│   ├── chainge-kernel-sync/      # Sync protocol
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── messages.rs       # SyncMessage types
│   │       ├── protocol.rs       # Sync state machine
│   │       ├── convergence.rs    # Anti-entropy algorithm
│   │       └── transport.rs      # Transport trait
│   │
│   ├── chainge-kernel-perms/     # Permissions + encryption
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── grant.rs          # Grant/Revoke logic
│   │       ├── state.rs          # Permission state computation
│   │       ├── envelope.rs       # Encrypted payload envelope
│   │       ├── keyshare.rs       # X25519 key sharing
│   │       └── crypto.rs         # ChaCha20-Poly1305, X25519
│   │
│   ├── chainge-kernel/           # Unified kernel API
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── kernel.rs         # Kernel struct, main API
│   │       ├── builder.rs        # KernelBuilder
│   │       └── config.rs
│   │
│   └── chainge-kernel-testkit/   # Testing utilities
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── vectors.rs        # Golden test vectors
│           ├── generators.rs     # Proptest generators
│           ├── fixtures.rs       # Test fixtures
│           └── fuzz/             # Fuzz targets
│               ├── canonical.rs
│               └── ingest.rs
│
├── tests/                        # Integration tests
│   ├── convergence_tests.rs
│   ├── sync_tests.rs
│   └── permission_tests.rs
│
└── examples/
    ├── simple_append.rs
    ├── sync_two_nodes.rs
    └── encrypted_sharing.rs
```

### 7.1 Crate Dependencies

```
chainge-kernel-core (no deps on other kernel crates)
       ↑
chainge-kernel-store (depends on core)
       ↑
chainge-kernel-sync (depends on core, store)
       ↑
chainge-kernel-perms (depends on core)
       ↑
chainge-kernel (depends on all)
       ↑
chainge-kernel-testkit (depends on all)
```

---

## 8. Definition of Done

### 8.1 chainge-kernel-core

| Requirement | Test |
|-------------|------|
| Receipt struct with all fields | Unit test |
| ReceiptId computed correctly | Golden vector test |
| Canonical CBOR encoding deterministic | Property: encode(r) == encode(r) across runs |
| Canonical encoding matches spec | Golden vector: known receipt → known bytes |
| Ed25519 signature verification | Valid sig passes, tampered fails |
| Blake3 payload hash verification | Correct hash passes, wrong hash fails |
| Strong types prevent misuse | Compile-time: can't pass ReceiptId where StreamId expected |
| StreamId derivation deterministic | Golden vector test |

**Acceptance Tests:**
```rust
#[test]
fn test_determinism_canonical_bytes() {
    let receipt = make_test_receipt();
    let bytes1 = canonical_bytes(&receipt);
    let bytes2 = canonical_bytes(&receipt);
    assert_eq!(bytes1, bytes2);
}

#[test]
fn test_receipt_id_golden_vector() {
    let receipt = parse_golden_receipt(GOLDEN_RECEIPT_JSON);
    let id = receipt.compute_id();
    assert_eq!(id.to_hex(), EXPECTED_RECEIPT_ID_HEX);
}

#[test]
fn test_signature_verification_valid() { ... }

#[test]
fn test_signature_verification_tampered() { ... }
```

### 8.2 chainge-kernel-store

| Requirement | Test |
|-------------|------|
| Insert receipt | Insert succeeds, can retrieve |
| Idempotent insert | Double insert returns AlreadyExists |
| Conflict detection | Same (stream, seq), different id → Conflict |
| Query by receipt_id | Returns correct receipt |
| Query by (stream, seq) | Returns correct receipt |
| Range query | Returns ordered receipts |
| Stream state tracking | head_seq advances correctly |
| Gap tracking | Gaps recorded and removed correctly |
| Fork detection | Multiple receipts at same position recorded |

**Acceptance Tests:**
```rust
#[test]
fn test_idempotent_insert() {
    let store = SqliteStore::open_memory();
    let receipt = make_receipt();
    let r1 = store.insert(&receipt);
    let r2 = store.insert(&receipt);
    assert!(matches!(r1, InsertResult::Inserted));
    assert!(matches!(r2, InsertResult::AlreadyExists));
    // Only one copy in DB
    assert_eq!(store.count_receipts(), 1);
}
```

### 8.3 chainge-kernel-sync

| Requirement | Test |
|-------------|------|
| StreamHeads computed correctly | Unit test |
| NeedReceipts computed from head diff | Unit test |
| Receipt delivery idempotent | Duplicate receipts handled |
| Convergence: same receipts | Two nodes sync, same receipt set |
| Convergence: same heads | Two nodes sync, same stream heads |
| Convergence: same state hash | Two nodes replay, same hash |
| Gap filling | Sync fills gaps |
| Out-of-order handling | Reordered messages still converge |
| Fork propagation | Fork detected on one node, propagated |

**Acceptance Tests:**
```rust
#[test]
fn test_convergence_offline_sync() {
    let (node_a, node_b) = make_two_nodes();

    // Both create receipts offline
    node_a.append(stream, kind, b"a1");
    node_a.append(stream, kind, b"a2");
    node_b.append(stream, kind, b"b1");

    // Sync through mock relay
    let relay = MockRelay::new();
    node_a.sync_with(&relay);
    node_b.sync_with(&relay);

    // Verify convergence
    assert_eq!(
        node_a.all_receipt_ids(),
        node_b.all_receipt_ids()
    );
    assert_eq!(
        node_a.stream_head(stream),
        node_b.stream_head(stream)
    );
    assert_eq!(
        node_a.replay_state_hash(stream),
        node_b.replay_state_hash(stream)
    );
}
```

### 8.4 chainge-kernel-perms

| Requirement | Test |
|-------------|------|
| Grant receipt created | Correct structure |
| Revoke receipt references grant | Validation |
| Permission state from replay | Grants appear, revokes hide |
| Expired grants rejected | Time-based check |
| Key sharing works | Recipient can decrypt |
| Revoked key not re-shared | New keys not shared to revoked |
| Envelope format correct | Encrypt/decrypt roundtrip |

**Acceptance Tests:**
```rust
#[test]
fn test_permission_grant_revoke_flow() {
    let alice = make_keypair();
    let bob = make_keypair();
    let stream = StreamId::derive(&alice.public(), "secret");

    // Alice grants Bob read access
    let grant_id = perms.grant(stream, bob.public(), Scope::ReadStream);
    assert!(perms.can_read(&bob.public(), &stream));

    // Alice revokes
    perms.revoke(grant_id);
    assert!(!perms.can_read(&bob.public(), &stream));
}

#[test]
fn test_encrypted_payload_sharing() {
    let alice = make_keypair();
    let bob = make_keypair();

    // Alice encrypts payload
    let (envelope, sym_key) = encrypt_payload(b"secret data");

    // Alice shares key with Bob
    let key_share = create_key_share(sym_key, &bob.public_x25519());

    // Bob decrypts
    let recovered_key = bob.decrypt_key_share(&key_share);
    let plaintext = decrypt_envelope(&envelope, &recovered_key);

    assert_eq!(plaintext, b"secret data");
}
```

### 8.5 Integration Tests

```rust
#[test]
fn test_full_workflow() {
    // 1. Create kernel
    let kernel = Kernel::new(store, crypto, clock);

    // 2. Create stream
    let stream = kernel.create_stream("my-log");

    // 3. Append receipts
    let r1 = kernel.append(stream, Kind::Data, b"event 1");
    let r2 = kernel.append(stream, Kind::Data, b"event 2");

    // 4. Query
    let receipts = kernel.query(stream, 1..=2);
    assert_eq!(receipts.len(), 2);

    // 5. Replay
    let (state_hash, _) = kernel.replay(stream);

    // 6. Second kernel, sync
    let kernel2 = Kernel::new(store2, crypto2, clock2);
    kernel2.sync(&kernel);

    // 7. Verify convergence
    assert_eq!(
        kernel.replay(stream).0,
        kernel2.replay(stream).0
    );
}
```

---

## 9. Implementation Phases

### Phase 1: Core Primitives (Foundation)
- [ ] Strong types (ReceiptId, StreamId, etc.)
- [ ] Canonical CBOR encoding
- [ ] Blake3 hashing
- [ ] Ed25519 signing/verification
- [ ] Receipt structure
- [ ] Golden vector tests

### Phase 2: Storage
- [ ] Store trait definition
- [ ] In-memory store (for tests)
- [ ] SQLite store
- [ ] Idempotent insert
- [ ] Stream state tracking

### Phase 3: Streams
- [ ] StreamId derivation
- [ ] Sequence number management
- [ ] Gap detection
- [ ] Fork detection
- [ ] Tombstone handling
- [ ] Replay function

### Phase 4: Sync
- [ ] Message types
- [ ] Protocol state machine
- [ ] Anti-entropy algorithm
- [ ] Transport trait
- [ ] Convergence tests

### Phase 5: Permissions
- [ ] Grant/Revoke receipts
- [ ] Permission state computation
- [ ] X25519 key agreement
- [ ] ChaCha20-Poly1305 encryption
- [ ] Envelope format
- [ ] Key sharing

### Phase 6: Polish
- [ ] Unified Kernel API
- [ ] Error handling
- [ ] Logging/tracing
- [ ] Documentation
- [ ] Examples

---

## Appendix A: Cryptographic Choices

| Purpose | Algorithm | Crate |
|---------|-----------|-------|
| Hashing | BLAKE3 | `blake3` |
| Signing | Ed25519 | `ed25519-dalek` |
| Key Agreement | X25519 | `x25519-dalek` |
| Symmetric Encryption | ChaCha20-Poly1305 | `chacha20poly1305` |
| Canonical Encoding | CBOR (RFC 8949) | `ciborium` with custom canonicalization |

## Appendix B: Error Codes

```rust
pub enum KernelError {
    // Validation
    InvalidSignature,
    PayloadHashMismatch,
    UnsupportedVersion,
    MalformedReceipt,

    // Stream
    SequenceConflict { expected: u64, got: u64 },
    StreamForked { at_seq: u64 },
    InvalidPrevReceipt,

    // Storage
    StorageError(String),
    ReceiptNotFound(ReceiptId),

    // Sync
    ProtocolError(String),
    TransportError(String),

    // Permissions
    Unauthorized,
    GrantNotFound(ReceiptId),
    DecryptionFailed,
}
```

---

*This specification is the constitution of the Chainge Kernel. All implementations must conform to these rules to ensure interoperability and deterministic convergence.*
