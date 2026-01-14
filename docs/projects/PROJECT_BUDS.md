# Buds - One Pager

**Status**: Active Development (Phase 10.3 Module 6.5)
**Last Updated**: 2026-01-14
**Kernel Contribution**: Receipt-based distributed sync with E2E encryption and phone-based identity

---

## 1. Vision Alignment

Buds is a privacy-first memory journal that proves out the core kernel thesis: **users can own their memory while still sharing it**. The app demonstrates that E2E encrypted, cryptographically signed, locally-stored memories can sync across devices and share with trusted circles - without any platform having access to the content. Every state change is a signed receipt, making the entire history auditable and non-repudiable. The relay is a dumb pipe that assigns sequence numbers but never sees plaintext.

**Core insight**: Separate what's signed (content) from what's ordered (sequence). Clients sign receipts locally, relay assigns ordering atomically. This eliminates re-signing paradoxes and enables conflict-free sync.

---

## 2. Technical Architecture

```
iOS App (Swift/SwiftUI)
├── UI Layer: SwiftUI views (Shelf, Memory, Circle, Auth)
├── Manager Layer: JarManager, ShareManager, InboxManager, JarSyncManager
├── Kernel Layer: ReceiptManager, IdentityManager, E2EEManager
├── Database: GRDB/SQLite (ucr_headers, jars, jar_members, jar_receipts)
└── Transport: RelayClient (HTTP to Cloudflare Workers)

Cloudflare Relay (TypeScript/Hono)
├── Handlers: jarReceipts, messages, devices, account, lookup
├── Storage: D1 (metadata) + R2 (encrypted payloads)
└── Auth: Firebase phone verification middleware
```

**Key primitives**:
- **Identity**: `did:phone:SHA256(phone + account_salt)` - same DID across all user devices
- **Trust**: TOFU (Trust On First Use) - device pubkeys pinned on jar membership
- **Sync**: Relay envelope pattern - unsigned preimage signed by client, sequence assigned by relay
- **Crypto**: Ed25519 signing, X25519 key agreement, AES-256-GCM encryption, CBOR canonical encoding

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **Receipt** | UCRHeader + CBOR payload + Ed25519 signature | Immutable, signed state change - the atomic unit of memory |
| **Jar** | Shared space with 1-12 members | Permission boundary - who can see what |
| **Relay Envelope** | {sequence_number, receipt_cid, received_at} | Ordering metadata (not signed) - enables conflict-free sync |
| **Wrapped Key** | AES key encrypted per-device via X25519+HKDF | Multi-device E2EE without key escrow |
| **Tombstone** | Deletion marker with timestamp | Garbage collection + prevents deleted entity resurrection |

---

## 4. Data Flow Pattern

### Memory Creation & Sharing
```
User Input → ReceiptManager.createSessionReceipt() → Sign with Ed25519
                    ↓
              Store in ucr_headers (local)
                    ↓
              ShareManager.autoShareToJar()
                    ↓
              E2EEManager.encryptMessage() → Wrap AES key per device
                    ↓
              RelayClient.sendMessage() → Relay → R2 storage
                    ↓
              [30s polling] InboxManager → Decrypt → Store in recipient's jar
```

### Jar Receipt Sync (Distributed Consensus-Free Ordering)
```
Client creates receipt (jar.member_added, jar.bud_shared, etc.)
    ↓
Signs CBOR payload (WITHOUT sequence)
    ↓
POST /api/jars/:jarId/receipts
    ↓
Relay assigns sequence atomically:
    INSERT jar_receipts VALUES (..., COALESCE(MAX(seq)+1, 1), ...)
    ↓
Returns envelope {sequence_number, receipt_cid}
    ↓
All members poll receipts → JarSyncManager 5-step pipeline:
    [1] Replay protection
    [2] Tombstone check
    [3] Halt detection
    [4] Gap detection (queue if out-of-order)
    [5] Signature + CID verification
    [6] Apply receipt
    [7] Mark processed
    [8] Process queued receipts
```

---

## 5. Lessons Learned

**What worked:**
- **Unsigned preimage pattern**: Client signs content, relay assigns sequence. Eliminates chicken-and-egg where sequence must be known to sign but assigned after signature verification.
- **Phone-based DID**: `did:phone:SHA256(phone + salt)` cleanly solved multi-device identity. All devices with same phone number = same DID, each with unique keypairs.
- **TOFU with device broadcast**: On jar.member_added, all member devices are broadcast to existing members. Simple, deterministic, no PKI needed.
- **Canonical CBOR**: RFC 8949 canonicalization ensures signatures are verifiable cross-platform. Golden tests lock the encoding.
- **R2 for payloads**: Offloading encrypted blobs to object storage (R2) vs. database (D1) was critical for cost/scale. D1 for metadata, R2 for content.

**What didn't:**
- **Original device-as-DID model**: Early design gave each device a unique DID. This broke multi-device badly - you couldn't share with "a person," only "a device."
- **Client-side sequence assignment**: Initially tried client-assigned sequences with conflict resolution. Race conditions made this intractable. Relay ordering was the fix.
- **Polling interval tuning**: 30s polling is a UX compromise. Too slow for real-time feel, but WebSockets add complexity. Future: consider WebSocket upgrade for active users.

