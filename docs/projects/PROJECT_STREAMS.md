# Streams - One Pager

**Status**: TestFlight Ready (Dec 2025)
**Last Updated**: 2026-01-14
**Kernel Contribution**: Public receipt ledger with social mechanics - proving receipts work without E2EE complexity

---

## 1. Vision Alignment

Streams is a **player wallet for sports bettors** - a social app where users log wins, losses, promos, and referrals as cryptographically signed receipts. The core thesis: **betting reputation should be portable**. Your track record shouldn't be locked inside DraftKings or FanDuel - it should follow you, be verifiable, and be monetizable.

Unlike Buds (private, E2E encrypted), Streams is **public-first**. This is intentional: it tests whether the receipt primitive works in a social context where verification and reputation matter more than privacy. The simplicity (no jars, no E2EE, no distributed sync) makes it a cleaner test of the core kernel primitives.

**Core insight**: Receipts as the atomic unit of memory work for both private (Buds) and public (Streams) use cases. The same primitives (CBOR, CID, Ed25519) apply universally.

---

## 2. Technical Architecture

```
iOS App (Swift/SwiftUI)
├── UI Layer: SwiftUI views (Feed, Capture, Profile, Public)
├── Services: IdentityManager, ReceiptRepository, APIClient
├── Crypto: CID, CBOREncoder (custom minimal implementation)
├── Database: GRDB/SQLite (ucr_headers, local_receipts)
└── Auth: Firebase Phone Auth (optional upgrade from local DID)

Cloudflare Worker API (TypeScript)
├── Endpoints: /v1/receipts, /v1/feed, /v1/receipts/:id/fire, /v1/stats
├── Storage: D1 (SQLite)
├── Moderation: Spam detection, profanity logging, community reports
└── CORS: Allowlisted origins only

GetStreams (Next.js)
├── Landing page + App Store links
├── Receipt viewer /r/:cid (planned)
└── Community guidelines
```

**Key primitives**:
- **Identity**: `did:streams:<base58(pubkey)>` or `did:streams:local-<pubkey_prefix>` for anonymous
- **Trust**: Ed25519 signatures (client-side), verification exists but not enforced server-side yet
- **Sync**: Simple POST to relay, no sequence numbers, no distributed ordering
- **Crypto**: CBOR canonical encoding, CIDv1 (dag-cbor, sha2-256), Ed25519

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **UCRHeader** | CID + DID + payload + timestamp + signature | The receipt - immutable signed claim of betting event |
| **Receipt Types** | win, loss, pending, promo, referral, manual | Domain-specific betting events |
| **Fire** | Like/upvote mechanic | Social validation signal for receipts |
| **DID Upgrade** | Local keypair → Firebase cloud DID | Anonymous-first with optional identity upgrade |
| **LocalReceipt** | Mutable app-layer metadata | Favorites, tags, images (not part of signed receipt) |

---

## 4. Data Flow Pattern

### Receipt Creation (Local-First)
```
User fills CaptureView (amount, book, type, note)
    ↓
IdentityManager.createSignedHeader()
    ├─ Build payload dict
    ├─ Encode to canonical CBOR (sorted keys, no cid/signature)
    ├─ Compute CIDv1 = base32(0x01 + 0x71 + sha256(cbor))
    ├─ Sign CBOR with Ed25519
    └─ Assemble UCRHeader
    ↓
Store in GRDB (ucr_headers + local_receipts)
    ↓
APIClient.postReceipt() → Cloudflare Worker → D1
    ↓
FeedView displays from local DB (not waiting for server)
```

### Public Feed (Server-Mediated)
```
PublicFeedView.onAppear()
    ↓
APIClient.fetchPublicFeed(limit: 50)
    ↓
Worker queries D1: SELECT * FROM receipts WHERE is_hidden = 0 ORDER BY created_at DESC
    ↓
Returns JSON array of receipts
    ↓
UI renders receipt cards with fire counts
```

### Fire Reaction
```
User taps fire button
    ↓
APIClient.fireReceipt(id, firedByDID)
    ↓
Worker: INSERT INTO fires, UPDATE receipts SET fire_count = fire_count + 1
    ↓
Returns updated fire_count
    ↓
UI updates immediately (optimistic)
```

---

## 5. Lessons Learned

**What worked:**
- **Custom CBOR encoder**: Building a minimal 256-line CBOR encoder was cleaner than depending on SwiftCBOR. Full control over canonical encoding.
- **Anonymous-first with DID upgrade**: Users can use the app immediately with local keypair, optionally link phone later. No friction to first receipt.
- **Fire as social primitive**: Simple like/upvote mechanic creates engagement without complex social graph.
- **Moderation via reports + auto-hide**: 3 reports = auto-hidden. Community policing scales.

**What didn't:**
- **No server-side signature verification**: Verification code exists but isn't called. This is a trust hole for production.
- **Force unwraps in APIClient**: Crash risk on malformed URLs. Acceptable for 100 users, not 100K.
- **Database migration v2**: Duplicates a column - data loss risk on update.

