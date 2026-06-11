# Roadmap — Milestones M0–M6

> Expands [PRD §7 (Milestones)](../../PRD.md#7-milestones). Exit criteria here refine the
> PRD's one-line criteria into checkable items; nothing is added to or removed from the v1
> scope defined by the PRD. Cross-references: [Goals and Non-Goals](../01-vision/goals-and-non-goals.md)
> (criterion IDs G1.x–G5.x), [Risks and Open Questions](../01-vision/risks-and-open-questions.md)
> (R1–R6, Q1–Q4), and [Tooling](./tooling.md) (`jassgen`, asset validation, benchmark harness).

---

## 1. Dependency graph

```
M0 (bootstrap)
 ├──► M1 (determinism spike)  ── decides Q1 (sim math) ──► M3
 ├──► M2 (API manifest + spec) ─ decides Q2, Q4 ─────────► M5
 │                                │
 │                                └─ engine-feature census informs M4 scope (R4)
 ├──► M3 (sim core)  ◄── M1
 └──► M4 (render core) ◄── M3 (reads sim state; needs sim entities to render)
                              │   decides Q3 (terrain) at M4 design start
M5 (API v1) ◄── M2 (spec) + M3 (sim) + M4 (render)
M6 (vertical slice) ◄── M5
```

Notes on the graph:

- **M1 and M2 can run in parallel** after M0 — the determinism spike is sim-internal while
  manifest work is parser/spec work; they share no artifacts.
- **M2 must finish before M5 starts**, but its engine-feature census (R4) should land early
  enough to inform M4 planning — M2's manifest milestone is therefore scheduled to complete
  before M4's design phase begins.
- **M3 strictly follows M1**: no gameplay math is written until the fixed-point vs
  ordered-float decision (Q1) is recorded.
- **M4 strictly follows M3** for its data source (render reads sim state, never invents it),
  though render spikes (camera, GLB loading, animation playback) may begin against stub
  state earlier — they just cannot exit M4 without the real sim underneath.
- **M5 is the convergence point**: spec (M2) implemented over sim (M3) + render (M4).
- **M6 is integration only**: no new engine systems, only game content, polish, and budget
  closure.

---

## 2. M0 — Repo bootstrap

**PRD exit criteria.** Go module, vendored G3N, CI (lint/test/headless), asset packs
downloaded + validated.

**Deliverables.**
- Go module layout matching [PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation):
  `litd/api`, `litd/api/helpers`, `litd/sim`, `litd/render`, `litd/asset`, `tools/`,
  `data/`, `assets/`, with the architecture rule (sim never imports render) enforced by an
  import-graph CI check from day one.
- Vendored G3N fork at `repoes/engine` with a patch log file; vendored war3-types source
  material at `repoes/war3-types` (read-only input).
- CI pipeline: lint, `go test`, headless test job, license allowlist scan (G4.1),
  cross-platform build matrix (Windows/Linux/macOS × amd64/arm64) — the matrix M1 will reuse.
- Asset packs ingested ([PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasy-rts-packs-zero-cost):
  Quaternius Ultimate Fantasy RTS, KayKit Hexagon + Builder, Kenney kits) with provenance
  recorded in `assets/MANIFEST` (G4.2).
- First working version of the asset-validation CLI ([Tooling §3](./tooling.md)): core-glTF
  check and format rejection (R-FMT-1/2), run over every ingested asset.
- "Hello G3N" smoke binary: opens a window, loads one validated GLB, plays one animation —
  the R1 detection harness in embryonic form.

**Exit criteria (all must hold).**
1. `go build ./...` and `go test ./...` green on all three desktop OSes in CI.
2. License scan green with the BSD/MIT/Apache allowlist; zero unknown licenses.
3. 100% of ingested assets pass the validation CLI; assets using unsupported KHR extensions
   are either re-exported to core profile or excluded (count reported).
4. Import-graph check active and green (sim/render separation).
5. Smoke binary renders an animated CC0 model on at least one real low-tier machine.

**Risk hooks.** R1 detection begins (extension census, animation smoke test); R5 hygiene
scans active from the first commit.

---

## 3. M1 — Determinism spike

**PRD exit criteria.** Fixed-point vs ordered-float decision; 10k-tick sim state-hash
reproducibility test green.

**Deliverables.**
- Spike harness: a minimal toy sim (movement integration, accumulating combat-like math, a
  seeded PRNG) implemented twice — fixed-point (`int32` 16.16 and `int64` 32.32 variants)
  and ordered-float — with a canonical state-hash function.
- CI matrix job running 10k-tick hash comparisons across OS/arch (G5.1, G5.2) and across Go
  versions.
- Micro-benchmarks of both representations against the ≤ 10 ms tick budget context (Q1
  criterion 2).
- Decision record for Q1 (criteria, measurements, choice) committed under `docs/prd/01-vision/`
  per the [review cadence](../01-vision/risks-and-open-questions.md#3-review-cadence).
- The deterministic math package skeleton (`litd/sim/fixmath` or equivalent) embodying the
  decision — the boundary R3's late-discovery fallback depends on.
- Determinism lint ruleset v1: no `map` iteration in gameplay packages, no wall-clock reads,
  no unordered reductions (G5.4 enforcement starts here).

**Exit criteria.**
1. Chosen representation produces bit-identical hashes across the full CI matrix over ≥ 100
   runs of the 10k-tick scenario (G5.1/G5.2). Per R3's trigger: a single ordered-float
   divergence eliminates ordered-float.
2. Q1 decision record signed off; the recommended default
   ([fixed-point 32.32](../01-vision/risks-and-open-questions.md#q1--fixed-point-vs-ordered-float-for-sim-math))
   applies if evidence is inconclusive.
3. Hash-reproducibility test and determinism lints are permanent CI fixtures, not spike
   leftovers.

**Depends on:** M0 (CI matrix). **Blocks:** M3 (no gameplay math before Q1).

---

## 4. M2 — API manifest + spec

**PRD exit criteria.** `jassgen` outputs `api-manifest.json`; all 2,521 functions classified
D1–D5; public API spec doc signed off.

**Deliverables.**
- `tools/jassgen` v1 per [Tooling §2](./tooling.md): parses `common.j` (1,536 natives),
  `blizzard.j` (985 BJ functions), and the `.d.ts` files; emits `api-manifest.json` against
  the published schema; emits the audit report.
- Classification pass over all 2,521 functions: D1–D5 assignment, canonical Go mapping or
  tombstone, engine-feature tags (the R4 census), `commonai` entries tombstoned per the Q4
  decision.
- Public API specification document: the ~20 noun types, full exported signatures, options
  structs, event taxonomy replacing the trigger zoo (R-API-1…6), execution-model contract
  (R-EXEC-1…5), and the generated JASS→Go mapping table.
- Q2 evidence: sample port of a small published JASS map's logic using only the mapping
  table; findings recorded in the Q2 decision record.
- Generated API stubs (compiling, panicking bodies) for the canonical surface — the
  skeleton M5 fills in.

**Exit criteria.**
1. Audit report shows `unclassified == 0` over 2,521 functions (G1.1) and `unmapped == 0`
   — every entry mapped or tombstoned with an enumerated reason (G1.2, G1.4).
2. Manifest regeneration is reproducible in CI: re-running `jassgen` on the vendored sources
   produces a byte-identical manifest (or the build fails).
3. API spec passes lint-level checks: type ceiling (G2.1), parameter-count rule (G2.3),
   no free functions with handle params (G2.4), zero G3N types (G2.2).
4. Engine-feature rollup reviewed against the M4 plan; any overflow triggers M4 re-planning
   per [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks).
5. Q2 and Q4 decision records signed off.
6. API spec doc signed off by the owner.

**Depends on:** M0. **Blocks:** M5; informs M4 scope.

---

## 5. M3 — Sim core

**PRD exit criteria.** ECS, 20 Hz tick, movement, pathfinding, combat for 500 units within
budget (headless benchmark).

**Deliverables.**
- ECS with struct-of-arrays component stores, capacities fixed at map load (R-SIM-3,
  R-GC-2); entity lifecycle (create/destroy/recycle) with pooled transients.
- Fixed 20 Hz tick loop with the deterministic cooperative scheduler (R-EXEC-1): script
  coroutines, tick-quantized waits (R-EXEC-5), deterministic event dispatch order (R-EXEC-2).
- Movement + deterministic A*/flow-field pathfinding on the WC3-style grid, single-threaded
  within tick resolution (R-SIM-5).
- Combat: orders, attack acquisition, damage application, death events, projectile entities.
- Command-stream interface: ordered commands in → state out; replay recording and headless
  replay verification (R-SIM-4, G5.3).
- Headless benchmark scenario (500 units + 500 projectiles in sustained combat) wired into
  the CI benchmark harness ([Tooling §4](./tooling.md)).
- `testing.AllocsPerRun` gates on the tick path (R-GC-1/5).

**Exit criteria.**
1. Worst-case tick ≤ 10 ms on the reference CPU in the headless benchmark (G3.3), enforced
   in CI from this milestone onward.
2. Zero allocations per steady-state tick (G3.8); CI fails on regression.
3. Replay verification green: recorded session replays to an identical final hash (G5.3);
   cross-platform hash job green for the full sim (G5.2).
4. Determinism lints green over all of `litd/sim` (G5.4); scheduler order-assertion tests
   green (G5.5).
5. Sim builds and tests with no GPU/window dependency anywhere in its import graph.

**Depends on:** M1 (math decision), M0. **Blocks:** M4 (state source), M5.

---

## 6. M4 — Render core

**PRD exit criteria.** GLB units/buildings/terrain rendered with animation, team color, RTS
camera; 60 FPS on reference machine.

**Deliverables.**
- `litd/render` reading interpolated sim state (R-SIM-1) — read-only by construction.
- GLB unit/building rendering with skeletal animation driven by sim state and the
  contractual clip set (`Idle`, `Walk`, `Attack`, `Death`; R-AST-3).
- Terrain per the Q3 decision (decided at M4 design start; default: KayKit-style tiles),
  with static chunk merging.
- Locked RTS camera: ~34°-from-vertical perspective, fixed yaw, pitch/zoom clamps;
  orthographic mode behind a flag (R-RND-1).
- Team color via shader uniform (R-RND-7); lighting model: one directional + ambient,
  VFX point/spot cap ≤ 8 (R-RND-4); unlit low preset (R-RND-5); tuned near/far planes
  (R-RND-6).
- The known custom passes from [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks):
  fog of war, minimap, selection circles.
- Scripted render benchmark scene with draw-call and frame-time counters, wired into the CI
  harness; GPU-instancing spike on the vendored fork per R2's scheduled investigation
  trigger.

**Exit criteria.**
1. ≥ 60 FPS typical scene, ≥ 30 FPS at 500 units on the reference machine (G3.1/G3.2).
2. ≤ 300 draw calls at max army size (G3.9); if the 250-call early-warning threshold was
   crossed, the R2 instancing patch is landed and measured.
3. Zero steady-state allocations per render frame (G3.8 render side).
4. All rendered assets passed the validation CLI; animation clips play correctly for every
   shipped unit model (R1 closure for the v1 asset set).
5. Q3 decision record signed off at design start; terrain implementation matches it.
6. Render package never mutates sim state — enforced by API shape (read-only views) and
   import-graph check.

**Depends on:** M3, M0; scope informed by M2's feature census. **Blocks:** M5.

---

## 7. M5 — API v1

**PRD exit criteria.** Full canonical API implemented over sim+render; audit report shows
0 unmapped / 0 duplicated.

**Deliverables.**
- Every non-tombstoned canonical symbol from the manifest implemented over `litd/sim` +
  `litd/render`: game lifecycle, players, units, orders, events, timers, regions/rects,
  items/abilities/upgrades data access, UI (`g.UI()`, R-UI-1), audio (R-AUD-1), input
  bindings (R-INP-1) — the complete capability surface.
- `litd/api/helpers` with all D4 logic-bearing helpers, names disjoint from core (G1.5).
- WC3-semantics edge behavior: invalid handles return zero-value no-op objects with
  debug-mode asserts (R-API-5).
- Final generated JASS→Go mapping table published in docs (G2.6).
- API conformance test suite: each manifest entry links to at least one test exercising its
  canonical symbol.

**Exit criteria.**
1. Audit report: 0 unmapped, 0 duplicated, 0 unclassified; every tombstone human-reviewed
   (G1.1–G1.5).
2. API lints green: type ceiling, parameter rule, no handle-param free functions, zero G3N
   types in exported signatures (G2.1–G2.5).
3. All performance and allocation gates from M3/M4 remain green with the full API layered
   on top — the public layer adds no per-tick/per-frame allocations.
4. Conformance suite green headlessly for all sim-side capability (render-side natives
   exercised in the benchmark scene).
5. Spec deviations (M2 spec vs as-built) folded back into the spec doc and re-signed.

**Depends on:** M2 + M3 + M4. **Blocks:** M6.

---

## 8. M6 — Vertical slice

**PRD exit criteria.** Playable skirmish: build, train, fight, win/lose; all
[§5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
budgets green in CI.

**Deliverables.**
- A complete skirmish game built **purely against `litd/api`** (dogfooding gate: any needed
  capability missing from the public API is an M5 defect, not a private hook): worker
  economy, building construction, unit training, combat, win/lose conditions.
- Scripted opponent written on the public API (per the
  [Q4 deferral](../01-vision/risks-and-open-questions.md#q4--commonaidts-ai-natives-port-in-v1-or-defer)
  — no `commonai` domain).
- Full WC3-grade input in play: drag-select, control groups 0–9, smart orders, hotkeys,
  edge-pan + middle-drag camera (R-INP-1); HUD on G3N widgets via `g.UI()` (R-UI-1).
- Game data as JSON/TOML tables in `data/` (R-AST-1); all models validator-clean (R-AST-2/3).
- End-to-end benchmark run in CI covering every §5.3 budget, including cold start, map
  load, RAM ceiling, and artifact size.
- A full-match replay recorded and hash-verified headlessly — the determinism thesis
  demonstrated on real gameplay, and the lockstep-readiness evidence for NG4.

**Exit criteria.**
1. A tester on the reference machine can play a skirmish from cold start to victory/defeat.
2. All §5.3 budgets green in CI (G3.1–G3.9): ≥ 60/≥ 30 FPS, ≤ 10 ms tick, ≤ 5 s cold start,
   ≤ 10 s map load (128×128), ≤ 1.5 GB RAM, ≤ 300 MB binary + base assets, zero hot-path
   allocations, ≤ 300 draw calls.
3. Full-match replay verification green on all three OSes.
4. Zero usages of non-public engine internals in the slice's game code.
5. Milestone-close review of [risks and open questions](../01-vision/risks-and-open-questions.md)
   completed; v2 candidate list (netcode, Lua VM, `commonai`, heightmap terrain) recorded.

**Depends on:** M5. **Blocks:** v1 release.

---

## 9. Cross-milestone gates summary

| Gate | Introduced | Enforced through |
|---|---|---|
| License/provenance scans (G4.x) | M0 | every milestone |
| Import-graph separation (sim ↛ render) | M0 | every milestone |
| Determinism hash matrix (G5.1/G5.2) | M1 | every milestone |
| Manifest regeneration reproducibility | M2 | every milestone |
| Tick budget + zero-alloc tick (G3.3/G3.8) | M3 | every milestone |
| FPS, draw-call, zero-alloc frame (G3.1/G3.2/G3.9) | M4 | every milestone |
| Audit 0-unmapped/0-duplicated (G1.x) | M5 | every milestone |
| Full §5.3 budget suite | M6 | release gate |

Tooling that powers these gates — `jassgen`, the asset-validation CLI, and the CI benchmark
harness — is specified in [Tooling](./tooling.md).
