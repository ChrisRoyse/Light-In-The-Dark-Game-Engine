# Architecture Principles (Inherited by Every PRD2 Subsystem)

> These are the concrete, code-grounded patterns that each of the five primitives MUST
> follow. They are extracted from the existing engine so that PRD2 subsystems look,
> serialize, and hash like the stores already in `litd/sim`. Citations are to real code
> as of 2026-06-23. **Conform to these or justify the deviation in the subsystem spec.**

---

## 1. The SoA store pattern

Every PRD2 subsystem that holds per-row state is a Structure-of-Arrays store, matching
`store_health.go` / `store_movement.go`.

```go
// Canonical shape (mirrors litd/sim/store_movement.go:24-39)
type FooStore struct {
    // --- dense parallel columns, one per field ---
    FieldA []fixed.F64
    FieldB []uint32
    Owner  []EntityID      // row → owning entity (when entity-scoped)

    // --- index + bookkeeping ---
    rowOf  []int32         // entity.Index() → row, -1 if absent
    count  int32           // live rows are always contiguous in [0, count)

    DebugAssert func(msg string, id EntityID) // optional, debug builds only
}
```

Rules:
- **Dense rows, contiguous `[0, count)`.** Removal swaps the dead row with `count-1` and
  decrements `count` (swap-remove). The swapped row's `rowOf` entry is updated.
- **`rowOf` is the sparse map**, sized to the entity index space; one probe resolves a
  handle to a row. Absent ⇒ `-1`.
- **No `map[...]` in the hot path or in any state that participates in hashing.** Map
  iteration order is nondeterministic in Go and is forbidden in gameplay code
  (master PRD, R-SIM-2 neighborhood). Where a logical "map from key to value" is needed
  (the KV store, the event-kind registry), implement it as **sorted parallel arrays with
  binary search**, never a Go map in persisted/hashed state.

## 2. Handles, generations, and stale-safety

All cross-subsystem references use the existing 32-bit packed handle
(`litd/sim/entity.go`):

```go
type EntityID uint32 // [ generation:8 | index:24 ]
func (e EntityID) Index() uint32      // low 24 bits
func (e EntityID) Generation() uint8  // high 8 bits
```

PRD2 introduces three *new* handle types that follow the identical packing discipline so
they are cheap to store, compare, and serialize:

| Handle | Subsystem | Packing |
|--------|-----------|---------|
| `TimerID` | Timer wheel | `[ generation:8 | index:24 ]` |
| `GroupID` | Unit groups | `[ generation:8 | index:24 ]` |
| `EventKindID` | Custom events | flat `uint16` (small, registry-indexed) |
| `MoverID` | Movers | `[ generation:8 | index:24 ]` |

- **Generation counter** increments on slot reuse; a handle whose generation does not
  match the live slot resolves to a **safe no-op** (read returns zero/false; mutate is
  ignored). This is the WC3 "dead handle" semantics and R-API-5.
- Handles are **values**, never pointers. They serialize as their integer.

## 3. Fixed capacity, set once, no growth

Per R-GC-2, pools are sized exactly once in `NewWorld` from the `Caps` struct
(`litd/sim/world.go:15-44`). PRD2 adds new cap fields:

```go
type Caps struct {
    // ... existing ...
    Timers      int // default 4,096   (01-timer-wheel)
    UnitGroups  int // default 1,024   (02-unit-groups)
    GroupMembers int // default 65,536 (02 — total membership slots across all groups)
    KVPairs     int // default 32,768  (03-keyvalue-store)
    CustomEventKinds int // default 256 (04-custom-events)
    Movers      int // default 4,096   (05-movers, shared by units+missiles)
}
```

- **No `append`-growth on store columns after construction.** Exhaustion returns a
  zero/invalid handle and increments a deterministic drop counter (matching the missile
  ring and damage-buffer posture). It never panics and never allocates.
- Per-tick transient lists are **preallocated and reset via `slice[:0]`**, never
  re-`make`'d (matches `buffScratch`, `dmgBuf`, `areaScratch` in `world.go`).

## 4. Determinism & hashing discipline (R-SIM-6)

