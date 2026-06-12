# ECS Architecture (R-SIM-3)

**Parent:** [PRD §5.1, §5.3.1](../../PRD.md) · **Related:** [Determinism](determinism.md) · [Tick & Scheduler](tick-and-scheduler.md) · [Pathfinding](pathfinding.md) · [Combat & Orders](combat-and-orders.md)

---

## 1. Requirement statement

R-SIM-3 mandates a data-oriented ECS with **struct-of-arrays (SoA) component stores** for units, missiles, and buffs, provisioned for **1,000 active units plus 1,000 missiles** per tick (the D-2026-06-11-18 stretch target, met on the recommended-spec machine); 500 + 500 within the 10 ms tick budget on the dual-core 2 GHz low-tier reference machine remains the guaranteed budget (PRD §5.3). *Revised 2026-06-11 per D-2026-06-11-18.* R-GC-2 binds the memory model: component stores are preallocated slices whose **capacity is fixed at map load and never reallocates mid-match**; all transient gameplay objects come from pools carved from the same allocation. R-GC-1/R-GC-5 make zero allocations per tick a CI-enforced invariant.

The design below is deliberately conservative: no archetype migration, no sparse-set generality beyond what an RTS needs, no reflection-driven registration. Predictability over generality — the component set of an RTS is known at compile time.

## 2. Memory model: fixed capacity at map load (R-GC-2)

At map load, the map header declares capacity limits (with engine-enforced ceilings). *Capacities revised 2026-06-11 per D-2026-06-11-18 (1,000-unit stretch target; 500 remains the low-tier guarantee) and D-2026-06-11-13 (scripted-doodad pool):*

| Pool | Default cap | Notes |
|---|---|---|
| Units | 4,000 | 1,000 *active* is the stretch perf target (500 guaranteed low-tier); cap leaves headroom for corpses-in-decay, summons |
| Missiles | 2,000 | pooled, high churn; 1,000 live is the perf target |
| Buff instances | 8,000 | many-per-unit |
| Order queue entries | 16,000 | pooled, see [Combat & Orders](combat-and-orders.md) |
| Pending events | 4,096/tick | ring buffer |
| Path requests/results | 512 in flight | see [Pathfinding](pathfinding.md) |
| Scripted doodads | 1,024 | render-only until first script touch, then promoted (§5, D-2026-06-11-13) |

Every component store allocates `make([]T, cap)` once during load. Slices are never appended past capacity; exhaustion is a *gameplay* outcome (creation fails, exactly as WC3 refuses to exceed food/handle limits) plus a debug-mode assert — never a reallocation. This guarantees stable backing arrays (pointers/indices into stores remain valid for the match), zero steady-state GC pressure, and a hard, knowable RAM ceiling that fits the 1.5 GB match budget (PRD §5.3).

## 3. Entity ID and generation scheme

An entity handle is a packed 32-bit value:

```go
// EntityID: [ generation:8 | index:24 ]
type EntityID uint32

func (e EntityID) Index() uint32      { return uint32(e) & 0x00FFFFFF }
func (e EntityID) Generation() uint8  { return uint8(e >> 24) }
```

- **Index** addresses a slot in the entity table (24 bits ≫ any pool cap).
- **Generation** is a per-slot counter incremented on every slot reuse. A stale handle whose generation no longer matches the slot resolves to "dead entity". This implements R-API-5's WC3 semantics directly: operations on invalid handles become zero-value no-ops, with a debug-mode assert.
- **Free list:** dead slots go onto an intrusive free list (a `next` index stored in the dead slot itself — no separate allocation). Free-list pop order is LIFO and therefore deterministic; entity creation order is part of simulation state and is covered by the [state hash](determinism.md).
- Generation wrap (256 reuses of one slot) is accepted: at 20 Hz a slot would need pathological churn to wrap within a match, and the consequence (a one-in-256 stale-handle false positive) is a no-op order, not corruption. If replay forensics ever show it matters, the ID widens to `uint64` with 16-bit generation — an ABI change confined to one type.

The public API's `Unit`, `Item`, etc. objects (PRD §4.3) are thin wrappers holding an `EntityID` plus a `*World`; they are value types and allocate nothing (R-GC-3).

## 4. Component stores: struct-of-arrays

Each component is a plain struct-of-slices store, indexed by a dense component row, with a mapping between entity index and component row:

```go
type TransformStore struct {
    // dense, parallel columns — one cache-friendly stream per field
    Pos      []fixed.Vec2   // 32.32 fixed-point (D-2026-06-11-1), see determinism.md
    Facing   []fixed.Angle
    Entity   []EntityID     // row -> owning entity (for iteration)
    rowOf    []int32        // entity index -> row, -1 if absent (sparse, cap = entity cap)
    count    int32
}
```

