# TheMove - One Pager

**Status**: Production (themove.social)
**Last Updated**: 2026-01-14
**Kernel Contribution**: **TRUE GENESIS** - First code project ever. Proved anonymity via deterministic hashing before receipts existed.

---

## 1. Vision Alignment

TheMove is an **anonymous college bulletin board** - students post "moves" (party announcements, hot takes, confessions) without revealing identity. The core thesis: **you can have accountability without identity**.

Users authenticate with their .edu email, but the system **never stores the email in posts or reactions**. Everything is keyed to `SHA256(email + pepper)` - a deterministic hash that enables repeat-user detection while being cryptographically irreversible.

**The insight that precedes receipts**: Memory can be attributed to a hash, not a person. The hash proves "same author" without revealing who. This is the seed of the identity model that became `did:phone` in Buds and `did:streams` in Streams.

**This was your first code project.** The sophistication of the architecture - two-token auth, database-as-referee, timing-safe HMAC, DST-aware resets, idempotent webhooks - is remarkable for a first attempt.

---

## 2. Technical Architecture

```
Next.js 15 (App Router, Server Components Only)
├── src/app/
│   ├── page.tsx                    # Landing (email entry)
│   ├── start/year/page.tsx         # Year picker
│   ├── b/[school]/[year]/          # Board view + compose
│   ├── api/
│   │   ├── resolve/route.ts        # Email → school, mint prebind
│   │   ├── bind/route.ts           # Year → board, mint access
│   │   ├── post/free/route.ts      # Create free post
│   │   ├── reaction/route.ts       # Up/down votes
│   │   ├── checkout/route.ts       # Stripe session
│   │   └── stripe/webhook/route.ts # Payment completion
│   └── legal/                      # Terms, privacy, takedown
├── src/lib/
│   ├── token.ts                    # HMAC sign/verify
│   ├── db.ts                       # Prisma + emailHashHex
│   ├── day.ts                      # Timezone-aware dateKey
│   ├── ratelimit.ts                # In-memory sliding window
│   └── price.ts                    # Dynamic pricing
├── prisma/schema.prisma            # Data model
└── middleware.ts                   # Origin guard
```

**Tech Stack**:
- Next.js 15 (zero client JS - all server-rendered)
- PostgreSQL + Prisma
- Stripe (payments)
- Vercel (deployment)
- TypeScript (strict)

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **authorHash** | `SHA256(email + EMAIL_PEPPER)` | Pseudonymous identity - same email = same hash, irreversible |
| **Prebind Token** | JWT with email, 10min expiry | Temporary token carrying email for year selection |
| **Access Token** | JWT with authorHash, 30min expiry | Main auth token - never contains email |
| **dateKey** | `YYYY-MM-DD` in school timezone | Daily reset boundary (DST-safe) |
| **FreePost** | Unique constraint `(boardId, dateKey)` | Database-enforced "1 free post per day" |
| **Board** | `SHA256(schoolId + year)` | Deterministic board identifier |

---

## 4. The Anonymity Model (The Clever Core)

### Two-Token Authentication Flow

```
User enters email@wustl.edu
    ↓
POST /api/resolve
├── Validate .edu domain against School.domain
├── Hash email: SHA256(email.lowercase() + ":" + EMAIL_PEPPER)
├── Check EmailAccess table:
│   ├── EXISTS: Returning user → mint Access token, redirect to board
│   └── NOT EXISTS: New user → mint Prebind token, redirect to year picker
    ↓
Year Picker (/start/year)
├── Decode prebind token (contains email - last time it appears!)
├── User selects graduation year
    ↓
POST /api/bind
├── Verify prebind token
├── Extract email from token (FINAL COPY)
├── Create/upsert Board(schoolId, year)
├── Create/upsert EmailAccess(emailHash, boardId)
├── Mint Access token: { school, year, authorHash } - NO EMAIL
├── Set access cookie, redirect to board
    ↓
Board (/b/[school]/[year])
├── Verify access token (authorHash only)
├── User posts/reacts using authorHash
└── EMAIL NEVER APPEARS AGAIN
```

### Why This Works

1. **Email → Hash is one-way**: Can't reverse `SHA256(email + pepper)` to get email
2. **Pepper prevents rainbow tables**: Even if DB leaks, hashes are useless without pepper
3. **Deterministic**: Same email always produces same hash → enables "same author" detection
4. **Two-token containment**: Email exists only in prebind token (10 min), then vanishes forever
5. **authorHash is pseudonymous**: Posts/reactions attributed to hash, not person

