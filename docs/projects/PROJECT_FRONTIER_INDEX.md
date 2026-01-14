# Frontier Index (Thielverse) - One Pager

**Status**: YC Demo Complete (Weekend Build)
**Last Updated**: 2026-01-14
**Kernel Contribution**: **Receipts as knowledge infrastructure** - verified intelligence network for frontier innovation

---

## 1. Vision Alignment

The Frontier Index is a **verified memory layer for human progress**. It transforms public data (papers, patents, filings, grants) into cryptographically hashed, timestamped receipts that can be indexed, searched, and reasoned about.

**The thesis**: LLMs hallucinate because they lack ground truth. A receipt-based verification layer provides immutable facts that AI can cite, not invent. **Receipts-in, bias-out.**

**The kernel connection**: This is the receipt primitive applied to **knowledge** instead of personal memory (Buds) or reputation (Streams). Same pattern - hash, timestamp, source, immutable - different domain.

**Built in a weekend for YC.** Demonstrates that receipt infrastructure can power not just apps, but AI reasoning.

---

## 2. Technical Architecture

```
Next.js 16 (React 19, TypeScript)
├── src/app/
│   ├── page.tsx                    # Home feed (receipts by frontier)
│   ├── entity/[slug]/page.tsx      # Entity detail + Ask bar
│   ├── api/
│   │   ├── receipts/latest/        # Feed with tier gating
│   │   ├── analysis/[cid]/         # Content-addressed intelligence
│   │   ├── ask/                    # Deterministic Q&A with citations
│   │   ├── brief/                  # Daily frontier digest
│   │   ├── search/                 # Full-text search
│   │   ├── lens/[name]/[slug]/     # Deterministic lens outputs
│   │   ├── cron/promote/           # Flip visibility (3/min)
│   │   └── cron/openalex/          # Ingest papers (15min)
│   └── components/
│       ├── ReceiptTable.tsx        # Main display
│       ├── AnalysisModal.tsx       # 5-layer intelligence view
│       └── AskBar.tsx              # Entity-specific Q&A
├── db/schema.sql                   # receipts, entities, entity_receipt
└── scripts/
    ├── seed_receipts_from_csv.ts
    ├── generate_and_upload_analyses_simple.ts
    └── backfill_entities_from_csv.ts

Supabase (PostgreSQL + Storage)
├── receipts table                  # Hash-deduplicated public facts
├── entities table                  # Persistent actors (orgs, labs, people)
├── entity_receipt join             # Many-to-many relationships
└── Storage bucket: analyses/       # Content-addressed JSON artifacts
```

**Tech Stack**:
- Next.js 16 + React 19 + Tailwind v4
- Supabase (PostgreSQL + Storage)
- Vercel (deployment + crons)
- IPFS-compatible CID generation (planned)

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **Receipt** | `SHA256(url\|published_at\|title)` | Immutable proof-of-fact from public sources |
| **Entity** | Persistent actor (org, lab, person) | Aggregation point for receipts over time |
| **Analysis** | Content-addressed JSON artifact | 5-layer intelligence (technical, market, regulatory, sentiment, lenses) |
| **CID** | `sha256-{hash}` (IPFS-compatible) | Content address for analysis artifacts |
| **Frontier** | AI, Energy, Biotech, Thielverse | Domain classification |
| **Lens** | Deterministic reasoning template | Same inputs → same outputs, with citations |

---

## 4. Data Model

### Layer 1: Public Tier (Free)

```sql
receipts (
  id UUID PRIMARY KEY,
  hash TEXT UNIQUE,           -- SHA256(url|published_at|title)
  source TEXT,                -- OpenAlex, USPTO, SEC, etc.
  title TEXT,
  url TEXT,
  published_at TIMESTAMP,
  frontier TEXT,              -- AI|Energy|Biotech|Thielverse
  visible BOOLEAN DEFAULT FALSE,
  summary TEXT,
  cid TEXT,                   -- Link to analysis if analyzed
  created_at TIMESTAMP
)
```

### Layer 2: Intelligence Tier (Pro)

```sql
analyses (
  cid TEXT PRIMARY KEY,       -- Content address
  receipt_hash TEXT,          -- Link to receipt
  version INT,
  storage_path TEXT,          -- analyses/{cid}.json
  analysis_types TEXT[],
  quality_score FLOAT,
  token_cost INT,
  created_at TIMESTAMP
)
```

### Analysis JSON Artifact (5 Layers)

