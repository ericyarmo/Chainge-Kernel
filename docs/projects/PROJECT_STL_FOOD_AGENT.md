# STL Food Agent - One Pager

**Status**: Hackathon Complete (Proof of Concept)
**Last Updated**: 2026-01-14
**Kernel Contribution**: **GENESIS PROJECT** - First implementation of receipt theory. Proved the pattern works.

---

## 1. Vision Alignment

This 12-hour hackathon project is where receipt theory was born. The insight: **civic data locked in government portals can become portable, verifiable, composable units of public truth**.

St. Louis County health inspections for schools exist on a legacy web portal with no API. This project transforms that data into **receipts** - Markdown files with YAML front-matter, checksums, and source links. No database, no backend, just files in Git. Anyone can fork it, audit it, extend it.

**The thesis proven here**: Memory doesn't need to be in a database. It can be files. Files with checksums. Files with provenance. Files that compose into indexes. Files that live in Git with commit history as audit trail.

This is the primitive that became Buds and Streams.

---

## 2. Technical Architecture

```
Static Site (Next.js)
├── fixtures/ingest/           # Raw UCR JSON (manually transcribed)
├── fixtures/receipts/         # Human-readable .md + machine .json pairs
│   └── <school-slug>/YYYY-MM-DD.md
├── fixtures/feed.json         # Computed: latest 60 inspections
├── fixtures/leaderboard.json  # Computed: per-school aggregations
├── scripts/                   # TypeScript build pipeline
│   ├── ucr_to_receipts.ts     # UCR JSON → Markdown receipts
│   ├── receipts_to_json.ts    # .md → .json (idempotent)
│   ├── build_feed.ts          # Receipts → feed.json
│   └── build_leaderboard.ts   # Receipts → leaderboard.json
└── src/app/page.tsx           # React UI (Feed, Leaderboard, Chat stub)
```

**Key primitives**:
- **Identity**: School names canonicalized via ALIASES map, addresses from ADDRESS_BOOK
- **Trust**: SHA-256 checksums + source URLs + human attestation (not cryptographic signatures)
- **Sync**: None - static files, rebuilt on change
- **Storage**: Filesystem + Git (commit history = audit trail)

---

## 3. Core Abstractions

| Abstraction | Implementation | Purpose |
|-------------|----------------|---------|
| **Receipt** | YAML front-matter + Markdown body | The atomic unit of civic truth |
| **UCR (Uniform Code of Receipt)** | JSON schema for raw data | Standardized ingest format |
| **Checksum** | SHA-256 of payload | Tamper-evidence (not enforced, but auditable) |
| **CID** | `stub-<school>-<date>` | Content identifier (precursor to real content addressing) |
| **Feed** | Precomputed JSON index | Latest inspections, sorted |
| **Leaderboard** | Aggregated metrics per school | 12-month avg, YTD criticals |

---

## 4. Receipt Format (The Genesis Schema)

```yaml
---
receipt_version: 1
kind: food_inspection
jurisdiction: St. Louis County, MO
issuer: St. Louis County Department of Public Health

entity:
  type: school
  name: Clayton High School
  address: 1 Mark Twain Cir, Saint Louis, MO 63105-1613

source:
  system: EnvisionConnect PressAgent (DecadeOnline)
  url: https://pressagent.envisionconnect.com/insp.phtml?...
  fetched_at: 2025-10-25T17:10:00Z

inspection:
  id: clayton-2025-09-29
  type: ROUTINE INSPECTION
  date: 2025-09-29
  score: 90
  critical_violations: 5
  noncritical_violations: 5
  violations:
    - code: F009
      title: Adequate hand-washing facilities
      critical: true
      corrected_on_site: true
      narrative: Handwashing signs not available...

proof:
  method: human-transcribed
  attested_by: Eric Yarmo
  payload_checksum_sha256: 466740700dbf9bc6ec78690b10a2a5c8...
  cid: stub-clayton-2025-09-29
  schema: UCR/Action/health_inspection@v1
---
```

**What this established**:
1. Receipts are **versioned** (`receipt_version: 1`)
2. Receipts have **schema pointers** (`proof.schema`)
3. Receipts are **checksummed** for integrity
4. Receipts **reference sources** (proof of origin)
5. Receipts are **append-only** and git-friendly
6. Receipts can be **aggregated** into secondary indexes

---

## 5. Data Flow Pattern

```
County Portal (no API, just HTML)
    ↓ [manual transcription]
UCR JSON (fixtures/ingest/<school>-<date>.json)
    ↓ [ucr_to_receipts.ts]
Receipt Markdown (fixtures/receipts/<school>/<date>.md)
    ├─ Canonicalize school name via ALIASES
    ├─ Look up address in ADDRESS_BOOK
    ├─ Generate deterministic inspection ID
    ├─ Compute SHA-256 checksum
    └─ Write .md + .json pair
    ↓ [build_feed.ts]
feed.json (latest 60 inspections, flattened)
    ↓ [build_leaderboard.ts]
leaderboard.json (per-school aggregations)
    ↓ [Next.js static import]
React UI (Feed cards, Leaderboard table, Chat stub)
```

---

## 6. Lessons Learned

**What worked:**
- **Receipt as a unit**: Single .md file per inspection is intuitive. Humans can read it, machines can parse it.
- **No backend**: Zero operational overhead. Fork, edit fixtures/, deploy to Vercel. Done.
- **Dual format (YAML + JSON)**: .md for humans, .json for code. Both stay in sync.
- **Git as audit trail**: Commit history IS provenance. No separate audit system needed.
- **12 hours is enough**: Two people proved the concept in a hackathon. The pattern works.