---

## 5. Database as Referee

**The key architectural insight**: Business logic enforced by schema, not code.

### Unique Constraints as Invariants

```prisma
// One free post per board per day
FreePost {
  @@unique([boardId, dateKey])
}

// One reaction per user per post
Reaction {
  @@unique([postId, authorHash])
}

// One email binding per hash
EmailAccess {
  emailHash String @unique
}

// One order per Stripe session
Order {
  sessionId String @unique
}
```

**Why this matters**: Race conditions can't violate invariants. Two users trying to claim the free post simultaneously? One gets P2002 unique violation. Code doesn't need to coordinate - the database is the referee.

### Serializable Transactions

```typescript
prisma.$transaction(
  async (tx) => {
    await tx.freePost.create({ data: { boardId, dateKey } })
    const post = await tx.post.create({ data: { ... } })
    await tx.freePost.update({ where: { boardId_dateKey }, data: { postId: post.id } })
  },
  { isolationLevel: Prisma.TransactionIsolationLevel.Serializable }
)
```

Serializable isolation prevents phantom reads. Free post creation is atomic.

---

## 6. Data Flow Patterns

### Free Post Creation

```
User submits body (max 256 chars)
    ↓
POST /api/post/free
├── Verify access token → extract authorHash
├── Compute dateKey = dateKeyFor(school.tz) // NOW, not from client
├── BEGIN SERIALIZABLE
│   ├── CREATE FreePost(boardId, dateKey) // unique constraint
│   ├── CREATE Post(boardId, authorHash, body, dateKey, isPaid=false)
│   └── UPDATE FreePost.postId = newPost.id
├── COMMIT
    ↓
Success: 303 redirect to board
Failure (P2002): redirect with flash "quota used"
```

### Paid Post Creation (Stripe Webhook)

```
User submits body → /api/checkout
    ↓
Create Stripe session with metadata:
{ boardId, dateKey, authorHash, body }
    ↓
User completes Stripe checkout
    ↓
Stripe fires webhook: checkout.session.completed
    ↓
POST /api/stripe/webhook
├── Verify Stripe signature
├── Idempotency gate 1: INSERT StripeEvent(event.id) // unique
├── Idempotency gate 2: INSERT Order(sessionId) // unique
├── CREATE Post from session metadata (isPaid=true)
    ↓
Post appears on board (paid posts unlimited)
```

**Key insight**: Checkout NEVER writes to DB. Only the verified webhook writes. This prevents:
- Double-charging
- Fake payments
- Race conditions

---

## 7. Lessons Learned

**What worked:**
- **Two-token flow**: Containing email to 10-minute prebind window is elegant. Email vanishes after year selection.
- **Database-as-referee**: Unique constraints are more reliable than application logic. Can't be bypassed.
- **Timing-safe comparison**: Using `crypto.timingSafeEqual` for HMAC verification prevents timing attacks.
- **Timezone-aware daily reset**: `dateKeyFor(tz)` handles DST correctly. Unit tests prove spring/fall edges work.
- **Webhook-only writes**: Stripe payments are bulletproof because only verified webhooks create posts.
- **Zero client JS**: Server-side rendering simplifies mental model. No hydration bugs.

**What was clever for a first project:**
- **HMAC tokens**: Rolling your own JWT-like system with proper crypto (SHA256-HMAC, base64url, timing-safe verify)
- **Pepper secret**: Knowing to add a server-side pepper to prevent rainbow tables
- **Serializable isolation**: Understanding transaction isolation levels for concurrent writes
- **Rate limiting**: Building sliding-window rate limits in-memory with proper bucket separation
- **DST testing**: Writing explicit tests for daylight saving time transitions

**What could improve:**
- **Session refresh**: Tokens expire silently (30 min). Could add refresh flow.
- **Multi-instance rate limits**: In-memory limits don't work across Vercel instances. Noted for Redis swap.
- **Email verification**: Currently trusts .edu domain. Could add actual email verification.

---

