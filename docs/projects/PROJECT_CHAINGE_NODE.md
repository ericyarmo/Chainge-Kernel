# ChaingeNode - One Pager

**Status**: Kernel Complete (51% of tickets, Nov 2025) - UI remaining
**Last Updated**: 2026-01-14
**Kernel Contribution**: **53ms receipt generation** via NFC - proving memory physics can be FAST

---

## 1. Vision Alignment

ChaingeNode is a **witness device** - an iOS app that generates cryptographic receipts when someone taps an NFC wristband. The use case: civic participation (think community events, youth programs, volunteer check-ins). Tap your wristband, get a verifiable receipt that you were there.

**The thesis proven here**: Memory creation can be **instantaneous**. Sub-100ms end-to-end. <30ms perceived latency. Fast enough that it feels like magic - tap and you're done.

This matters because memory physics must be frictionless. If creating a receipt takes seconds, people won't do it. ChaingeNode proves you can have cryptographic integrity AND instant feedback.

**Privacy architecture**: ROOT_ID (your persistent identity) never leaves your device. TEMP_ID (written to NFC tags) is meaningless without the local mapping. You own your memory, nobody else can correlate it.

---

## 2. Technical Architecture

```
iOS App (Pure Swift, Zero Dependencies)
├── App Layer
│   └── ChaingeNodeApp.swift    # Pre-warms haptics + audio at launch
├── Services (1078 LOC)
│   ├── NFCService.swift        # CoreNFC read/write (268 LOC)
│   ├── DatabaseService.swift   # SQLite, 0.92ms inserts (304 LOC)
│   ├── ReceiptEngine.swift     # SHA256 signing (104 LOC)
│   ├── IdentityService.swift   # ROOT_ID ↔ TEMP_ID mapping (93 LOC)
│   ├── HapticEngine.swift      # Pre-initialized Taptic Engine (49 LOC)
│   └── AudioEngine.swift       # Pre-loaded sounds (51 LOC)
├── Models
│   ├── Receipt.swift           # UCR (Universal Civic Receipt)
│   ├── Identity.swift          # ROOT_ID + TEMP_ID
│   ├── TagPayload.swift        # 65-byte NFC payload
│   └── MemExport.swift         # .mem portable format
└── Views (UI - in progress)
```

**Key primitives**:
- **Identity**: Split model - ROOT_ID (persistent, private) + TEMP_ID (ephemeral, per-tag)
- **Trust**: SHA256 integrity hashes (not asymmetric signatures - good enough for v0.1)
- **Sync**: Local-only (no cloud), .mem export for portability
- **Storage**: SQLite with prepared statements, WAL mode, indexed lookups

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **Receipt (UCR)** | JSON with id, temp_id, root_id, timestamp, signature | Verifiable claim of participation |
| **ROOT_ID** | Persistent UUID, never leaves device | Your true identity |
| **TEMP_ID** | Ephemeral UUID, written to NFC tags | Public identifier (meaningless alone) |
| **Node** | The witnessing device | Creates + signs receipts |
| **TagPayload** | 65-byte JSON on NFC chip | Minimal data for fastest reads |
| **.mem Export** | JSON bundle of receipts | Portable memory file |

---

## 4. Performance Numbers (The Star of the Show)

### Measured Latency (Nov 25, 2025)

| Component | Specification | Actual | Factor |
|-----------|---------------|--------|--------|
| **Perceived latency** | <30ms | <30ms | On spec |
| **Total end-to-end** | <110ms | **53ms** | **2x faster** |
| **Database insert** | <25ms | **0.92ms** | **27x faster** |
| **Identity lookup** | <10ms | <5ms | 2x faster |
| **NFC read** | 40-60ms | 40-50ms | On spec |

### How This Speed is Achieved

**1. Two-Phase Architecture (Perception vs. Reality)**
```
Phase 1 (0-30ms): SYNCHRONOUS on main thread
├── Haptic feedback (pre-warmed UIImpactFeedbackGenerator)
├── Sound (pre-loaded .caf file)
└── Visual flash (green overlay, 200ms animation)

Phase 2 (30-53ms): ASYNC background
├── NFC tag read (40-50ms)
├── Identity lookup (<5ms)
├── Receipt creation + signing (<5ms)
└── Database insert (0.92ms)
```

User feels Phase 1 instantly. Phase 2 completes before they notice.

