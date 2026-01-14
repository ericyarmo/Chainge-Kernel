# Repo Analysis Framework

**Purpose**: Standardized template for documenting projects that contribute to the kernel vision - making memory portable, permissioned, verifiable, and reducing coordination costs to zero.

---

## One-Pager Template

### Header Block
```
# [PROJECT_NAME] - One Pager
Status: [Active Development | Paused | Archive | Production]
Last Updated: [Date]
Kernel Contribution: [Primary contribution to memory physics]
```

### 1. Vision Alignment (2-3 sentences)
How does this project advance the goal of:
- **Portable memory**: Data moves with the user, not locked to platforms
- **Permissioned memory**: User controls who sees what, cryptographically enforced
- **Verifiable memory**: Every state change is signed, auditable, non-repudiable
- **Zero-cost coordination**: Atomic interoperability without intermediary rent-seeking

### 2. Technical Architecture
```
[Architecture Type]
├── [Layer 1]: [Description]
├── [Layer 2]: [Description]
├── [Layer 3]: [Description]
└── [Storage]: [Where data lives]
```

Key primitives:
- **Identity model**: How users/devices are identified
- **Trust model**: How trust is established and verified
- **Sync model**: How state propagates across nodes
- **Crypto model**: What's signed, what's encrypted, what's in the clear

### 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| [Name] | [How it's built] | [Why it matters] |

### 4. Data Flow Pattern
```
[Input] → [Transform 1] → [Transform 2] → [Output]
           ↓                ↓
       [Side Effect 1]  [Side Effect 2]
```

### 5. Lessons Learned

**What worked:**
- [Pattern/decision that proved correct]

**What didn't:**
- [Pattern/decision that needed revision]

**Key insights:**
- [Non-obvious learning that transfers to other projects]

### 6. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| [Name] | [Path] | [How it could be extracted] |

### 7. Open Questions / Future Work
- [ ] [Unsolved problem]
- [ ] [Deferred complexity]
- [ ] [Known limitation]

### 8. Kernel Extraction Candidates
Components that could become part of the universal memory kernel:
1. [Component]: [Why it's kernel-worthy]

---

## Analysis Checklist

When analyzing a new repo, answer:

### Identity & Trust
- [ ] How are entities identified? (DID, pubkey, UUID, etc.)
- [ ] What's the trust model? (TOFU, PKI, web of trust, central authority)
- [ ] How are keys managed? (Keychain, HSM, derived, custodial)

### Cryptographic Foundations
- [ ] What's signed? (All mutations? Just commits? Nothing?)
- [ ] What's encrypted? (At rest? In transit? E2E?)
- [ ] Is there forward secrecy?
- [ ] Is canonicalization deterministic?

### State & Sync
- [ ] Where is authoritative state? (Local-first? Server-authoritative? Consensus?)
- [ ] How do conflicts resolve? (LWW, CRDT, manual, rejection)
- [ ] What's the consistency model? (Eventual? Strong? Causal?)

### Interoperability
- [ ] What formats are used? (JSON, CBOR, Protobuf, custom)
- [ ] What protocols are spoken? (HTTP, WebSocket, custom)
- [ ] How hard is it to add a new client/node?

### Economics
- [ ] What are the coordination costs?
- [ ] Who pays for storage/compute/bandwidth?
- [ ] What's the scaling model?

---

## Kernel Primitives Taxonomy

### Level 0: Cryptographic Primitives
- Signing (Ed25519, ECDSA)
- Key agreement (X25519, ECDH)
- Symmetric encryption (AES-GCM, ChaCha20-Poly1305)
- Hashing (SHA-256, BLAKE3)
- Content addressing (CID, hash-linking)

### Level 1: Identity Primitives
- DIDs (phone-based, key-based, federated)
- Device identity (multi-device, TOFU)
- Key rotation / revocation
- Account recovery

### Level 2: Receipt Primitives
- Canonical encoding (deterministic serialization)
- Signed receipts (non-repudiable state changes)
- Content-addressed storage (immutable blobs)
- Chain linking (parent CID → causal ordering)

### Level 3: Sync Primitives
- Sequence assignment (relay ordering vs. client ordering)
- Gap detection (out-of-order handling)
- Backfill (catch up on missed state)
- Tombstones (deletion markers)

### Level 4: Permission Primitives
- Capability tokens
- Encrypted groups (multi-recipient wrapping)
- Revocation (key rotation, member removal)
- Audit trails (who accessed what when)

### Level 5: Coordination Primitives
- Atomic multi-party operations
- Escrow patterns
- Commit-reveal schemes
- Consensus (if needed)

---

## Project Classification

### By Trust Model
1. **Fully Sovereign**: User holds all keys, no trusted third parties
2. **Relay-Assisted**: Metadata visible to relay, content E2E encrypted
3. **Custodial**: Third party holds keys (not kernel-aligned)

### By Sync Model
1. **Local-first**: Works offline, syncs when possible
2. **Sync-required**: Needs network for core function
3. **Real-time**: Requires persistent connection

### By Maturity
1. **Proof of concept**: Core idea demonstrated
2. **Alpha**: Feature incomplete, architecture unstable
3. **Beta**: Feature complete, hardening needed
4. **Production**: Battle-tested, ready for real use

---

## Document Naming Convention

```
/Developer/
├── REPO_ANALYSIS_FRAMEWORK.md    # This document
├── PROJECT_[NAME].md              # Individual project one-pagers
│   Examples:
│   ├── PROJECT_BUDS.md
│   ├── PROJECT_CHAINGE_WEBSITE.md
│   └── PROJECT_[FUTURE].md
```

---

## Version History
- v1.0 (2026-01-14): Initial framework based on Buds analysis