## 8. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **HMAC token system** | `src/lib/token.ts` | Direct extraction - sign/verify with timing-safe compare |
| **emailHashHex** | `src/lib/db.ts` | Pepper-protected email hashing pattern |
| **dateKeyFor** | `src/lib/day.ts` | DST-safe timezone date computation |
| **Rate limiter** | `src/lib/ratelimit.ts` | Sliding window per-IP limiting |
| **Two-token flow** | `api/resolve` + `api/bind` | Containment pattern for sensitive data |
| **Webhook idempotency** | `api/stripe/webhook` | Event + Order uniqueness gates |

---

## 9. Open Questions / Future Work

- [ ] **Email verification**: Send confirmation link instead of trusting .edu domain
- [ ] **Redis rate limits**: Swap in-memory for Redis to work across instances
- [ ] **Token refresh**: Add refresh mechanism instead of silent expiry
- [ ] **Multi-school support**: One email accessing multiple schools (currently one binding per hash)
- [ ] **Moderation dashboard**: Admin view for flagged content (partially implemented)
- [ ] **Push notifications**: "New move on your board" alerts

---

## 10. Kernel Extraction Candidates

**Tier 1 - Ready to extract:**
1. **emailHashHex pattern**: `SHA256(email + pepper)` - used in Buds' phone-based DID
2. **HMAC token system**: Sign/verify with timing-safe comparison
3. **Two-token containment**: Sensitive data in short-lived token, hash in long-lived token
4. **dateKeyFor**: DST-safe timezone date computation

**Tier 2 - Pattern-level:**
5. **Database-as-referee**: Unique constraints enforce invariants
6. **Webhook-only writes**: External events → verified writes only
7. **Serializable transactions**: Concurrent write protection

**Tier 3 - Domain-specific:**
8. **Dynamic pricing**: Price based on board population
9. **Daily reset**: Time-windowed content lifecycle

---

## 11. Security Model

| Property | Implementation | Notes |
|----------|----------------|-------|
| **Anonymity** | SHA256(email + pepper) | One-way hash, pepper prevents rainbow tables |
| **Authentication** | HMAC-SHA256 tokens | Timing-safe verification |
| **Session** | httpOnly cookies, 30min | No refresh, silent expiry |
| **CSRF** | Origin header check | Middleware guards POST endpoints |
| **Rate limiting** | Sliding window, per-IP | 3/min free, 15/min react, 3/min checkout |
| **Payment** | Webhook-only writes | Stripe signature verification |

---

## 12. Why This Matters for the Kernel

TheMove is the **intellectual ancestor** of the receipt model:

| Concept in TheMove | Evolved Into |
|--------------------|--------------|
| `authorHash = SHA256(email + pepper)` | `did:phone:SHA256(phone + salt)` in Buds |
| Deterministic hash for "same author" | CID for "same receipt" |
| Two-token containment | Prebind → Access flow is like Buds' identity upgrade |
| Database-as-referee | Relay sequence uniqueness constraints |
| Daily reset (dateKey) | Receipt timestamps + time-bounded queries |

**The core insight that carried forward**: Identity can be a hash. Attribution doesn't require revelation. You can have accountability (same author across posts) without identity (knowing who that author is).

This is the foundation of the kernel's privacy model: **memory attributed to cryptographic identifiers, not personal information**.

---

## 13. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | Next.js 15 | App Router, RSC, zero client JS |
| Language | TypeScript | Strict mode |
| Database | PostgreSQL | Production-grade, Prisma ORM |
| Payments | Stripe | Checkout + webhooks |
| Deployment | Vercel | Zero-config, edge functions |
| Node | v20 | Latest LTS |

---

## 14. The "First Project" Factor

What makes this remarkable as a **first code project**:

1. **Security mindset from day one**: HMAC, timing-safe compare, pepper, httpOnly cookies
2. **Distributed systems thinking**: Idempotency, serializable transactions, webhook verification
3. **Database design**: Schema-enforced invariants, indexes, unique constraints as business logic
4. **Time handling**: DST-aware timezone computation with unit tests for edge cases
5. **Production readiness**: Stripe integration, rate limiting, moderation, legal pages
6. **Documentation discipline**: Decision log, auto-generated file trees, route catalogs

Most first projects are TODO apps. This is a **production-deployed, payment-enabled, cryptographically-anonymous social platform**.

The architectural patterns here - two-token flow, database-as-referee, webhook-only writes - are patterns that senior engineers debate. You implemented them correctly on your first try.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
*True Genesis: The anonymity model that became the kernel's identity philosophy*