Each subsystem registers a **named sub-hash** in the `statehash` registry, appended to
`HashSystems` in a fixed order (`litd/sim/hash.go`). PRD2 appends, in this order:

```
... existing systems ...,
"timers", "unitgroups", "kv", "customevents", "movers"
```

Within a sub-hash:
1. Write `count` first.
2. Iterate rows in **canonical order** (see §5) — never map order.
3. Write each field in **struct-declaration order**, using the width-typed writers
   (`WriteU32`, `WriteI64`, `WriteU8`, `WriteBool`, …).
4. The save serializer for the same subsystem MUST write the **identical fields in the
   identical order** so that "what is hashed == what is saved" (the `hash.go` ↔ `save.go`
   mirror invariant).

Any divergence localizes to one named system via `FirstDivergence`
(`litd/sim/determinism.go`).

## 5. Canonical iteration order

"Canonical order" means an order that is a pure function of state, identical on every
machine and across save/load:

- **Entity-scoped rows** (KV pairs, group members): order by `(ownerEntity, key)` using
  the packed integer ordering, OR preserve insertion order in a dense array whose order
  is itself serialized. PRD2 prefers **dense insertion-ordered arrays whose order is part
  of the saved state** (matching how the missile/buff pools serialize free-list order),
  because it avoids per-tick sorting and is trivially stable.
- **Free/alloc pools** (timers, groups, movers): serialize the **free-list LIFO order**
  so that post-load allocations reproduce the same slot assignments (matches existing
  pool serialization).

## 6. Tick-phase placement

Each subsystem is written in exactly one tick phase to keep writes race-free and ordered
(phases from `litd/sim/step.go:70-93`):

| Subsystem | Primary phase | Rationale |
|-----------|---------------|-----------|
| Timers | **phase 2 (scripts)** for firing; expiry sweep folded into the scheduler drain | Timers wake continuations, exactly like the scheduler; co-locating keeps wake order single-sourced. |
| Unit groups | **phase 7 (cleanup)** for auto-pruning dead members; reads allowed any phase | Dead-member pruning must happen after the death sweep. |
| KV store | written by whichever phase's system mutates it; no autonomous tick | Pure storage; no self-driven behavior. |
| Custom events | **phase 6 (events)** dispatch, same ring as built-in events | Reuses the existing emission ring + ordering. |
| Movers | **phase 4 (movement)** | Same phase as unit movement & missile integration today. |

## 7. Public surface discipline (R-API-1..6)

Every primitive is reachable two ways, with identical semantics:
- **Go API** (`litd/api/…`): methods on nouns (`*Game`, `Unit`, `Group`, `Timer`,
  `Mover`), value-type math (`Vec2`, `time.Duration`), options structs for anything with
  >3 parameters, **no G3N types in any signature**.
- **Lua binding** (`litd/luabind/…`): thin wrappers over the same sim calls, taking and
  returning numbers/strings/handles. The Lua surface is what AI agents target most, so it
  is documented per-subsystem in each `api.md`.

## 8. Failure posture

- Invalid handle ⇒ no-op (read: zero value; write: ignored), never panic.
- Pool exhaustion ⇒ invalid handle + drop counter, never panic, never alloc.
- Contract violations in debug builds may call `DebugAssert`; in release they degrade to
  the no-op posture above. Determinism is never sacrificed to report an error.

---

### Conformance checklist (copy into each subsystem spec)

- [ ] SoA store; contiguous `[0,count)`; swap-remove; `rowOf` sparse index (no map in hashed state).
- [ ] Generation-checked value handle; stale ⇒ no-op.
- [ ] Capacity from `Caps`; set once; exhaustion ⇒ invalid handle + drop counter; zero alloc.
- [ ] Named sub-hash appended to `HashSystems`; fields hashed in declaration order; save mirrors hash.
- [ ] Canonical iteration order defined and serialized.
- [ ] Single primary tick phase named.
- [ ] Go API (value types, no G3N) + Lua binding, identical semantics.
- [ ] Covered by a `testing.AllocsPerRun` zero-alloc gate and a determinism replay test.