```json
{
  "id": "sha256-{deterministic_hash}",
  "receipt_hash": "sha256(url|date|title)",
  "meta": { "title", "url", "published_at", "frontier", "entities", "summary" },
  "entity_links": [{ "slug": "helion-energy", "confidence": 0.8 }],
  "technical": { "innovation_score": 0.7, "notes": "..." },
  "market": { "funding_signal": 0.6, "momentum": "...", "partnerships": [...] },
  "regulatory": { "risk_score": 0.3, "compliance_flags": [...] },
  "sentiment": { "overall": "bullish", "confidence": 0.75 },
  "lenses": {
    "engineer-realist": { "output": "...", "citations": ["R1", "R2"] }
  }
}
```

---

## 5. The Deterministic Intelligence Pattern

### Why Determinism Matters

Traditional AI reasoning:
```
Query → LLM → Answer (different every time, may hallucinate)
```

Frontier Index reasoning:
```
Query → Fetch receipts → Deterministic template → Answer with citations
Same inputs → Same outputs → Auditable
```

### Lens System (Zero-Temperature Reasoning)

```typescript
// /api/lens/[name]/[slug]
// Input: Entity slug + lens name
// Process:
1. Fetch last 20 receipts for entity
2. Sort deterministically (by date, then hash)
3. Apply lens template (engineer-realist, market-mapper, etc.)
4. Assemble output with [R#] citations

// Output: Same answer every time, cites only verified receipts
```

### Ask Endpoint

```typescript
// /api/ask?q=helion
// Returns cached intelligence with citations:

"Based on 47 receipts for Helion Energy:

Recent Activity: Most recent update on 2025-11-08
Frontiers: Energy
Intelligence: 12 receipts have full analysis available

Latest receipts:
• 2025-11-08 — OpenAlex: Helion Polaris reactor design update [R1]
• 2025-11-06 — SEC Form D: $25M Series C funding [R2]"
```

Every claim cites a receipt. No hallucination possible.

---

## 6. Ingestion Pipeline

### Cron Jobs

**Promote Cron** (`/api/cron/promote`, every 1 min):
```sql
UPDATE receipts SET visible = true
WHERE visible = false
ORDER BY published_at ASC
LIMIT 3
```
Makes feed "alive" - ~4,300 receipts/day revealed.

**OpenAlex Cron** (`/api/cron/openalex`, every 15 min):
```typescript
1. Fetch: api.openalex.org/works?search=fusion&sort=publication_date:desc
2. Parse: title, publication_date, doi
3. Hash: SHA256(url|published_at|title)
4. Upsert: receipts with frontier="Energy", visible=true
```
Ingests real academic papers continuously.

### Seed Scripts

```bash
# Bulk load from curated CSV
npx tsx scripts/seed_receipts_from_csv.ts

# Generate analysis artifacts from CSV columns
npx tsx scripts/generate_and_upload_analyses_simple.ts

# Link entities to receipts
npx tsx scripts/backfill_entities_from_csv.ts
```

---

## 7. Business Model (Built Into Code)

### Tier Gating

| Feature | Free | Pro ($5/mo) | Enterprise ($25k+/yr) |
|---------|------|-------------|----------------------|
| Receipts | 7-day delay | Real-time | Real-time |
| Analysis CID | Hidden | Visible | Visible + custom |
| Full JSON | No | Yes | Yes + private |
| API access | No | 1k/day | Unlimited |

### Unit Economics

```
10M receipts indexed (cheap: metadata only)
→ 100k analyzed (expensive: LLM + compute, top 1%)
→ $0.01 per analysis = $1,000 total

10k Pro users × $5/mo = $50,000/mo
Serve cost: ~$3,000/mo (cached fetches)
Margin: 93%
```

**The insight**: Index everything (cheap), analyze selectively (expensive), sell access to analysis. Knowledge moat compounds.

---

## 8. Lessons Learned

**What worked:**
- **Deterministic reasoning**: Same inputs → same outputs → auditable. Kills hallucination.
- **Content-addressed storage**: `analyses/{cid}.json` means artifacts are immutable and cacheable forever.
- **Tier gating via delay**: 7-day delay for free tier is simple and effective. No complex auth.
- **Promote cron**: Makes feed feel "alive" with 3 receipts/min revealed. Cheap illusion.
- **Weekend scope discipline**: Prioritized working demo over perfect architecture.

**What was rough:**
- **No actual LLM calls**: Demo uses cached/canned responses. Full pipeline not implemented.
- **CID not true IPFS**: Uses `sha256-` prefix, not real IPFS multihash.
- **Tier gating not enforced**: All responses public in demo.

