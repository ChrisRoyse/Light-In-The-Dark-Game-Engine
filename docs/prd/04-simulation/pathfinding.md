# Pathfinding (R-SIM-5)

**Parent:** [PRD §5.1](../../PRD.md) · **Related:** [Determinism](determinism.md) · [ECS Architecture](ecs-architecture.md) · [Tick & Scheduler](tick-and-scheduler.md) · [Combat & Orders](combat-and-orders.md)

---

## 1. Requirement statement

R-SIM-5 requires deterministic A\*/flow-field pathfinding on a WC3-style grid, with **no threads inside tick resolution in v1** — pathfinding runs single-threaded inside the tick's movement phase, within an explicit time budget, and produces bit-identical results across platforms per [R-SIM-2](determinism.md). All grid math is fixed-point/integer; there is no float anywhere in the pathing subsystem.

## 2. The WC3-style pathing grid

### 2.1 Grid structure

*Revised 2026-06-11 per D-2026-06-11-7: heightmap terrain with cliff levels and ramps is the v1 terrain, not tile meshes.*

Warcraft III's pathing model is the template: the world is overlaid with a fine pathing grid at **4× the resolution of the terrain cell grid** — terrain cells of 128 world units are divided into 32×32-unit pathing cells. A 128×128-cell map is therefore a 512×512 pathing grid (262,144 cells), which at one byte of flags per cell is 256 KB — trivially preallocated at map load per R-GC-2 and cheap to scan.

