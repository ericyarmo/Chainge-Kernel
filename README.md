# Chainge Kernel

**The atom of verifiable memory.**

A receipt is a signed claim that something happened. The kernel stores and verifies receipts. Everything else emerges.

---

## The Receipt

```
Receipt {
    author:    [u8; 32]      // Ed25519 public key
    schema:    String        // URI identifying payload type
    refs:      Vec<[u8; 32]> // Links to other receipts (causal ordering)
    payload:   Bytes         // Opaque content (≤64KB)
    signature: [u8; 64]      // Ed25519 signature
}
```

Four semantic fields plus a signature. That's the atom.

---

## Properties

| Property | What it means |
|----------|---------------|
| **Signed** | Cryptographically proves who created it |
| **Immutable** | Can never be changed after creation |
| **Content-addressed** | ID is derived from content, not assigned |
| **Linkable** | refs create DAGs—chains, merges, witnesses |
| **Offline-first** | Sign and verify without network |
| **Portable** | Works anywhere, verified by anyone |

---

## Philosophy

The kernel is **common law for memory**.

| Layer | Analogy | What it is |
|-------|---------|------------|
| Kernel | Natural law | Axioms that can't be violated |
| Conventions | Common law | Patterns emerging from practice |
| Applications | Jurisdictions | Rules for interpretation |

The kernel doesn't say what's true. It says what was attested. Trust is computed by observers, not declared by the system.

See [ChaingeOS/08_PHILOSOPHY.md](../ChaingeOS/08_PHILOSOPHY.md) for the full frame.

---

## This Implementation (Rust)

```
crates/chainge-kernel/
├── src/
│   ├── lib.rs        # Public API
│   ├── receipt.rs    # Receipt struct, create/verify
│   ├── canonical.rs  # DAG-CBOR encoding
│   ├── crypto.rs     # Ed25519, SHA-256
│   ├── store.rs      # Store trait + MemoryStore
│   └── error.rs      # Error types
└── tests/
    └── golden.rs     # Golden test vectors
```

```bash
cargo build
cargo test   # 43 tests pass
```

---

## Golden Test Vectors

`tests/golden_vectors.json` contains 10 test vectors. Any implementation that produces identical outputs is kernel-compatible.

| Vector | Tests |
|--------|-------|
| empty_refs_empty_payload | Minimal case |
| empty_refs_with_payload | Payload encoding |
| single_ref | Reference encoding |
| two_refs_sorted | Refs normalization |
| many_refs | 8 refs |
| max_schema_length | 256-byte schema boundary |
| large_payload | 1KB payload |
| binary_payload | All 256 byte values |
| realistic_civic | Real JSON payload |
| chain_receipt | B references A |

---

## Documentation

| Document | Purpose |
|----------|---------|
| [SPEC.md](SPEC.md) | Kernel specification (v0.5.1) |
| [CONVENTIONS.md](CONVENTIONS.md) | Blessed patterns: chains, tombstones, heads |
| [docs/DAG_SEMANTICS.md](docs/DAG_SEMANTICS.md) | How refs create causal structure |
| [docs/MANIFESTO.md](docs/MANIFESTO.md) | Origin story |

---

## Prior Art

This kernel has been implemented across multiple projects:

| Project | Domain | What it proved |
|---------|--------|----------------|
| TheMove | Anonymous social | Attribution without identity |
| Buds | Private memory | E2EE + relay sync |
| Streams | Betting reputation | Public signed receipts |
| ChaingeNode | Civic participation | 53ms NFC receipts |
| Frontier Index | AI knowledge | Deterministic reasoning |

---

## License

MIT OR Apache-2.0

---

*"A receipt is a signed claim that something happened. That's the atomic unit. Everything else emerges."*