**Key insights:**
- **Receipts scale to knowledge**: Same pattern (hash, timestamp, source, immutable) works for papers, patents, filings.
- **Selective analysis is the moat**: Index 10M, analyze 100k. Quality scoring determines what gets intelligence.
- **Citations are everything**: AI that cites verified receipts can't hallucinate. The constraint is the feature.

---

## 9. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **Receipt hash pattern** | `scripts/*.ts` | `SHA256(url\|date\|title)` - universal dedup |
| **Content-addressed storage** | `/api/analysis/[cid]` | CID-based artifact retrieval |
| **Deterministic lens** | `/api/lens/[name]/[slug]` | Template-based reasoning with citations |
| **Promote cron** | `/api/cron/promote` | Gradual visibility revelation |
| **Tier delay logic** | `/api/receipts/latest` | 7-day delay enforcement |
| **Entity-receipt graph** | `db/schema.sql` | Many-to-many with roles |

---

## 10. Kernel Extraction Candidates

**Tier 1 - Ready to extract:**
1. **Receipt hash pattern**: `SHA256(url|date|title)` for deduplication
2. **Content-addressed storage**: CID → artifact, immutable forever
3. **Deterministic reasoning template**: Same inputs → same outputs → citations required

**Tier 2 - Needs generalization:**
4. **Entity-receipt graph**: Persistent actors linked to receipts over time
5. **Quality scoring**: Novelty + impact → selective analysis
6. **Tier gating**: Delay-based access control

**Tier 3 - Domain-specific:**
7. **Frontier classification**: AI/Energy/Biotech taxonomy
8. **OpenAlex integration**: Academic paper ingestion
9. **Lens system**: Specific reasoning templates

---

## 11. Why This Matters for the Kernel

The Frontier Index proves receipts work for **knowledge infrastructure**:

1. **Receipts scale to billions**: Index 10M papers, patents, filings. Same pattern as personal memory.

2. **Deterministic reasoning is achievable**: Constrain AI to cite verified receipts → no hallucination.

3. **Content-addressed storage works**: `analyses/{cid}.json` is immutable, cacheable, auditable.

4. **Selective analysis creates moats**: Index everything, analyze top 1%. Quality scoring is the differentiator.

5. **The business model is built-in**: Free tier proves value, Pro tier captures revenue, Enterprise scales.

**The connection to other projects:**

| Project | Receipt Domain | Key Innovation |
|---------|----------------|----------------|
| TheMove | Social posts | Attribution via hash |
| STL Food Agent | Civic inspections | File-based receipts |
| Streams | Betting reputation | Public + signed |
| Buds | Personal memory | Private + E2EE |
| ChaingeNode | Civic participation | 53ms generation |
| **Frontier Index** | **Human knowledge** | **Deterministic AI reasoning** |

The kernel isn't just for apps - it's infrastructure for AI that doesn't lie.

---

## 12. The YC Pitch (What They Were Selling)

**One sentence**: *A verified index of frontier progress that proves LLMs can reason accurately when given ground truth instead of vibes.*

**Demo script (3-4 min)**:
1. Home feed → "We index papers, patents, filings across frontier domains"
2. Click receipt → "Every row is hashable, timestamped, verifiable"
3. Entity page → "Intelligence aggregates over time"
4. Ask box → "Deterministic reasoning: same inputs, same outputs, citations required"
5. Tier demo → "Free = 7-day delay + metadata; Pro = real-time + full intelligence"
6. Close → "Selective analysis at 93% margins. Network gets smarter as we ingest more."

**Why it wins:**
- Technical sophistication (IPFS-ready, content-addressed, append-only)
- Clear business model (Free → Pro → Enterprise)
- Compounding moat (every receipt improves entity graphs)
- Perfect timing (LLM hallucination problem + open data explosion)

---

## 13. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | Next.js 16 | Latest React 19, server components |
| Database | Supabase (PostgreSQL) | Fast setup, storage included |
| Storage | Supabase Storage | Content-addressed artifacts |
| Deployment | Vercel | Cron support, edge functions |
| CID | @ipld/dag-json, multiformats | IPFS-compatible (planned) |

---

## 14. Current State vs. Full Vision

| Feature | Demo (Weekend) | Full Vision |
|---------|----------------|-------------|
| Receipts | ~600 seeded | 10M+ indexed |
| Analysis | CSV-templated | LLM-generated |
| CID | sha256- prefix | True IPFS |
| Tier gating | All public | Enforced |
| Search | Basic ILIKE | Semantic + vector |
| Lenses | 2 templates | 10+ specialized |

The demo proves the architecture. The full system is 10x more data + real LLM integration.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
*Built in a weekend for YC - receipts as knowledge infrastructure*