v1 terrain is a **heightmap with discrete cliff levels and ramps** (D-7); the sim-side abstraction is unchanged (R-SIM-5: the sim sees the grid, never the mesh — tile meshes remain possible later behind the same abstraction). Alongside the flag byte, each pathing cell carries a **cliff level** (small integer, baked from the heightmap's discrete cliff layers at map load). Walkability semantics, heightmap-primary:

- Movement between two adjacent cells is legal only if they share a cliff level **or** at least one of them is a **ramp cell**. Ramp cells are authored regions connecting exactly two adjacent cliff levels; they carry both levels (and interpolate render height) — to the sim a ramp is simply a cell that joins level *L* and *L+1*.
- Cliff faces need no blocking flags: they *are* the level discontinuity, and the level-equality rule blocks them implicitly. The dilated collision layers (§2.2) bake the rule in per collision class.
- **Within-level height variation (rolling terrain) never affects walkability** — only the discrete cliff level does. Exactly WC3's model: smooth height is cosmetic, cliffs are gameplay. Steep-slope unwalkability, where a map wants it, is expressed by the map baking `Walkable` off — a data decision, not a slope computation in the sim.
- Cliff level is grid state covered by the [state hash](determinism.md), and it feeds the high-ground combat/vision rules defined elsewhere.

Each cell carries a flag byte:

| Flag | Meaning |
|---|---|
| `Walkable` | ground units may occupy |
| `Flyable` | air units may occupy (separate layer; air pathing is near-trivial) |
| `Buildable` | structures may be placed |
| `Blight`/reserved | faction-specific build rules, future use |
| `OccupiedStatic` | stamped by a building/destructable |
| `OccupiedDynamic` | reserved by a unit (see §5) |

Static flags (`Walkable`/`Flyable`/`Buildable` from the heightmap — water level, authored unwalkable regions — plus cliff levels, ramps, and map-placed doodads/destructables) are baked at map load. Destructables (trees) clear their stamp when destroyed — the WC3 tree-cutting mechanic falls out of the same path.

### 2.2 Collision sizes

Units have a **collision size** (radius in world units) from the unit data table (R-AST-1), quantized to a small set of classes exactly as WC3 does (e.g. 8 / 16 / 24 / 32 / 48 — final values are data, not code). Rather than test a disc against cells per query, the grid is conservatively **dilated per collision class**: for each class, a derived bitmap marks cells where a unit *center* of that class may legally stand (a cell is clear iff all cells within the class radius are clear). Dilated layers are recomputed incrementally in the cells around any stamp change. This turns "can a footman's disc fit through this gap" into a single bit test, and reproduces WC3's familiar behavior where large units cannot thread gaps small units can.

### 2.3 Buildings stamp the grid

Structures do not use collision discs; they **stamp a rectangular footprint** of cells `OccupiedStatic` when placement completes, and clear it on death/cancel — identical to WC3 pathing-map stamps. Placement validation tests the footprint against `Buildable` + occupancy; the builder-vacates-the-footprint dance is handled by issuing the builder a move-out order before stamping. Stamp/unstamp events mark the affected region dirty for the dilated layers and **invalidate cached paths** that pass through the region (paths store a bounding box for cheap intersection tests). All stamping happens in the tick's pathing phase ([phase 4](tick-and-scheduler.md)), in deterministic order.

## 3. A\* vs flow-field for 1,000 units

*Revised 2026-06-11 per D-2026-06-11-18: budgets provisioned at the 1,000-unit stretch target; 500 remains the low-tier guarantee.*

### 3.1 The analysis

| Criterion | A\* (per-unit, hierarchical-ready) | Flow field (per-destination) |
|---|---|---|
| Cost model | O(search) per *request*; requests are bursty (on order, on invalidation) | O(grid region) per *destination* per invalidation; amortizes over units sharing a goal |
| 1,000 units, scattered goals | Fine — most units are idle or following cached paths on any given tick | Poor — hundreds of distinct destinations means hundreds of fields |
| 1,000 units, one goal (attack-move blob) | Burst of 1,000 searches (mitigated by path sharing, §3.2 — but divergence re-paths scale with unit count) | Excellent — one field serves the whole blob, at any blob size |
| Memory | Open/closed scratch reused across queries; paths are short waypoint lists | One direction byte per cell per live field: 256 KB per field on 512×512 — caps hard against 1.5 GB RAM budget |
| Determinism | Easy: sequential, integer costs, explicit [tie-breaking](#4-deterministic-tie-breaking) | Easy: integration sweep in fixed cell order |
| Dynamic restamps | Invalidate + re-request affected paths | Re-integrate affected fields (coarser, costlier per event) |
| WC3 behavioral fidelity | High — WC3 is per-unit path + local resolution | Different "feel": crowd-flow behavior, units don't queue at chokes the WC3 way |

### 3.2 Decision

**Primary: A\*** on the dilated pathing grid, per collision class, with these standard amplifiers:

- **Hierarchical coarse stage:** a 16×16-cell sector graph (HPA\*-lite) answers long-distance reachability and produces a corridor; fine A\* runs only inside the corridor. Keeps worst-case single-query cost bounded on 512×512.
- **Path sharing for group orders:** a group ordered to one destination computes one representative path per collision class; members follow offset copies and re-path individually only when they diverge (formation logic, §5). This removes the "1,000 simultaneous searches" burst in the common case.
- **Reachability short-circuit:** sector-graph connected-component labels answer "unreachable" in O(1) before any search, then fall back to nearest-reachable-cell (WC3 behavior when targeting an unwalkable point).

**Flow fields move from contingency to planned-likely** (*revised 2026-06-11 per D-2026-06-11-18*). At the 500-unit scale the A\*+sharing path was expected to hold and flow fields sat in reserve. At 1,000 units the budget math doubles exactly where A\* is weakest — the shared-goal blob: even with path sharing, divergence re-paths scale with unit count, and a 1,000-unit attack-move across a freshly restamped region can queue more fine-stage work than the per-tick expansion budget (§6) clears promptly, stretching path latency past what the corridor-following mask hides. Accordingly:

- The flow-field backend stays behind the same `PathProvider` interface for the massed shared-goal scenario, but it is now **expected to be enabled, not held as contingency**: M3 benchmarks run at both 500 (low-tier guarantee) and 1,000 (recommended spec), and a 1,000-unit shared-goal scenario missing budget triggers implementation inside v1, not v1.x. Per the standing owner directive, the option is not deferred for difficulty — the seam, the memory (§7 reserves four field slots), and the determinism rules (§4 rule 5) are all in place from M3 so enabling it is a backend drop-in, not a re-plan.
- Per-unit A\* remains primary for everything else — scattered goals and WC3 behavioral fidelity (units queue at chokes; the flow-field backend serves only massed shared-goal moves, where blob behavior reads naturally anyway).

The requirement's "A\*/flow-field" wording is deliberately satisfied by an architecture where either backend can serve a request without gameplay-visible differences beyond paths taken (which are deterministic under each backend).

## 4. Deterministic tie-breaking

A\* is only deterministic if every choice with equal cost is broken identically everywhere. Mandated rules:

1. **Integer costs only.** Cardinal step = 10, diagonal = 14 (the classic ×10 octile approximation) in fixed-point-free integer math; heuristic is octile distance in the same units, scaled to remain admissible. No float `sqrt(2)` anywhere.
2. **Total ordering on the open list.** Priority key is the tuple `(f, h, insertionSeq)` compared lexicographically: equal `f` breaks toward smaller `h` (goal-ward bias, also a standard speedup), and remaining ties break by **insertion sequence number** — a monotone counter, not heap-internal order. The binary heap implementation must compare the full tuple; heap "stability" is never assumed.
3. **Fixed neighbor expansion order.** Neighbors are always generated in the same compass order (N, NE, E, SE, S, SW, W, NW). Corner-cutting through diagonally adjacent blocked cells is disallowed (both orthogonal neighbors must be clear), which also matches WC3 movement feel.
4. **Deterministic request order.** Path requests are queued and serviced in `(requestTick, requestSeq)` order in the pathing phase — never reordered by priority heuristics unless that priority is itself a deterministic function of sim state.
5. The same tuple-ordering discipline applies to flow-field integration (fixed sweep order) if that backend is enabled.

The open-list key, concretely:

```go
type pqKey struct {
    f, h  int32  // octile-integer costs
    seq   uint32 // insertion order, monotone per search
}

func less(a, b pqKey) bool {
    if a.f != b.f { return a.f < b.f }
    if a.h != b.h { return a.h < b.h }
    return a.seq < b.seq // total order — no two keys compare equal
}
```

The closed set and g-cost array are flat per-cell arrays indexed by cell, stamped with a per-search epoch counter instead of being cleared between searches (clearing 512×512 arrays per query would dominate the budget; epoch-stamping makes "reset" free and is itself deterministic).

These rules make the chosen path a pure function of grid state + request, so it needs no special treatment in the [state hash](determinism.md) beyond the grid and the stored paths themselves.

### 4.1 Movement layers

Three movement domains, as in WC3, each with its own view of the grid:

- **Ground** — full grid + dilated collision layers, the system described above.
- **Air** — flat cost field, blocked only by map bounds; air units path as straight lines with the same local-avoidance reservation rules in a separate air occupancy layer (flyers clump and shove among themselves but ignore ground occupancy).
- **Amphibious/special** (table-driven pathing masks per unit type, R-AST-1) — a unit's pathing mask selects which flag combination counts as walkable for *it*; the dilated layers are computed per (collision class × pathing mask) combination actually present in the loaded map's unit roster, not for the full cross product.

## 5. Formation, clumping, and local resolution

Grid A\* gives corridors; the WC3 "feel" comes from the local layer:

- **Dynamic occupancy reservation:** moving units reserve the cell(s) ahead of them (`OccupiedDynamic`); a unit whose next cell is reserved waits briefly, tries a local sidestep (deterministic order: prefer the side closer to the path), and only re-paths after a stall threshold (in ticks). This produces WC3's characteristic single-file queuing at chokepoints rather than RVO-style fluid crowds — a deliberate fidelity choice, and far cheaper than velocity-obstacle solvers.
- **Idle units yield:** an idle unit whose cell is requested by a mover receives a deterministic "shove" move order to an adjacent free cell (WC3's push-out-of-the-way behavior). Mutual-shove cycles are broken by entity index order.
- **Group movement:** group orders compute a loose formation — members are assigned offsets around the path (assignment by sorted entity index, deterministic) and individually steer to their offset point, collapsing to single-file in corridors when offsets are unwalkable. No persistent formation state survives the order; WC3-style "clump at destination" is accepted and units spread via the shove rule on arrival.
- All local decisions are integer/fixed-point and ordered by dense-store iteration order ([ECS §6](ecs-architecture.md)) — local avoidance is historically the #1 desync source in RTS engines, so every rule above specifies its ordering explicitly.

## 6. Budget per tick

Pathfinding shares the 10 ms tick budget (PRD §5.3); its slice is **≤ 2 ms worst case** on the reference machine, enforced as follows:

- **Expansion budget, not wall-clock:** the pathing phase services queued requests up to a per-tick budget of **N node expansions** (initial value 8,000 sized to the 500-unit low-tier guarantee; provisional 16,000 for the 1,000-unit stretch target on recommended spec — both tuned in M3, *revised 2026-06-11 per D-2026-06-11-18*) — counted work, not measured time, because a wall-clock cutoff would be non-deterministic ([Determinism §2.3](determinism.md)). Wall-clock is *measured* for the benchmark gate but never *consulted* by gameplay logic. Note the budget value is per map/sim version (it is sim semantics, below), so the low-tier and stretch configurations are distinct sim configurations, not a runtime adaptation.
- **Suspendable searches:** a search that exhausts the budget parks its open/closed state (preallocated per the pool rules in [ECS §2](ecs-architecture.md)) and resumes next tick. Units with pending paths play their "acknowledge" state and start moving along the coarse-stage corridor immediately, so latency is masked exactly as in WC3.
- **Per-request node cap:** any single search exceeding a hard node ceiling (e.g. 4× the sector-corridor estimate) terminates with best-partial-path toward the goal — bounding the pathological worst case (fully walled targets are already short-circuited by reachability labels).
- The M3 headless benchmark scenes (500 units on the low-tier reference and 1,000 units on recommended spec — D-2026-06-11-18; mass re-path on building stamp, cross-map attack-move, 1,000-unit shared-goal blob) gate this budget in CI alongside the [zero-alloc requirement](ecs-architecture.md): the pathing phase, like every phase, allocates nothing at steady state. The 1,000-unit shared-goal scene is also the §3.2 flow-field trigger gate.

Worked example of why counted budgets matter: suppose tick N has 12 queued requests whose searches total 11,000 expansions. Under the 8,000-expansion budget, the first requests complete, one search parks mid-flight, and the remainder wait — *identically on every machine*. Under a 2 ms wall-clock cutoff, a fast machine finishes all 12 and a slow one finishes 9, and the two sims have diverged. The budget number is therefore part of simulation semantics (and of the [replay/version contract](determinism.md)) — tuning it is a sim-version change, not a config knob.

### 6.1 Path request lifecycle

The full life of a request, tying the pieces together:

1. **Issue** — the Orders phase translates a move/attack-move order into a path request `(entity, goal, collisionClass, pathingMask, requestTick, requestSeq)` pushed onto the preallocated request queue.
2. **Short-circuit** — the pathing phase checks sector connected-component labels: unreachable goals resolve immediately to nearest-reachable-cell substitution.
3. **Coarse stage** — sector-graph search yields a corridor; the unit may begin moving along the corridor's first leg this tick.
4. **Fine stage** — corridor-constrained A\* runs under the expansion budget, possibly parked and resumed across ticks (§6).
5. **Result** — waypoints written into the unit's pooled path buffer; Movement consumes them with local avoidance (§5) layered on top.
6. **Invalidation** — a building stamp intersecting the path's bounding box, or a stall threshold being exceeded, re-enqueues from the unit's current position; the old buffer is recycled to the pool.

Every step is single-threaded inside the tick and ordered by `(requestTick, requestSeq)` — R-SIM-5's "no threads inside tick resolution" is satisfied not by locking but by there being nothing to lock.

## 7. Memory and preallocation

All pathing memory is fixed at map load per R-GC-2 ([ECS §2](ecs-architecture.md)):

| Structure | Size (128×128-cell map) |
|---|---|
| Base flag grid (512×512 × 1 B) | 256 KB |
| Cliff-level grid (512×512 × 1 B, D-2026-06-11-7) | 256 KB |
| Dilated layers (× ~4 class/mask combos) | ~1 MB |
| Sector graph (32×32 sectors, portals) | ~64 KB |
| g-cost + epoch arrays (per concurrent search slot × 2 slots) | ~2 MB |
| Open-list arena, path waypoint pool, request queue (sized for D-18 caps) | ~768 KB |
| Flow-field slots (4 × 256 KB direction fields, reserved per §3.2, D-2026-06-11-18) | 1 MB |

Roughly 5–6 MB total (*revised 2026-06-11 per D-2026-06-11-7/-18*) — still comfortably inside the sim's cache-resident ambitions and the 10 s map-load budget (PRD §5.3).

## 8. Tooling and acceptance hooks

- **Grid debug overlay** (render-side, reads grid snapshots): walkability, dilated layers per class, live reservations, last N paths — the indispensable tool for tuning the §5 feel rules. Lives entirely in `litd/render`/debug builds; the sim exposes a read-only grid view, honoring the PRD §4.1 boundary.
- **Determinism fixtures:** scripted scenarios (choke-point single-file, mutual shove, mass re-path on stamp, unreachable-target fallback) run headless on the full platform matrix and must produce identical hash traces — local avoidance is the historically desync-prone layer, so it gets dedicated fixtures rather than relying only on the general [10k-tick suite](determinism.md).
- **Behavioral fidelity checklist (M3 review):** large units can't thread small gaps; trees open paths when felled; buildings reject placement on occupied footprints; units queue at chokes instead of flowing around; idle units shove aside; ground units cross cliff levels only via ramps, never up cliff faces (*added 2026-06-11 per D-2026-06-11-7*). Each item is a headless assertion, not a manual eyeball.
