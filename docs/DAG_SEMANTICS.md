# DAG Semantics: How Refs Create Causal Structure

**Version**: 0.5.1
**Date**: 2026-01-15

This document explains how the `refs` field creates a directed acyclic graph (DAG) of receipts, establishing causal relationships without central coordination.

**Important framing**: The kernel is not a DAG system. It's an **addressing system for memory** that allows DAG structures to emerge. The kernel provides content-addressed receipts with hash references; applications build whatever graph structures they need on top.

---

## Core Principle

```
If B.refs contains A.receipt_id, then B had knowledge of A when it was created.
```

This is **Lamport's happened-before relation** (causal precedence), not physical time ordering. Receipt A is *causally prior* to B—meaning B's author possessed A's hash at signing time. This says nothing about wall-clock time; two receipts created seconds apart might have no causal relationship if neither references the other.

The resulting structure is a **content-addressed DAG**—a directed acyclic graph where edges are cryptographic hash references. This is the same primitive underlying Git, IPFS, and certificate transparency logs.

**Note**: We avoid the term "Merkle DAG" here because that implies structured inclusion proofs. The kernel provides hash-linking; efficient membership proofs require additional structure at the application layer.

---

## Structural Patterns

### 1. Genesis (refs = [])

A receipt with no refs is a root node. It asserts existence without prior context.

```
┌───────────┐
│     A     │
│ refs: []  │
└───────────┘
```

### 2. Chain (refs.len() = 1)

Each receipt references exactly one predecessor. This creates a linear, append-only log.

```
┌───┐      ┌───┐      ┌───┐      ┌───┐
│ A │ ◄─── │ B │ ◄─── │ C │ ◄─── │ D │
└───┘      └───┘      └───┘      └───┘

A.refs = []
B.refs = [A]
C.refs = [B]
D.refs = [C]
```

**Use case**: Personal activity log, message thread, sequential state updates.

### 3. Fork (multiple receipts reference same parent)

Two receipts independently extend the same predecessor. This is not a conflict—it's concurrent activity.

```
              ┌───┐
         ┌─── │ B │    B.refs = [A]
         │    └───┘
┌───┐    │
│ A │ ◄──┤
└───┘    │
         │    ┌───┐
         └─── │ C │    C.refs = [A]
              └───┘
```

**Key insight**: B and C are *concurrent*—neither is causally prior to the other. They exist in the same partial order but are incomparable. This is a feature: independent actors don't need coordination.

**Equivocation warning**: A single author can also fork their own chain—signing two receipts that both reference the same parent but contain contradictory claims. The kernel considers both valid (signatures verify). Detecting and penalizing equivocation is an **application-layer concern**. See CONVENTIONS.md for single-writer chain patterns and equivocation detection strategies.

### 4. Merge (refs.len() > 1)

A receipt references multiple predecessors, acknowledging awareness of all of them.

```
┌───┐             ┌───┐
│ A │ ◄───┐       │ B │ ◄───┐
└───┘     │       └───┘     │
          │                 │
          └────────┬────────┘
                   │
               ┌───▼───┐
               │   M   │    M.refs = [A, B]
               └───────┘
```

**Use case**: Resolving forks, batch acknowledgment, multi-party agreement.

### 5. Countersign (witness pattern)

A second author references the first author's receipt, creating external attestation.

```
┌─────────────────┐           ┌─────────────────┐
│ CLAIM           │           │ WITNESS         │
│ author: Alice   │ ◄──────── │ author: Org     │
│ schema: civic.. │           │ schema: counter │
│ "I volunteered" │           │ "confirmed"     │
└─────────────────┘           └─────────────────┘
                              refs: [claim.receipt_id]
```

**Trust implication**: The claim now has two attestations—the original author and the witnessing org. Applications can weight trust accordingly.

---

## Complex DAG Example

```
      ┌───┐
      │ A │ ◄──────────────────────────┐
      └───┘                            │
        ▲                              │
        │                              │
      ┌───┐           ┌───┐            │
      │ B │ ◄──────── │ D │            │
      └───┘           └───┘            │
        ▲               ▲              │
        │               │              │
      ┌───┐             │          ┌───┴───┐
      │ C │ ◄───────────┴───────── │   F   │
      └───┘                        └───────┘

A.refs = []           genesis
B.refs = [A]          chain from A
C.refs = [B]          chain from B
D.refs = [B]          fork from B (concurrent with C)
F.refs = [A, C, D]    merge: sees A directly, plus both branches
```

Receipt F establishes that its author was aware of the entire graph at creation time.

---

## Refs Normalization

The kernel normalizes refs before signing:

