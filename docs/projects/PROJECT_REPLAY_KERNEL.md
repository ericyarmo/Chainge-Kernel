# Replay Memory Kernel (Nolan Internship) - One Pager

**Status**: MVP Complete (Days 1-6 of 8)
**Last Updated**: 2026-01-14
**Kernel Contribution**: **PROOF OF TEACHABILITY** - High school intern implements receipt chains in Java (a language the mentor doesn't know)

---

## 1. Vision Alignment

This project proves the kernel concepts are **teachable, language-agnostic, and applicable to new domains**. In one week, a high school intern built:

- Cryptographic receipt chains
- SHA-256 hash linking (blockchain-style)
- Tamper detection via audit
- Configurable sampling (80-90% storage savings)
- 19 passing tests

All in **Java** - a language the mentor doesn't even know. The teaching was conceptual ("here's what a receipt is, here's why hashing matters"), and the intern translated it to working code.

**Domain**: Minecraft event logging. Every player movement becomes a receipt. The chain proves "this sequence of events happened exactly this way, and nobody tampered with it." Real-world parallel: CoreProtect (anti-grief plugin) uses identical patterns.

**The meta-proof**: If a high school student can learn and implement receipt physics in a week, the concepts are ready for broader adoption.

---

## 2. Technical Architecture

```
Java 17 + Gradle
├── cli/
│   └── ChaingeCommand.java      # record | replay | audit
├── model/
│   ├── Receipt.java             # Header + payload
│   └── ReceiptHeader.java       # v, tick, seq, kind, hashes
├── crypto/
│   ├── Sha256.java              # SHA-256 wrapper
│   ├── Hex.java                 # Hex encoding
│   └── ReceiptHasher.java       # Hash computation
├── deterministic/
│   └── DeterministicEncoding.java  # Byte-perfect serialization
├── events/
│   └── PoseEvent.java           # x, y, z, yaw, pitch
├── io/
│   ├── JsonlWriter.java         # JSONL output
│   └── JsonlReader.java         # JSONL parsing
├── recording/
│   └── ReceiptChain.java        # Chain management
├── replay/
│   └── SessionReplayer.java     # Playback display
├── audit/
│   └── SessionAuditor.java      # Integrity verification
└── config/
    └── SamplingConfig.java      # Sample rate control
```

**Dependencies**: Just Gson (JSON) + JUnit (testing). No blockchain libraries.

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **Receipt** | Header + escaped JSON payload | Immutable event record |
| **ReceiptHeader** | v, tick, seq, kind, payloadHash, receiptHash, prevHash | Cryptographic envelope |
| **ReceiptChain** | Linked list via prevHash | Tamper-evident sequence |
| **PoseEvent** | x, y, z, yaw, pitch | Minecraft player position |
| **SamplingConfig** | Sample rate (1, 5, 10...) | Storage optimization |

---

## 4. Receipt Structure

```json
{
  "header": {
    "v": 1,
    "tick": 0,
    "seq": 0,
    "kind": "Pose",
    "payloadHash": "22d3acbf...",
    "receiptHash": "c24083ac...",
    "prevHash": null
  },
  "payloadJson": "{\"x\":0.0,\"y\":64.0,\"z\":0.0,\"yaw\":0.0,\"pitch\":-15.0}"
}
```

**Key fields**:
- `tick`: Minecraft game tick (20/sec)
- `seq`: Monotonic sequence number
- `kind`: Event type (extensible)
- `payloadHash`: SHA-256 of payload bytes
- `receiptHash`: SHA-256 of (header + payloadHash + prevHash)
- `prevHash`: Previous receipt's hash (blockchain-style linking)

---

## 5. Hash Chain (The Core Innovation)

```
Receipt 0 (prevHash: null)
    payloadHash = SHA256("{x:0.0,y:64.0,z:0.0,...}")
    receiptHash = SHA256(header || payloadHash || null)
         ↓
Receipt 1 (prevHash: receiptHash[0])
    payloadHash = SHA256("{x:0.5,y:64.0,z:0.3,...}")
    receiptHash = SHA256(header || payloadHash || receiptHash[0])
         ↓
Receipt 2 (prevHash: receiptHash[1])
    ...
```

**Tamper detection**: If ANY byte changes in ANY receipt:
1. Payload hash breaks
2. Receipt hash breaks
3. All subsequent receipts break (they reference prevHash)
4. Audit fails immediately

---

## 6. Commands

```bash
# Record 10 simulated gameplay events
./gradlew run --args="record"
# Output: session.jsonl

# Replay recorded session
./gradlew run --args="replay session.jsonl"
# Output: Human-readable event display

# Verify integrity
./gradlew run --args="audit session.jsonl"
# Output: "All 10 receipts verified. Chain is valid."

# Record with sampling (80% storage savings)
./gradlew run --args="record --sample-rate 5"
# Output: Only every 5th tick recorded
```

---

## 7. The Teaching Moment: DAY_3.5

**The bug**: Day 3 stored payload as nested JSON object. JsonlReader parsed it with Gson, then called `.toString()`. But Gson doesn't guarantee field order. Hash verification failed on valid files.

**The fix**: Store payload as **escaped string**, never re-parse. This preserves exact bytes for deterministic hashing.

**The lesson**: This is why the real kernel uses **canonical CBOR** (RFC 8949) - JSON serialization is not deterministic. The intern learned this the hard way and now understands why canonicalization matters.

```java
// WRONG: Nested object (field order not preserved)
{ "header": {...}, "payload": {"x": 0.0, "y": 64.0} }

// RIGHT: Escaped string (exact bytes preserved)
{ "header": {...}, "payloadJson": "{\"x\":0.0,\"y\":64.0}" }
```

---

## 8. Sampling Optimization

**Problem**: Minecraft runs at 20 ticks/sec. 1 hour = 72,000 receipts.

**Solution**: Configurable sampling rate.

```java
shouldRecord(tick) = (tick % sampleRate == 0)
```

| Sample Rate | 1 Hour | Storage Savings |
|-------------|--------|-----------------|
| 1 (every tick) | 72,000 receipts | 0% |
| 5 (every 5th) | 14,400 receipts | 80% |
| 10 (every 10th) | 7,200 receipts | 90% |

**Key insight**: Even with sampling, hash chain remains valid. `seq` stays sequential, `tick` shows actual tick numbers. Audit still works.

---

## 9. Lessons Learned

**What worked:**
- **Daily plans with clear goals**: Each day had a focused objective (record, replay, audit, sample)
- **Test-first verification**: Golden vectors proved hashing was correct before building more
- **Progressive complexity**: Start simple (Pose events), add features (sampling, multiple event types)
- **Real bug as teaching**: DAY_3.5 determinism bug taught more than any lecture could

**What the intern learned:**
- Cryptographic hashing (SHA-256, hex encoding)
- Hash chains (blockchain fundamentals)
- Deterministic serialization (why JSON isn't enough)
- Test-driven development (19 tests, all passing)
- Trade-off analysis (sampling rate vs. fidelity)

**What this proves about the kernel:**
- Concepts are **language-agnostic** (Java, Swift, TypeScript - same pattern)
- Concepts are **teachable** (high school intern, one week)
- Concepts are **applicable** (Minecraft is a real use case)

---

## 10. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **Hash chain pattern** | `ReceiptChain.java` | Universal - same as Buds/Streams |
| **Deterministic encoding** | `DeterministicEncoding.java` | Template for any language |
| **Audit logic** | `SessionAuditor.java` | Generic receipt verification |
| **Sampling config** | `SamplingConfig.java` | Trade-off pattern |
| **JSONL format** | `JsonlWriter/Reader.java` | Human-readable receipt storage |

---

## 11. Test Coverage

**19 tests, all passing:**

| Test Class | Tests | Purpose |
|------------|-------|---------|
| `GoldenVectorsTest` | 1 | Byte-perfect hash verification |
| `ReceiptChainTest` | 6 | Chain creation, linking, sampling |
| `JsonlReaderTest` | 3 | File parsing, sequence validation |
| `SessionAuditorTest` | 5 | Valid audit, tamper detection |
| `SamplingConfigTest` | 4 | Rate validation |

**Golden test example:**
```java
// Known input → known output (any byte change fails)
String payload = "{\"x\":0.0,\"y\":64.0,\"z\":0.0}";
String expected = "22d3acbf..."; // Pre-computed
assertEquals(expected, Sha256.hashHex(payload.getBytes()));
```

---

## 12. Kernel Extraction Candidates

**Tier 1 - Direct port to other languages:**
1. **Hash chain pattern**: `prevHash` linking is universal
2. **Deterministic encoding**: Byte-perfect serialization
3. **Audit logic**: Verify hash chain integrity

**Tier 2 - Conceptual patterns:**
4. **Sampling trade-off**: Storage vs. fidelity is domain-agnostic
5. **Event extensibility**: Add new event types without core changes
6. **JSONL format**: Human-readable, streaming-friendly

---

## 13. Why This Matters for the Kernel

This project proves **three critical things**:

### 1. Kernel concepts are teachable
A high school intern with no prior cryptography experience implemented:
- SHA-256 hash chains
- Tamper detection
- Deterministic serialization
- Configurable sampling

In one week. With clear daily plans and progressive complexity.

### 2. Kernel concepts are language-agnostic
The mentor doesn't know Java. The teaching was:
- "A receipt is a signed claim that something happened"
- "Hash chains link receipts together"
- "Changing any byte breaks the chain"

The intern translated these concepts to working Java code.

### 3. Kernel concepts apply to new domains
Minecraft event logging isn't personal memory or betting reputation. It's **gameplay integrity**. Same pattern:
- Events happen (player moves)
- Events become receipts (hashed, timestamped)
- Receipts chain together (prevHash linking)
- Tampering is detectable (audit)

**Real-world application**: CoreProtect (Minecraft anti-grief plugin) uses exactly this pattern to track "who placed/broke which block when."

---

## 14. The Teaching Arc

| Day | Focus | Outcome |
|-----|-------|---------|
| 1 | Hashing fundamentals | Golden vectors passing |
| 2 | Chain logic | ReceiptChain creating linked receipts |
| 3 | Replay system | JsonlReader + SessionReplayer working |
| 3.5 | **Bug fix** | Learned deterministic serialization |
| 4 | Audit command | Tamper detection working |
| 5-6 | Sampling | 80-90% storage savings |
| 7-8 | Polish | Multiple event types, documentation |

**The bug on Day 3.5 was the best lesson.** It taught why canonical encoding matters - something the intern will remember forever.

---

## 15. Code Statistics

| Category | Lines |
|----------|-------|
| Production code | ~600 |
| Test code | ~500 |
| Documentation | ~2,000 (daily plans, README) |
| **Total** | ~3,100 |

Clean, well-documented, portfolio-ready.

---

## 16. Demo Script (From DEMO_DAY.md)

```
1. Record (2 sec)
   ./gradlew run --args="record"
   → 10 receipts created

2. Replay (20 sec)
   ./gradlew run --args="replay session.jsonl"
   → Show tick-by-tick events

3. Audit (20 sec)
   ./gradlew run --args="audit session.jsonl"
   → "Chain is valid"

4. Tamper + Re-audit (15 sec)
   Edit session.jsonl (change x coordinate)
   ./gradlew run --args="audit session.jsonl"
   → "TAMPERED: payload hash mismatch at seq 3"

5. Tests (15 sec)
   ./gradlew test
   → 19/19 passing
```

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
*Proof: The kernel is teachable. One week. One intern. One language you don't know.*