- **SoA, not AoS:** systems touch only the columns they need (movement reads `Pos`/`Facing` and never drags combat stats through cache). This is where the 500-unit low-tier guarantee — and the 1,000-unit stretch target (D-2026-06-11-18) — is won.
- **Dense iteration:** rows `[0, count)` are always live and contiguous. Removal is swap-with-last (copy last row into the vacated row, fix `rowOf` for the moved entity, decrement count). Swap-remove changes iteration order — which is fine *because* the order remains fully deterministic (it depends only on the deterministic history of adds/removes) and the [state hash](determinism.md) hashes stores in row order, so any ordering bug surfaces immediately.
- **No archetypes:** with ~10 component types and known access patterns, per-component dense stores beat archetype machinery in both simplicity and worst-case behavior. Cross-component joins iterate the smaller store and probe `rowOf` of the other — O(1) per probe, no allocation.
- **Stable IDs where scripts need them:** scripts hold `EntityID`s, never rows; rows are an internal, ephemeral concept.

## 5. Component list (RTS baseline)

| Component | Key fields | Notes |
|---|---|---|
| **Transform** | position (Vec2 fixed), facing, height offset | the only component every entity has |
| **Movement** | speed, turn rate, target waypoint, path handle, move state | consumes [pathfinding](pathfinding.md) results |
| **Collision** | collision size (radius class), pathing flags (ground/air/build), grid stamp ref | drives grid occupancy, WC3 collision-size semantics |
| **Health** | life, max life, regen, armor value, armor type, death state/decay timer | armor type indexes the [damage table](combat-and-orders.md) |
| **Combat** | attack(s): damage dice, attack type, cooldown, range, acquisition range, damage point, missile ref, current target, cooldown clock | see [attack cycle](combat-and-orders.md) |
| **Ability** | ability slots: ability ID, level, cooldown clock, mana cost ref, cast state machine, effect-pipeline ref | definitions composed from registered plugin mechanisms ([Combat & Orders §5.1](combat-and-orders.md)); data-table-driven per R-AST-1 |
| **Buff** | (instance store) buff ID, target entity, source entity, remaining ticks, stack count, periodic clock | buffs are entities-lite: pooled instances, not full entities |
| **Inventory** | item slots (6, WC3-style), item entity refs | items are entities with their own components |
| **Order** | current order, order queue head (pooled linked entries) | see [order system](combat-and-orders.md) |
| **Owner** | player index, team, color | |
| **UnitType** | unit-type ID → immutable data-table row (stats, model, sounds) | the SLK analogue, R-AST-1 |
| **Missile** | source, owner, guidance state (target entity / point / direction), speed, acceleration, arc, payload ref (damage packet or effect-pipeline ref), guidance ID, impact-behavior ID, TTL/range, hit-mask | independent first-class entity, dedicated store + pool; [Combat & Orders §3.5](combat-and-orders.md) |

Render-side concerns (model instance, animation clip state, selection circle) live in `litd/render` mirror structures keyed by `EntityID`, never in sim components — the PRD §4.1 hard rule.

**Doodads — promotion on first touch (D-2026-06-11-13).** *Added 2026-06-11 per D-2026-06-11-13.* Doodads default to render-side-only storage: no entity, no component rows — the zero-cost case stays zero-cost for the thousands of placed scenery objects a map carries. The first time a script addresses a doodad (show/hide, the `SetDoodadAnimation` analogues, reposition), it is **promoted**: an `EntityID` is allocated from the scripted-doodad pool (§2) and a small **Doodad component row** is created holding the map-placement index, visibility flag, animation override, and position/facing override; render resolves a promoted doodad through the `EntityID`-keyed mirror instead of the static placement list. Promotion happens in script execution order (deterministic), is one-way for the match, and promoted rows are authoritative state — covered by the [state hash](determinism.md) and the save format (R-SIM-6). Doodads with pathing footprints stamp the [grid](pathfinding.md) at map load regardless of promotion state.

### 5.1 Memory math (sanity check against the 1.5 GB budget)

Back-of-envelope at the revised default caps, 32.32 fixed-point positions ([Determinism §2](determinism.md)). *Revised 2026-06-11 per D-2026-06-11-18 (capacities) and D-2026-06-11-1 (32.32 field widths):*

| Store | Approx bytes/row | Rows | Total |
|---|---|---|---|
| Transform | ~32 | 4,000 | 128 KB |
| Movement (incl. waypoint buffer refs) | ~64 | 4,000 | 256 KB |
| Combat (2 weapon slots) | ~160 | 4,000 | 640 KB |
| Health / Collision / Owner / Order heads | ~64 | 4,000 | 256 KB |
| Buff instances | ~40 | 8,000 | 320 KB |
| Missiles (first-class entities, §3.5 fields) | ~128 | 2,000 | 256 KB |
| Order queue pool | ~24 | 16,000 | 384 KB |
| Doodad rows (promoted, D-13) | ~32 | 1,024 | 32 KB |
| Pathing grid + cliff levels + dilated layers + buckets + flow-field slots | — | — | ~5–6 MB |