**2. Pre-Initialization at App Launch**
```swift
// ChaingeNodeApp.swift - runs at startup
HapticEngine.shared.prepare()     // Warm Taptic Engine (saves ~100ms)
AudioEngine.shared.loadTapSound() // Pre-load audio (saves ~50ms)
DatabaseService.shared.prepareStatements() // Compile SQL once
```

**3. Direct Memory Read (Skip NDEF Parsing)**
```swift
// Bypass NDEF message parsing overhead (saves 50-100ms)
tag.readNDEF { message in
    // Direct JSON decode, no NDEF traversal
    let payload = try JSONDecoder().decode(TagPayload.self, from: data)
}
```

**4. Hardware-Accelerated Crypto**
```swift
// CryptoKit uses Secure Enclave, not CPU
import CryptoKit
let hash = SHA256.hash(data: payload)
```

**5. Prepared Statements + Indexes**
```swift
// Compiled once at startup
insertReceiptStmt = try db.prepare("INSERT INTO receipts ...")
// Indexed columns
CREATE INDEX idx_temp_id ON receipts(temp_id);
CREATE INDEX idx_root_id ON receipts(root_id);
```

---

## 5. Data Flow Pattern

### NFC Tap → Receipt (53ms total)

```
NFC Tag Detected (CoreNFC hardware interrupt)
    ↓
[0-30ms] IMMEDIATE FEEDBACK (synchronous)
├── HapticEngine.tap() → Taptic Engine fires
├── AudioEngine.playTap() → Pre-loaded sound
└── VisualFeedback.flash() → Green overlay
    ↓
[30-50ms] NFC READ (async)
└── tag.readNDEF() → TagPayload { v:1, id:UUID, t:"chainge" }
    ↓
[50-55ms] IDENTITY LOOKUP (async)
└── IdentityService.lookup(tempId) → rootId
    ↓
[55-58ms] RECEIPT CREATION (async)
├── Generate receipt UUID
├── Capture timestamp
├── Sign: SHA256(id:tempId:rootId:timestamp)
└── Build Receipt struct
    ↓
[58-59ms] DATABASE INSERT (async)
└── db.execute(insertReceiptStmt) → 0.92ms
    ↓
DONE (53ms total, <30ms perceived)
```

---

## 6. Lessons Learned

**What worked:**
- **Two-phase feedback**: Decoupling perception from completion is the key insight. Users don't care about async - they care about feeling fast.
- **Pre-warming everything**: Haptic engine, audio, SQL statements. Cold start is death.
- **Zero dependencies**: Pure Swift + system frameworks. No CocoaPods, no SPM packages. Ship what you control.
- **Direct NFC read**: Skipping NDEF parsing saves 50-100ms. Read raw memory, decode yourself.
- **Split identity (ROOT_ID/TEMP_ID)**: Privacy by architecture. TEMP_ID on tag is useless without local mapping.

**What was rough:**
- **NFC is slow**: 40-50ms is hardware-limited. Can't optimize further without custom silicon.
- **SHA256 vs Ed25519**: Current signatures are integrity hashes, not asymmetric. Good enough for v0.1, but not cryptographically binding to an identity.
- **No cloud sync**: Local-only means receipts don't survive device loss. .mem export is the mitigation.

**Key insights:**
- **Perception > reality**: 30ms perceived beats 100ms actual. Design for human perception first.
- **Memory physics must be instant**: If receipt creation is slow, adoption dies. 53ms is table stakes.
- **Privacy via architecture**: ROOT_ID/TEMP_ID split means no server needs to know your identity.

---

## 7. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **Two-phase feedback pattern** | `NFCService.swift` | Universal - any tap/scan interaction |
| **HapticEngine** | `Services/HapticEngine.swift` | Direct extraction - pre-warmed haptics |
| **AudioEngine** | `Services/AudioEngine.swift` | Direct extraction - pre-loaded sounds |
| **DatabaseService** | `Services/DatabaseService.swift` | Prepared statement pattern, WAL mode |
| **ReceiptEngine** | `Services/ReceiptEngine.swift` | SHA256 signing (upgrade to Ed25519) |
| **.mem export format** | `Models/MemExport.swift` | Portable receipt bundle spec |
| **Identity split** | `Models/Identity.swift` | ROOT_ID/TEMP_ID pattern |

---

## 8. Open Questions / Future Work

