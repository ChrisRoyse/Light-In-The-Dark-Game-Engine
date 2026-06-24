# Unit-Group Store — Specification

## 1. Data model

```go
// litd/sim/unitgroup.go (new)

type GroupID uint32 // [ generation:8 | index:24 ]; stale ⇒ no-op

type GroupStore struct {
    // Per-group rows (SoA), one row per allocated group.
    Start []int32     // index into the shared Members arena
    Len   []int32     // member count for this group
    Cap   []int32     // span capacity reserved in the arena (Len <= Cap)
    Gen   []uint8     // generation for handle validation
    live  []bool

    // Shared membership arena: all groups' members live here.
    Members []EntityID // size Caps.GroupMembers; group g owns Members[Start[g] : Start[g]+Len[g]]

    free      []int32     // free group slots (LIFO)
    spanAlloc spanAllocator // free-span allocator over Members (best-fit or buddy)
    count     int32

    // membership de-dup acceleration: for a given group, a small open-addressed probe
    // over its own span is O(Cap); for hot large groups an optional per-group presence
    // bitset in a side arena keyed by entity index keeps add() O(1). The bitset is a
    // derived index (rebuilt on load), NOT hashed.
    presence presenceIndex
}
```

### Why a shared arena (R-UGR-7)
1,024 groups × a fixed per-group array would either waste memory (every group sized for
the worst case) or cap group size arbitrarily. A single `Members` arena of 65,536 slots
with a free-span allocator lets one boss-wave group hold 400 units while a thousand other
groups hold 0–5 each. When a group grows past its reserved `Cap`, the span allocator
relocates it to a larger free span (copying `Len` entities — bounded, no Go alloc) or, if
the arena is full, the add fails with a dropped-member counter.

> **Simpler first cut.** A v1 MAY give every group a fixed small cap (e.g. 64) drawn from
> a flat `Caps.UnitGroups × 64` arena and only add the span allocator when a real map needs
> large groups. The API and hash format are identical either way; only the allocator behind
> `Start/Len/Cap` changes, and that is non-hashed implementation detail save for the member
> bytes themselves.

## 2. Membership invariants

- **Unique:** `Add(g, e)` is a no-op if `e` already in `g` (checked via the presence
  index, O(1), or a linear span scan in the fixed-cap cut).
- **Insertion-ordered:** new members append at `Members[Start+Len]`; `Len++`. Order is
  meaningful and serialized (R-UGR-2).
- **Swap-remove:** `Remove(g, e)` swaps the member with the last and decrements `Len`
  (O(1)). **This breaks strict insertion order on removal** — see §3 for the ordered-remove
  variant and the determinism guarantee.

## 3. Ordering & removal semantics

Two removal disciplines, both deterministic:

- **`Remove` (fast, swap):** O(1), reorders. Use when iteration order need not survive a
  removal (the common AOE/iterate-and-kill case, where the group is rebuilt each use).
- **`RemoveOrdered` (stable):** O(n) shift, preserves insertion order. Use when the group
  is long-lived and order matters across edits (e.g. a turn queue).

Both are deterministic functions of state. The hash captures the resulting member order
either way, so two engines performing the same operations land on the same order.

Auto-pruning of dead members (cleanup phase) uses **stable order** by default so that
pruning a mid-list death does not perturb the order of survivors more than necessary; the
pruning pass itself is deterministic (it scans `Members[Start:Start+Len]` in index order
and compacts).

## 4. Operations (all zero-alloc at steady state)

| Op | Signature (sim) | Cost | Notes |
|----|------------------|------|-------|
| Create | `CreateGroup() GroupID` | O(1) | from `free`; reserves a starting span (or fixed cap) |
| Destroy | `DestroyGroup(g)` | O(1) | frees slot + span; bumps `Gen` |
| Add | `GroupAdd(g, e)` | O(1)* | unique; fails (drop counter) if arena full |
| Remove | `GroupRemove(g, e)` | O(1) | swap-remove |
| RemoveOrdered | `GroupRemoveOrdered(g, e)` | O(n) | stable |
| Clear | `GroupClear(g)` | O(1) | `Len=0` |
| Count | `GroupCount(g) int32` | O(1) | quest progress, emptiness |
| Contains | `GroupContains(g, e) bool` | O(1)* | presence index |
| Pick first | `GroupFirst(g) EntityID` | O(1) | order-deterministic |
| Iterate | `GroupEach(g, fn)` | O(n) | visits in member order; safe against in-loop removal via index snapshot |
| Union | `GroupUnion(dst, a, b)` | O(\|a\|+\|b\|) | into dst, dedup |
| Intersect | `GroupIntersect(dst, a, b)` | O(...) | into dst |
| Difference | `GroupDifference(dst, a, b)` | O(...) | a∖b into dst |
| Copy | `GroupCopy(dst, src)` | O(\|src\|) | |

\* O(1) with the presence index; O(Cap) linear in the fixed-cap first cut.

### Query-fill (R-UGR-6)
Deterministic population from spatial/predicate queries, reusing the existing collision
grid and owner/type stores:

```go
GroupFillRadius(g, center fixed.Vec2, radius fixed.F64, mask QueryMask)
GroupFillRect(g, rect Rect, mask QueryMask)
GroupFillOwner(g, player int32, mask QueryMask)
GroupFillType(g, unitType UnitTypeID, mask QueryMask)
```

`GroupFill*` first `Clear`s `g`, then visits candidate cells **in deterministic cell
order** (row-major over the query's cell range, then by entity index within a cell — never
map order), appending matches. `QueryMask` filters by alive/enemy/ally/structure/etc. The
visitation order *is* the resulting group order, so it is reproducible.

## 5. Auto-pruning (R-UGR-3)

In phase 7 (cleanup), after the death sweep produces the list of entities that died this
tick, the group store prunes them:

- Naïve "scan every group for every death" is O(groups × deaths). Instead, an optional
  **reverse membership index** (`entity index → list of (group, position)`), itself a
  non-hashed derived structure, makes pruning O(deaths × membership). For the fixed-cap
  first cut, a bounded scan over live groups is acceptable given the small caps.
- Pruning uses stable compaction (§3) and is deterministic.

Groups themselves are **not** auto-destroyed when empty; emptiness is a queryable state
(spawners rely on "group empty ⇒ respawn").

## 6. Capacity & exhaustion (R-UGR-7)

- `Caps.UnitGroups` (1,024) bounds group count; `Caps.GroupMembers` (65,536) bounds total
  membership across all groups.
- Group-slot exhaustion ⇒ `CreateGroup` returns invalid `GroupID(0)` + `groupDropped++`.
- Arena exhaustion ⇒ `GroupAdd` is a no-op + `groupMemberDropped++`.
- Both counters are hashed. No panic, no alloc.

## 7. Tick phase & hashing

- **Phase 7** owns autonomous mutation (pruning). All other mutation is caller-driven and
  may occur in the caller's phase.
- Sub-hash `"unitgroups"`: write `count`, the two drop counters, then for each live group
  in ascending slot order write `(packedHandle, Len)` followed by `Members[Start:Start+Len]`
  in member order; then the free group-slot list and the span-allocator free list (so
  future allocations reproduce). The presence/reverse indices are **not** hashed.