Even at doubled capacities and doubled positional widths, the entire authoritative sim state is **single-digit megabytes** — it still fits in L3 cache on the reference CPU, which is the real point of the SoA discipline. The 1.5 GB match budget (PRD §5.3) is consumed almost entirely by render-side assets, not the sim.

## 6. System execution order within a tick

Systems run **strictly sequentially, in a fixed registration order** — single-threaded inside the tick per R-SIM-5 and [Determinism §2.3](determinism.md). The order (full phase context in [Tick & Scheduler](tick-and-scheduler.md)):

1. **CommandIngest** — apply this tick's player/script commands to Order components
2. **ScriptScheduler** — resume due coroutines (R-EXEC-1/2/5)
3. **OrderResolve** — pop/translate orders into system intents (move target, attack target, cast)
4. **AbilityAndBuff** — cast state machines, buff ticks/expiry
5. **Pathing** — path requests, grid updates from building stamps
6. **Movement** — integrate positions, local avoidance, facing turns
7. **Combat** — acquisition, attack cycles, damage application
8. **Missiles** — advance per guidance program, hit-test, run impact behaviors, deliver payloads ([Combat & Orders §3.5](combat-and-orders.md))
9. **Death/Cleanup** — process kills, decay, swap-remove, free-list returns, pool recycling
10. **EventFlush** — deterministically ordered event dispatch to handlers (R-EXEC-2)
11. **StateHash** (CI/checkpoint cadence) — see [Determinism §5](determinism.md)

Within each system, iteration is over the dense store in row order. Where a system's *effects* could be order-sensitive between entities in the same tick (e.g. two units killing each other), effects are written to a deferred buffer during iteration and applied in a second pass — same discipline WC3-likes use to make "simultaneous" deaths well-defined.

## 7. Zero-alloc iteration patterns in Go (R-GC-1/3)

The tick path must show `testing.AllocsPerRun == 0` in CI. Patterns mandated:

- **Index loops over stores, not abstractions:** `for i := int32(0); i < s.count; i++ { ... s.Pos[i] ... }`. No iterator objects, no `range` over channels, no callback-style `ForEach(func(...))` — a closure capturing loop state allocates, and Go cannot always inline through function-valued parameters.
- **No interface values in hot paths:** systems are concrete structs invoked directly from a hand-written tick function, not through a `System` interface slice (interface method calls also block inlining and risk boxing). The system *order* is code, reviewed like code.
- **Preallocated scratch:** every system owns its scratch buffers (open lists, neighbor arrays, deferred-effect buffers) sized at map load and reset by `buf = buf[:0]` — re-slicing keeps capacity, allocates nothing.
- **Value-type payloads:** events, damage packets, and orders are fixed-size structs copied into pooled ring buffers — no `interface{}`/`any` payloads, no `fmt` formatting (R-GC-3 confines string work to debug builds behind a build tag).
- **Escape-analysis hygiene:** anything that must not escape is kept method-local or pointer-into-store; CI runs the alloc benchmarks with `-gcflags=-m` output archived so escape regressions are diagnosable, and R-GC-5 fails the build on any nonzero delta.
- **Pools are typed arrays + free lists,** not `sync.Pool` — `sync.Pool` is non-deterministic in what it returns and GC-emptied, both unacceptable here.

The canonical join, written out (movement consuming transforms), to fix the idiom:

```go
func (s *MovementSystem) Step(w *World) {
    mv, tf := &w.Movement, &w.Transform
    for i := int32(0); i < mv.count; i++ {
        if mv.State[i] != MoveActive {
            continue
        }
        e := mv.Entity[i]
        t := tf.rowOf[e.Index()] // O(1) probe; t >= 0 guaranteed by invariant:
        // Movement requires Transform, enforced at add-time, debug-asserted here.
        tf.Pos[t] = step(tf.Pos[t], mv.Waypoint[i], mv.SpeedPerTick[i])
    }
}
```

No closures, no interfaces, no bounds beyond the slice's own checks (which the compiler hoists for the `[:count]` pattern), no allocation. Every system in `litd/sim` reads like this; cleverness is spent in the data layout, not the loop bodies.

## 8. Acceptance hooks

- Headless benchmark (R-SIM-4), two gates from M3 (*revised 2026-06-11 per D-2026-06-11-18*): 500 units + 500 missiles ≤ 10 ms/tick on the low-tier reference machine (the guarantee, PRD §5.3), and 1,000 units + 1,000 missiles ≤ 10 ms/tick on the recommended-spec machine (the stretch target).
- `AllocsPerRun` benchmarks per system and for the whole tick — zero baseline, regression-fails per R-GC-5.
- Store-order determinism is implicitly verified by the [10k-tick hash trace](determinism.md): any unordered structure in a store would diverge the trace immediately.