- [ ] **Asymmetric signatures**: Upgrade SHA256 hashes to Ed25519 for cryptographic binding
- [ ] **Merkle trees**: Batch verification for multi-receipt exports
- [ ] **Cloud sync**: Optional backup (E2EE encrypted ROOT_ID mapping)
- [ ] **Multi-node witnessing**: Same person taps at multiple events, receipts combine
- [ ] **Timestamp anchoring**: Use a timestamping service for non-repudiation
- [ ] **Android port**: CoreNFC → Android NFC API
- [ ] **Binary NFC payload**: Shrink 65 bytes → 21 bytes for even faster reads

---

## 9. Kernel Extraction Candidates

**Tier 1 - Ready to extract:**
1. **Two-phase feedback pattern**: Perception/completion split. Universal.
2. **HapticEngine + AudioEngine**: Pre-warmed feedback services.
3. **Prepared statement pattern**: 27x speedup on DB inserts.
4. **.mem export format**: Portable receipt bundles.

**Tier 2 - Needs generalization:**
5. **ReceiptEngine**: Upgrade to Ed25519, make receipt type configurable.
6. **ROOT_ID/TEMP_ID split**: Generalize to any identity system.
7. **NFCService**: Abstract NFC layer for multi-platform.

**Tier 3 - Hardware-specific:**
8. **CoreNFC integration**: iOS-only, but pattern is portable.
9. **Direct memory read**: Optimization that skips NDEF parsing.

---

## 10. Security Model

| Property | Implementation | Gaps |
|----------|----------------|------|
| **Integrity** | SHA256 of receipt payload | Not bound to identity (no asymmetric sig) |
| **Privacy** | ROOT_ID never leaves device | TEMP_ID correlation possible if attacker has tag |
| **Availability** | Local SQLite, .mem export | No cloud backup (by design) |
| **Compliance** | COPPA (no PII), GDPR (local-first) | Solid |

**Future security upgrades:**
- Ed25519 signatures (bind receipts to ROOT_ID cryptographically)
- Merkle tree proofs (batch verification)
- Timestamp anchoring (notarization)

---

## 11. Why This Matters for the Kernel

ChaingeNode proves several kernel hypotheses:

1. **Memory physics can be FAST**: 53ms end-to-end, <30ms perceived. No excuses for slow receipt creation.

2. **Perception > completion**: Two-phase architecture is the pattern. Fire feedback immediately, finish work async.

3. **Privacy via architecture**: ROOT_ID/TEMP_ID split means identity stays local. No server correlation possible.

4. **Zero dependencies is achievable**: Pure Swift + system frameworks. Ship what you control.

5. **Hardware constraints are real**: NFC is 40-50ms, hardware-limited. Design around it, not against it.

**The innovation**: This is the fastest receipt generation in the portfolio. If civic participation requires a tap, that tap must feel instant. ChaingeNode proves it's possible.

---

## 12. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Platform | iOS 13.0+ | NFC requires iPhone 7+ hardware |
| Language | Swift 5.9 | Pure, no dependencies |
| NFC | CoreNFC | Apple's native NFC framework |
| Database | SQLite3 | System framework, prepared statements |
| Crypto | CryptoKit | Hardware-accelerated, Secure Enclave |
| UI | SwiftUI | Modern, declarative |
| Haptics | UIKit | UIImpactFeedbackGenerator |

---

## 13. Tag Specifications

| Tag Type | Usable Bytes | Cost | Notes |
|----------|--------------|------|-------|
| NTAG213 | 144 bytes | $0.08-$0.15 | Minimum viable |
| NTAG215 | 504 bytes | ~$0.15 | **Tested, verified** |
| MIFARE Classic 1K | 1024 bytes | $0.24-$0.50 | More storage |

**Current payload (65 bytes)**:
```json
{"v":1,"id":"550e8400-e29b-41d4-a716-446655440000","t":"chainge"}
```

**Binary-optimized (21 bytes)**: Version (1) + UUID (16) + Type (4)

---

## 14. The Witness Model

```
Person (ROOT_ID - persistent, private)
    │
    ├── Tag 1 (TEMP_ID_1 - ephemeral, on wristband)
    ├── Tag 2 (TEMP_ID_2 - ephemeral, on keychain)
    └── Tag N

Node (witness device - phone running ChaingeNode)
    │
    └── Creates receipts when tags are tapped

Receipt
    ├── Proves: TEMP_ID was witnessed by NODE at TIME
    ├── Signed: SHA256(id:temp_id:root_id:timestamp)
    └── Stored: Local SQLite, exportable via .mem
```

**The insight**: Multiple tags can map to one ROOT_ID. Multiple nodes can witness the same tag. Receipts compose across time and space.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
*53ms end-to-end, <30ms perceived - memory physics at human speed*