**Key insights:**
- **Separation of concerns in signing**: What you sign (content) and what orders it (sequence) are fundamentally different. Mixing them creates paradoxes.
- **Tombstones are first-class**: Deletion isn't the absence of data, it's a signed statement "this was deleted." Without tombstones, deleted entities can resurrect.
- **Gap handling is non-trivial**: Out-of-order receipts require queuing + backfill + poison detection. 5-retry limit + 7-day age limit prevents infinite queues.
- **E2EE multi-device is hard**: Each device needs its own key, each message must wrap the symmetric key for all recipient devices. Key rotation is still deferred.

---

## 6. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **CBORCanonical** | `ChaingeKernel/CBORCanonical.swift` | Direct extraction - RFC 8949 canonical encoder |
| **ReceiptManager** | `ChaingeKernel/ReceiptManager.swift` | Generalize beyond bud/jar types - universal receipt factory |
| **JarSyncManager** | `Core/JarSyncManager.swift` | The 5-step pipeline is generic - extract as ReceiptSyncEngine |
| **E2EEManager** | `Core/E2EEManager.swift` | Multi-device wrapping pattern is reusable |
| **Relay envelope pattern** | `buds-relay/handlers/jarReceipts.ts` | Direct extraction - atomic sequence assignment |
| **Phone-based DID** | `ChaingeKernel/IdentityManager.swift` | Generalize salt derivation for other identity anchors |

---

## 7. Open Questions / Future Work

- [ ] **Forward secrecy**: Current design uses static X25519 keys. Ratcheting (Double Ratchet) deferred to Phase 12
- [ ] **Key rotation**: What happens when a device is compromised? Revocation + re-encryption not implemented
- [ ] **Offline conflict resolution**: If two devices create jars offline with same name, no merge strategy exists
- [ ] **Metadata privacy**: Relay sees who messages who. Onion routing or mixnets could hide this
- [ ] **Blob storage**: Images/media are referenced but not yet E2E encrypted and synced
- [ ] **Account recovery**: If user loses all devices, no recovery path exists (by design, but UX problem)
- [ ] **Rate limiting hardening**: Current limits are placeholder, need real abuse modeling

---

## 8. Kernel Extraction Candidates

**Tier 1 - Ready to extract:**
1. **CBORCanonical**: Deterministic serialization is foundational. Used for all signing.
2. **CID computation**: CIDv1 (dag-cbor, sha2-256, base32) - standard content addressing.
3. **Relay envelope pattern**: Sequence assignment without re-signing. Generic pattern.

**Tier 2 - Needs generalization:**
4. **ReceiptManager → ReceiptFactory**: Abstract away bud/jar specifics, make type-agnostic.
5. **JarSyncManager → ReceiptSyncEngine**: The 5-step pipeline works for any receipt stream.
6. **E2EEManager → MultiDeviceEncryptor**: Key wrapping for N devices with TOFU.

**Tier 3 - Needs redesign:**
7. **IdentityManager**: Phone-based DID is one option. Kernel should support multiple identity anchors (passkeys, ENS, etc.).
8. **Permission model**: Jars are a specific permission primitive. Kernel needs capability-based access control.

---

## 9. Security Model Summary

| Property | Implementation | Gaps |
|----------|----------------|------|
| **Confidentiality** | AES-256-GCM E2EE | No forward secrecy |
| **Integrity** | Ed25519 signed receipts | Relay could drop/reorder (detectable via gaps) |
| **Authenticity** | TOFU device pinning | No revocation, compromised device persists |
| **Non-repudiation** | All mutations are signed receipts | Log is append-only |
| **Availability** | Local-first, relay is cache | Relay downtime blocks sharing |

---

## 10. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| iOS | Swift 6 + SwiftUI | Native performance, modern concurrency |
| Database | GRDB (SQLite) | Local-first, proven, migrations |
| Crypto | CryptoKit | Native Apple, Ed25519/X25519/AES-GCM |
| Serialization | SwiftCBOR 0.4.5 | **Locked** - golden tests prevent signature breaks |
| Backend | Cloudflare Workers | Edge compute, global latency, cheap |
| Storage | D1 + R2 | SQLite (metadata) + object storage (payloads) |
| Auth | Firebase Phone Auth | Frictionless phone verification |

---

## 11. Why This Matters for the Kernel

Buds proves several kernel hypotheses:

1. **Receipts work**: Every state change as a signed receipt creates an auditable, non-repudiable history. No central authority needed.

2. **E2EE at scale is tractable**: Multi-device, multi-recipient encryption with TOFU is implementable without PKI infrastructure.

3. **Relay ordering solves consensus**: By separating signing (client) from ordering (relay), we avoid the need for distributed consensus while maintaining causal ordering.

4. **Phone-based DID is viable**: For consumer apps, phone numbers are the natural identity anchor. The `did:phone` scheme works.

5. **Local-first + sync is the model**: Data lives on user devices, relay is a dumb pipe for coordination. Users own their memory.

**The gap**: Buds is a single-app kernel. The real kernel must be app-agnostic - any app should be able to write receipts, any app should be able to read them (with permission). That's the next frontier.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
