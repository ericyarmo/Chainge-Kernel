# Chainge Kernel Conventions

**Version**: 0.5.0
**Date**: 2026-01-15

This document defines **blessed patterns** for building on top of the kernel. These conventions can evolve without breaking the kernel specification.

The kernel provides the primitive (signed receipts with refs). Conventions provide the semantics.

---

## 1. Tombstone Pattern

**Purpose**: Mark a receipt as logically deleted.

```
Tombstone Receipt {
    author:  same as target (or authorized delegate)
    schema:  "tombstone/v1"
    refs:    [target_receipt_id]
    payload: { "reason": "..." }  // optional
}
```

**Semantics**:
- A tombstone receipt references the receipt being deleted
- Only the original author (or a delegate) should create tombstones
- Applications query: "does this receipt have a tombstone from an authorized author?"

**Why convention, not kernel?**
- Deletion semantics are domain-specific
- Some apps may allow un-delete (tombstone the tombstone)
- Some apps may require multi-party approval for deletion

---

## 2. Chain Pattern

**Purpose**: Create ordered sequences of receipts.

```
Receipt A (genesis):
    refs: []

Receipt B (follows A):
    refs: [A.receipt_id]

Receipt C (follows B):
    refs: [B.receipt_id]
```

**Semantics**:
- `refs[0]` is the "previous" receipt in the chain
- Single-author chains are append-only logs
- Multi-author chains require coordination (or merge)

**Variations**:
- **Strict chain**: Exactly one ref pointing to previous
- **Sparse chain**: Skip refs for efficiency, verify by walking

---

## 3. Head Pattern

**Purpose**: Track the "current state" of a mutable entity.

```
Head Pointer Receipt {
    author:  entity owner
    schema:  "head/v1"
    refs:    [current_tip_receipt_id]
    payload: { "entity": "user/alice/profile" }
}
```

**Semantics**:
- The head pointer is itself a receipt (immutable, signed)
- To update: create new data receipt, then new head pointing to it
- Query: find latest head receipt for entity, follow refs to current state

**Why convention, not kernel?**
- "Current" is application-defined
- Different conflict resolution strategies exist
- Some apps want multi-head (branches)

---

## 4. Countersign Pattern

**Purpose**: Witness or endorse another receipt.

```
Countersign Receipt {
    author:  witness (different from original)
    schema:  "countersign/v1"
    refs:    [original_receipt_id]
    payload: { "verdict": "confirmed" }
}
```

**Semantics**:
- The countersigner attests they've seen the original
- Does NOT mean they verify the original's claims
- Multiple countersigns build consensus

**Trust implications**:
```
1 signature  = claim
2 signatures = witnessed claim
N signatures = social consensus
```

---

## 5. Delegate Pattern

**Purpose**: Authorize another key to act on your behalf.

```
Delegation Receipt {
    author:  delegator (original authority)
    schema:  "delegate/v1"
    refs:    []
    payload: {
        "delegate": "<delegate_public_key_hex>",
        "scope": ["tombstone", "head"],
        "expires_ms": 1736956800000
    }
}
```

**Semantics**:
- Delegator authorizes delegate for specific actions
- Scope limits what operations delegate can perform
- Expiration is advisory (applications enforce)

**Revocation**:
- Tombstone the delegation receipt
- Or create new delegation with empty scope

---

## 6. Merge Pattern

**Purpose**: Combine multiple branches or receipts.

```
Merge Receipt {
    author:  merger
    schema:  "merge/v1"
    refs:    [branch_a_tip, branch_b_tip, ...]
    payload: { "strategy": "union" }
}
```

**Semantics**:
- Merge receipt references all branches being merged
- Payload describes merge strategy (application-specific)
- Creates new unified tip for chain continuation

---

## 7. Batch Pattern

**Purpose**: Group related receipts atomically.

```
Batch Receipt {
    author:  batch creator
    schema:  "batch/v1"
    refs:    [receipt_1, receipt_2, ..., receipt_n]
    payload: { "operation": "import" }
}
```

**Semantics**:
- All referenced receipts are part of the same logical operation
- Applications can process batch atomically
- Useful for migrations, bulk operations

---

## 8. Schema Registry Pattern

**Purpose**: Define and version payload schemas.

```
Schema Definition Receipt {
    author:  schema authority
    schema:  "schema/v1"
    refs:    [previous_version_id]  // or [] for v1
    payload: {
        "uri": "civic.inspection/v1",
        "spec": { ... schema definition ... }
    }
}
```

**Semantics**:
- Schema URIs resolve to schema definition receipts
- Refs chain schema versions together
- Applications validate payloads against schemas

---

## 9. Entity Resolution Pattern

**Purpose**: Map human-readable names to receipt IDs.

```
Name Binding Receipt {
    author:  naming authority
    schema:  "name/v1"
    refs:    []
    payload: {
        "name": "alice",
        "namespace": "users",
        "target": "<receipt_id_hex>"
    }
}
```

**Semantics**:
- Names are claims by naming authorities
- Multiple authorities may claim same name (conflict)
- Applications choose which authorities to trust

---

## 10. Encryption Envelope Pattern

**Purpose**: Store encrypted data in receipts.

```
Encrypted Receipt {
    author:  data owner
    schema:  "encrypted/v1"
    refs:    []
    payload: {
        "algorithm": "xchacha20-poly1305",
        "recipients": ["<pubkey_hex>", ...],
        "ciphertext": "<base64>"
    }
}
```

**Semantics**:
- Kernel stores opaque bytes (doesn't know it's encrypted)
- Schema signals encryption to applications
- Key exchange is out of band

---

## Pattern Composition

Patterns compose naturally:

```
Encrypted + Chain:
    Private append-only log

Tombstone + Delegate:
    Authorized deletion

Head + Countersign:
    Witnessed state updates

Batch + Schema:
    Typed bulk imports
```

---

## Versioning Conventions

**Schema URIs** follow the pattern: `<domain>.<type>/v<N>`

Examples:
- `civic.inspection/v1`
- `civic.presence/v1`
- `tombstone/v1`
- `head/v1`
- `delegate/v1`

**Breaking changes** require new version number.

---

## Trust Model

Conventions don't change the kernel's trust model:

1. **Signatures prove authorship**, not truth
2. **Refs prove ordering**, not validity
3. **Countersigns prove witnessing**, not endorsement

Applications build trust by:
- Requiring countersigns from trusted parties
- Validating delegation chains
- Checking tombstone authority
- Enforcing schema constraints

---

*These conventions are recommendations. Applications may define their own patterns using the same kernel primitives.*

*For the kernel specification, see SPEC.md.*