1. **Sort** lexicographically by bytes
2. **Reject** if duplicates are present

```
Input:   refs = [0xcc..., 0xaa..., 0xbb...]
Stored:  refs = [0xaa..., 0xbb..., 0xcc...]  (sorted)
```

**Why this matters**: Deterministic signatures. The same logical set of refs, provided in any order, produces the same receipt bytes and therefore the same receipt ID.

```
refs = [B, A, C]  ─┐
                   ├──►  normalize  ──►  refs = [A, B, C]  ──►  signature
refs = [A, C, B]  ─┘
```

Without normalization, the same logical receipt could have multiple IDs, breaking content-addressing.

---

## Limits

| Constraint | Value | Rationale |
|------------|-------|-----------|
| Max refs | 128 | Bounds verification cost; prevents abuse |
| Ref size | 32 bytes | SHA-256 receipt ID |
| Max refs bytes | 4 KB | 128 × 32 bytes |

**Why 128?** This bounds the work a verifier must do. Each ref is a potential lookup. 128 is sufficient for:
- Batch operations (process N receipts)
- Quorum signatures (M-of-N attestations)
- Deep merges (multiple concurrent branches)

If you need more, create intermediate aggregation receipts.

---

## Partial Order Properties

The DAG defines a **partial order** on receipts:

| Relation | Meaning |
|----------|---------|
| A < B | A happened-before B (B.refs contains A, transitively) |
| A \|\| B | A and B are concurrent (neither refs the other) |
| A = B | Same receipt (same ID) |

**There is no total order.** Concurrent receipts cannot be ordered by the DAG alone. If your application needs total order, use a relay sequence number (see CONVENTIONS.md) or schema-level timestamps (untrusted).

---

## Convergence (Qualified)

Two nodes that sync **and are mutually willing to share everything** will converge to the same DAG:

```
Node 1: {A, B, C}
Node 2: {A, D, E}

After full sync:
Node 1: {A, B, C, D, E}
Node 2: {A, B, C, D, E}
```

This works because:
1. Receipts are immutable (no updates, only appends)
2. IDs are content-addressed (same content = same ID)
3. Sync is set union (no conflict resolution needed)

**Important caveat**: This convergence property only holds when both nodes share everything. In practice, nodes may:
- Filter by schema, author, or policy
- Hold encrypted payloads they can't share
- Apply selective disclosure rules
- Respect redaction requirements

In these cases, nodes may **never converge**, and that's by design. The correct statement is: *"Nodes converge on the union of receipts they are mutually willing and able to share."*

The kernel provides the convergent data structure (join-semilattice). Access control, encryption, and policy live at the application and replication layers.

---

## What the DAG Does NOT Provide

| Concern | Status | Where it lives |
|---------|--------|----------------|
| Total ordering | Not provided | Relay sequence numbers |
| Conflict resolution | Not provided | Application semantics |
| Garbage collection | Not provided | Application policy |
| Access control | Not provided | Application layer |

The kernel gives you **causal structure**. What you do with it is up to you.

---

## History Compression: Why 128 Refs Is More Than Enough

The 128-ref limit might seem restrictive, but with proper structure you can **commit to** arbitrary history in constant space. The technique is analogous to LZ77 compression: instead of repeating data, you reference where it already exists.

**Critical distinction**: Compression here means *commitment compression*, not *storage compression*. A root receipt that references intermediate nodes **proves the author had knowledge of all leaves**, but it does not make those leaves retrievable. Availability is a separate concern (see below).

### The LZ77 Analogy

LZ77 compresses by replacing repeated sequences with back-references:

```
Original:  "ABCABCABC"
Compressed: "ABC" + (offset=3, length=6)
```

The DAG equivalent: instead of referencing N individual receipts, create an intermediate receipt that commits to them, then reference that single receipt.

```
Instead of:    F.refs = [R1, R2, R3, ... R200]     ✗ exceeds 128

Do this:       C1.refs = [R1, R2, ... R100]        checkpoint 1
               C2.refs = [R101, R102, ... R200]    checkpoint 2
               F.refs = [C1, C2]                   ✓ only 2 refs
```

F transitively includes all 200 receipts through just 2 direct references.

### Checkpoint Pattern

For a long chain, create periodic checkpoints:

```
     R1 ◄── R2 ◄── R3 ◄── ... ◄── R99 ◄── R100
                                            │
                                    ┌───────▼───────┐
                                    │  Checkpoint 1 │
                                    │  refs: [R100] │
                                    └───────┬───────┘
                                            │
     R101 ◄── R102 ◄── ... ◄── R200 ◄───────┘
                                 │
                         ┌───────▼───────┐
                         │  Checkpoint 2 │
                         │  refs: [R200] │
                         └───────────────┘
```

