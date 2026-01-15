# DAG Semantics: How Refs Create Causal Structure

**Version**: 0.5.0
**Date**: 2026-01-15

This document explains how the `refs` field creates a directed acyclic graph (DAG) of receipts, establishing causal relationships without central coordination.

---

## Core Principle

```
If B.refs contains A.receipt_id, then A happened-before B.
```

This is **Lamport's happened-before relation** applied to receipts. The receipt ID (a SHA-256 hash) serves as a cryptographic commitment: B could not have been created without knowledge of A.

The resulting structure is a **Merkle DAG**—a directed acyclic graph where edges are hash references. This is the same primitive underlying Git, IPFS, and certificate transparency logs.

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

**Key insight**: B and C are *concurrent*—neither happened-before the other. They exist in the same partial order but are incomparable. This is a feature: independent actors don't need coordination.

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

## Convergence

Two nodes that sync will converge to the same DAG:

```
Node 1: {A, B, C}
Node 2: {A, D, E}

After sync:
Node 1: {A, B, C, D, E}
Node 2: {A, B, C, D, E}
```

This works because:
1. Receipts are immutable (no updates, only appends)
2. IDs are content-addressed (same content = same ID)
3. Sync is set union (no conflict resolution needed)

This is why "merge = union" is a kernel invariant. The DAG is a **join-semilattice**: any two states have a unique least upper bound (their union).

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

*For implementation details, see SPEC.md §10 (Refs: The Primitive for Relationships).*
