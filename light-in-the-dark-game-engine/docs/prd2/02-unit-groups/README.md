# 02 — Persistent Unit-Group Store

> **Requirement namespace:** `R-UGR-*`
> **Primary tick phase:** phase 7 (cleanup) for auto-pruning; reads any phase
> **New sub-hash:** `"unitgroups"`
> **New caps:** `Caps.UnitGroups` (default 1,024), `Caps.GroupMembers` (default 65,536)

---

## Problem

The "group" — a durable set of units you can name, grow, shrink, iterate, and operate on
as a whole — is one of the most-used WC3 primitives. The tutorial corpus uses groups for
one-click kills, AOE damage application, AI team iteration, escort waves, lowest-HP target
search, and crowd (audience) animation. Today the engine only offers **transient
enumeration**: `QueryEnumUnits`-style callbacks that visit units once and forget them.
There is no object you can store in a variable, pass to a function, accumulate into over
time, or save.

This forces every map to re-implement groups as Lua tables — which iterate in
nondeterministic order, churn garbage, and silently fail to serialize.

## Solution

A **persistent unit-group store**: a fixed pool of durable, handle-addressed, ordered,
duplicate-free sets of `EntityID`, backed by a shared membership arena. Insertion-ordered
iteration, O(1) add/remove, full set algebra (union/intersect/difference), deterministic
query-fill (radius/rect/owner/type/predicate), and automatic pruning of dead members.
Serializable and hashed like every other store.

## Design highlights

- **Ordered + unique.** Iteration yields members in insertion order (serialized), so
  "first matching unit" and "for each in order" are deterministic without sorting.
- **Shared membership arena.** Groups don't each own a fixed-size array; they index into
  one large `GroupMembers` arena via per-group `(start,len)` spans with a free-span
  allocator, so 1,024 groups can have wildly different sizes without waste.
- **Auto-pruning.** Dead handles are removed in cleanup; iteration never yields a stale
  unit (R-UGR-3).
- **Zero-alloc set algebra.** All combinators write into a destination group using
  preallocated scratch; no temporary slices.

## What this unlocks

- AOE abilities: fill a group by radius, iterate, damage each (the canonical
  electric-shock/nova pattern).
- Spawners & AI: keep the camp's spawned units in a group; "is the group empty?" drives
  respawn; iterate the group to issue orders.
- Quests: accumulate killed/collected units into a group; `Count()` is the progress
  number.
- The [mover](../05-movers/) and [ability](../06-ability-composition/) layers target groups
  for multi-hit and chain effects.

## Documents

- [spec.md](spec.md) — data model, arena, operations, ordering, pruning.
- [api.md](api.md) — Go API and Lua binding.
- [test-plan.md](test-plan.md) — determinism, set-algebra correctness, zero-alloc, FSV.