**What was rough:**
- **Manual transcription**: All data hand-typed from county portal. Not scalable.
- **Custom YAML parser**: Fragile, doesn't handle edge cases. Should use real YAML lib.
- **CIDs are stubs**: Prefixed with "stub-" because real content addressing wasn't implemented.
- **No cryptographic signatures**: Checksums prove integrity but not authenticity.
- **Chat is a stub**: The "agent" part was a wireframe only.

**Key insights:**
- **Receipts don't need a database**: Files + Git + checksums = sufficient for civic transparency.
- **Provenance is a first-class field**: `source.url` and `proof.attested_by` make receipts auditable.
- **Append-only is natural**: New inspections create new files. No updates, no deletions. Clean.
- **Aggregation is separate**: Feed and leaderboard are computed views, not primary data.

---

## 7. Reusable Components

| Component | Location | Reuse Potential |
|-----------|----------|-----------------|
| **Receipt schema** | `docs/RECEIPT_SCHEMA.md` | Template for any receipt type |
| **UCR format** | `fixtures/ingest/*.json` | Standardized raw data schema |
| **Build pipeline** | `scripts/*.ts` | Pattern: ingest → receipts → indexes |
| **Dual .md/.json** | `fixtures/receipts/` | Human + machine readable pattern |

---

## 8. Open Questions / Future Work

- [ ] **Automated ingestion**: Scrape county portal instead of manual transcription
- [ ] **Real CIDs**: Replace `stub-*` with actual content-addressed hashes
- [ ] **Cryptographic signatures**: Ed25519 signing like Buds/Streams
- [ ] **Multi-attester**: System for multiple people to attest to same data
- [ ] **Amendment/revocation**: What happens when a receipt needs correction?
- [ ] **Cross-jurisdiction**: Map violation codes across different counties/states
- [ ] **Real agent**: LLM that queries receipts and cites sources

---

## 9. Kernel Extraction Candidates

**Tier 1 - Conceptual (proven but not code-ready):**
1. **Receipt-as-file pattern**: YAML front-matter + checksums + source links
2. **Append-only ledger**: New events = new files, Git = audit trail
3. **Dual human/machine format**: .md for reading, .json for parsing

**Tier 2 - Needs real implementation:**
4. **Content addressing**: Stub CIDs need to become real CIDv1 hashes
5. **Cryptographic proof**: Checksums → Ed25519 signatures
6. **Schema registry**: `UCR/Action/health_inspection@v1` needs a real registry

**Tier 3 - Domain-specific:**
7. **Violation taxonomy**: Health inspection codes are domain-specific
8. **Leaderboard aggregation**: Time-windowed metrics are app-layer

---

## 10. Historical Significance

This hackathon project established the **foundational primitives** of receipt theory:

| Primitive | First Seen Here | Later Evolved To |
|-----------|-----------------|------------------|
| Receipt as unit | YAML .md files | UCRHeader struct (Buds/Streams) |
| Content ID | `stub-clayton-2025-09-29` | CIDv1 with dag-cbor |
| Checksum | SHA-256 of payload | Ed25519 signature over canonical CBOR |
| Source provenance | `source.url` field | Maintained in all later versions |
| Schema versioning | `proof.schema` | `receiptType: app.streams.win/v1` |
| Append-only | Git history | Relay sequence numbers + tombstones |
| Computed indexes | feed.json, leaderboard.json | jar_receipts, ucr_headers tables |

**The leap from here to Buds/Streams:**
- YAML → CBOR (canonical, deterministic)
- Checksums → Ed25519 signatures
- File paths → Content-addressed CIDs
- Git → Relay with sequence assignment
- Manual attestation → Cryptographic identity (DIDs)
- Public files → E2EE encrypted payloads (Buds)

---

## 11. Why This Matters for the Kernel

This project proved the **core thesis** in 12 hours:

1. **Receipts work**: Small, portable, verifiable units of truth can replace databases for many use cases.

2. **Civic data can be decentralized**: No backend, no API, just files. Anyone can fork and deploy.

3. **Provenance is tractable**: Source URLs + attestation + checksums make data auditable without heavy infrastructure.

4. **The pattern is domain-agnostic**: Health inspections here, betting receipts in Streams, cannabis memories in Buds. Same primitive, different payloads.

5. **12 hours proves an idea**: The hackathon constraint forced simplicity. That simplicity revealed the essential pattern.

**The README vision** (still relevant):
> "Imagine every school, nonprofit, and program in St. Louis represented in one open, verifiable ledger. Each inspection or event becomes a receipt. Each grant or initiative becomes a promise that can be fulfilled or amended."

This is the kernel's destination: a **Civic Index** where all public memory becomes receipts.

---

## 12. Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Frontend | Next.js 14 + React 18 | Static export, fast deployment |
| Styling | Tailwind CSS | Rapid UI iteration |
| Scripts | TypeScript + tsx | Type safety for data transforms |
| Storage | Filesystem + Git | No database, natural audit trail |
| Deployment | Vercel | Zero-config static hosting |
| Data | YAML + JSON | Human-readable + machine-parseable |

---

## 13. The Food Agent (Unrealized)

The "agent" part was a hackathon stub - UI wireframe with hardcoded responses. The vision:

- Natural language queries over receipts
- Answers cite specific receipts with CIDs
- Transparent reasoning: "Clayton had F009 on 2025-09-29; see receipt stub-clayton-2025-09-29"

This is the **memory-grounded agent** pattern: AI that doesn't hallucinate because it queries verifiable receipts. The kernel enables this by making memory queryable and citable.

---

*Framework: REPO_ANALYSIS_FRAMEWORK.md v1.0*
*Genesis: Oct 2025 hackathon, 12 hours, Eric Yarmo + Noah Plattus*