**Key insights:**
- **Public receipts are simpler**: No E2EE, no key wrapping, no recipient management. Just sign and publish.
- **DID without phone binding has tradeoffs**: `did:streams:<pubkey>` is pure but loses multi-device. Phone-based DID (like Buds) solves this.
- **Receipts as reputation**: The betting domain makes receipt value obvious - your track record IS your identity.

---

## 6. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **CBOREncoder** | `Services/CID.swift` | Direct extraction - minimal 256-line canonical CBOR encoder |
| **CID.computeCIDv1DagCBOR** | `Services/CID.swift` | Universal - same as Buds implementation |
| **Base58** | `Utilities/Base58.swift` | Reusable encoding utility |
| **UCRHeader** | `Models/UCRHeader.swift` | Template for receipt structure |
| **Moderation patterns** | `worker/src/index.ts` | Spam detection, report system, auto-hide |

---

## 7. Open Questions / Future Work

- [ ] **Server-side signature verification**: The code exists but isn't called. Critical for production.
- [ ] **Multi-device sync**: Current design is single-device. Need phone-based DID like Buds.
- [ ] **Rate limiting**: TODO in worker - needs KV store for per-DID limits.
- [ ] **Screenshot OCR**: Auto-extract amount/book from betting app screenshots.
- [ ] **Leaderboards**: Weekly/monthly/all-time by profit, ROI, volume.
- [ ] **ATProto federation**: Publish receipts to Bluesky-compatible network.
- [ ] **Streak mechanics**: Daily streaks with push notifications (retention driver).
- [ ] **.mem export**: Portable receipt format for cross-app interop.

---

## 8. Kernel Extraction Candidates

**Tier 1 - Ready to extract:**
1. **CBOREncoder**: Clean 256-line canonical encoder. No dependencies.
2. **CID computation**: Same pattern as Buds, universal utility.
3. **Base58**: Standard encoding, no dependencies.

**Tier 2 - Needs generalization:**
4. **UCRHeader → GenericReceipt**: Abstract away betting-specific types.
5. **IdentityManager**: The DID upgrade/downgrade pattern is interesting - anonymous first, identity later.

**Tier 3 - Domain-specific (less reusable):**
6. **Fire reactions**: Social mechanic, not kernel-level.
7. **Moderation system**: App-specific policy, not protocol.

---

## 9. Comparison: Streams vs Buds

| Aspect | Streams | Buds |
|--------|---------|------|
| **Privacy model** | Public-first | Private-first (E2EE) |
| **DID scheme** | `did:streams:<pubkey>` | `did:phone:SHA256(phone+salt)` |
| **Multi-device** | Not solved | Solved via phone-based DID |
| **Sync model** | Simple POST, no ordering | Relay envelope with sequence |
| **Encryption** | None (public receipts) | AES-256-GCM + X25519 wrapping |
| **Social mechanics** | Fire reactions | Reactions + Jar sharing |
| **Complexity** | ~5K LOC | ~20K+ LOC |
| **Maturity** | TestFlight ready | Production hardened |

**Key difference**: Streams proves the receipt primitive works without the complexity of E2EE and distributed sync. It's a cleaner test case for "do receipts as memory units actually work?"

---

## 10. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| iOS | Swift 5.9 + SwiftUI | Native, modern |
| Database | GRDB (SQLite) | Same as Buds, proven |
| Crypto | CryptoKit | Native Apple, Ed25519 |
| CBOR | Custom encoder | 256 lines, no deps, full control |
| Backend | Cloudflare Workers | Edge compute, cheap |
| Storage | D1 | SQLite at edge |
| Auth | Firebase Phone | Optional identity upgrade |

---

## 11. Why This Matters for the Kernel

Streams validates kernel hypotheses in a **simpler context**:

1. **Receipts work for public memory**: Same CBOR + CID + signature pattern works when privacy isn't required. The primitive is universal.

2. **DID without phone works (with tradeoffs)**: Pure `did:<method>:<pubkey>` is simple but breaks multi-device. Phone-based DIDs (Buds) solve this but add dependency.

3. **Social mechanics layer on top**: Fire reactions, leaderboards, streaks are app-layer concerns. The kernel just provides signed receipts - apps add meaning.

4. **Reputation IS memory**: In the betting domain, your receipt history IS your reputation. This makes portable memory economically valuable (better odds, monetization, trust).

5. **Simpler = faster iteration**: Without E2EE complexity, Streams went from concept to TestFlight faster. It's a proving ground before adding encryption.

**The gap**: Streams receipts are public. Buds receipts are private. The kernel needs to support BOTH - a receipt that can be selectively revealed (ZK proofs?) or permissioned (capabilities?). Neither project fully solves this yet.

---

## 12. Product Vision (Beyond Kernel)

**3-Year Goal**: "Streams becomes the LinkedIn of sports betting - your verifiable betting reputation follows you everywhere."

**Monetization path**:
1. Freemium subscription (advanced analytics)
2. Affiliate revenue (sportsbook signups)
3. Marketplace (sharps selling picks)
4. Data licensing (anonymized betting data)

**Growth loop**: Beautiful receipt → Share to Twitter → Web preview → App Store → Onboard → Create receipt → Share → Loop

This is app-layer vision, not kernel. But it shows why portable, verifiable memory has economic value.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
