# Chainge Kernel

**Portable. Permissioned. Verifiable. Memory you own.**

The Chainge Kernel is infrastructure for memory that belongs to people, not platforms.

---

## The Problem

Your memory is trapped. Betting history in DraftKings. Health records scattered across hospitals. Reputation locked in platforms that can delete you. The cost of interoperability isn't high - it's infinite, because platforms don't want it to exist.

## The Solution

**Receipts** - cryptographically signed claims that something happened.

```
Receipt = {
  cid: content_address,      // What
  did: identity,             // Who
  signature: ed25519(cbor),  // Proof
  timestamp: when,           // When
}
```

Receipts are:
- **Portable** - They move with you
- **Permissioned** - You control who sees what
- **Verifiable** - Cryptographically signed, auditable
- **Composable** - Combine into larger truths

## Status

**Far from complete. Absolutely proven.**

This kernel has been implemented 8 different ways across 5 languages in 5 months:

| Project | Domain | Innovation |
|---------|--------|------------|
| [TheMove](docs/projects/PROJECT_THEMOVE.md) | Anonymous social | Attribution without identity |
| [Buds](docs/projects/PROJECT_BUDS.md) | Private memory | E2EE + relay sync |
| [Streams](docs/projects/PROJECT_STREAMS.md) | Betting reputation | Public signed receipts |
| [ChaingeNode](docs/projects/PROJECT_CHAINGE_NODE.md) | Civic participation | 53ms NFC receipts |
| [Frontier Index](docs/projects/PROJECT_FRONTIER_INDEX.md) | AI knowledge | Deterministic reasoning |
| [Replay Kernel](docs/projects/PROJECT_REPLAY_KERNEL.md) | Gaming | Teachable in 1 week |

Read the full story: **[docs/MANIFESTO.md](docs/MANIFESTO.md)**

## Architecture

```
┌─────────────────────────────────────────┐
│            Applications                 │
├─────────────────────────────────────────┤
│            Receipt Layer                │
│   Sign → Hash → Store → Sync → Verify   │
├─────────────────────────────────────────┤
│            Identity Layer               │
│        DIDs, keys, devices              │
├─────────────────────────────────────────┤
│          Cryptographic Layer            │
│   Ed25519, X25519, AES-GCM, CBOR, CID   │
└─────────────────────────────────────────┘
```

## This Implementation (Rust)

```
crates/
├── chainge-kernel-core   # Receipt, CID, signatures
├── chainge-kernel-store  # SQLite persistence
├── chainge-kernel-sync   # Relay protocol
├── chainge-kernel-perms  # Capabilities, encryption
└── chainge-kernel        # Unified API
```

```bash
cargo build
cargo test
```

## Documentation

- [Manifesto](docs/MANIFESTO.md) - The full story (start here)
- [Spec v0](SPEC_V0.md) - Technical specification
- [Analysis Framework](docs/ANALYSIS_FRAMEWORK.md) - How projects are documented
- [Project Docs](docs/projects/) - Individual project deep-dives

## A Note on How This Was Built

Every line of code across all implementations was written via LLM pair programming. First code ever: August 2025. Total elapsed: 5 months. This is proof that:

1. The concepts are language-agnostic
2. LLMs compress learning
3. The patterns compound
4. Serious infrastructure can be built this way

## License

MIT OR Apache-2.0

## Contact

Eric Yarmolinsky
January 2026

---

*"Make a Chainge. Remember the Future."* - Eric Yarmo