Each checkpoint captures the "tip" of a segment. To prove awareness of R1-R200, you only need to reference Checkpoint 2.

### Tree Compression (Merkle Tree Pattern)

For even better compression, use a binary tree structure:

```
Level 0 (leaves):     R1   R2   R3   R4   R5   R6   R7   R8
                       \   /     \   /     \   /     \   /
Level 1:               [1,2]    [3,4]     [5,6]     [7,8]
                          \      /           \       /
Level 2:                 [1-4]               [5-8]
                             \               /
Level 3 (root):              [1-8]
```

```
L1_a.refs = [R1, R2]
L1_b.refs = [R3, R4]
L1_c.refs = [R5, R6]
L1_d.refs = [R7, R8]
L2_a.refs = [L1_a, L1_b]
L2_b.refs = [L1_c, L1_d]
Root.refs = [L2_a, L2_b]
```

**Depth = O(log N)**. The root receipt proves awareness of all N leaves.

### Compression Capacity

With 128 refs per receipt and tree structure:

| Depth | Receipts Summarized | Example |
|-------|---------------------|---------|
| 1 | 128 | Single batch |
| 2 | 16,384 | Day's activity |
| 3 | 2,097,152 | ~2 million |
| 4 | 268,435,456 | ~268 million |
| 5 | 34,359,738,368 | ~34 billion |

**Five levels of indirection can summarize the entire history of human civic activity.**

### Delta Encoding

The tree structure naturally encodes deltas:

```
                    ┌─────────────┐
                    │  Root v2    │
                    │ refs: [v1,  │◄─── "What changed since v1"
                    │        Δ]   │
                    └─────────────┘
                          │
           ┌──────────────┼──────────────┐
           ▼                             ▼
    ┌─────────────┐              ┌─────────────┐
    │  Root v1    │              │   Delta Δ   │
    │ (previous)  │              │ refs: [new] │
    └─────────────┘              └─────────────┘
```

To sync from v1 to v2, you only need the delta—not the entire history. This is exactly how Git pack files and rsync work.

### Summary: Compression Strategy

| Pattern | When to use | Compression ratio |
|---------|-------------|-------------------|
| Direct refs | < 128 items | 1:1 (no compression) |
| Checkpoint | Linear history | O(1) per segment |
| Merkle tree | Large batches | O(log N) depth |
| Delta | Incremental sync | Only new items |

The 128-ref limit is not a constraint—it's a **commitment primitive**. Any history, no matter how large, can be committed to in a single receipt with O(log N) intermediate nodes.

### Commitment vs Availability

This is important enough to state twice:

| Concept | What it means | Where it lives |
|---------|---------------|----------------|
| **Commitment** | "I attest that I knew about these receipts" | Kernel (refs) |
| **Availability** | "Others can retrieve these receipts" | Replication layer |

A tree of aggregation receipts proves *commitment*. It does **not** guarantee:
- The intermediate nodes exist anywhere
- Anyone else has copies
- The leaves are retrievable

If you need verifiable availability, you need additional machinery:
- Replication protocols (sync to N nodes)
- Availability proofs (data availability sampling)
- Inclusion proofs (prove leaf exists in tree without fetching all)

The kernel provides commitment. Availability is a replication-layer concern.

---

## Summary

```
refs.len() = 0   →  Genesis (root node, no dependencies)
refs.len() = 1   →  Chain (linear history, single parent)
refs.len() > 1   →  Merge (convergence, multiple parents)

The DAG emerges from independent actions.
No central coordinator. No timestamps required.
Hash references ARE the causal clock.
```

---

## Layer Separation (Meta-Note)

This document describes **kernel physics**—what the refs primitive enables structurally. There are two other layers this document intentionally does not cover:

| Layer | Concerns | Documents |
|-------|----------|-----------|
| **Kernel** (this doc) | Receipts, refs, IDs, signatures, causal structure | SPEC.md, DAG_SEMANTICS.md |
| **Replication** | Sync protocols, availability, indexing, query, filtering | SYNC.md (future) |
| **Governance** | Trust policies, permissions, equivocation penalties, dispute resolution | CONVENTIONS.md, application-specific |

The kernel is intentionally minimal. It provides:
- Content-addressed identity
- Cryptographic authenticity
- Causal ordering primitive

It does **not** provide:
- Total ordering
- Availability guarantees
- Access control
- Conflict resolution
- Anti-spam / anti-abuse
- State management

These are real requirements. They live in the replication and governance layers, not the kernel.

---

*For implementation details, see SPEC.md §10 (Refs: The Primitive for Relationships).*
