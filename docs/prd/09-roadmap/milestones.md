# Roadmap — Milestones M0–M9

> Expands [PRD §7 (Milestones)](../../PRD.md#7-milestones). Exit criteria here refine the
> PRD's one-line criteria into checkable items; nothing is added to or removed from the v1
> scope defined by the PRD. Cross-references: [Goals and Non-Goals](../01-vision/goals-and-non-goals.md)
> (criterion IDs G1.x–G5.x), [Risks and Open Questions](../01-vision/risks-and-open-questions.md)
> (R1–R9, Q1–Q5), and [Tooling](./tooling.md) (`jassgen`, asset validation, `assetgen`,
> benchmark harness).
>
> *Revised 2026-06-11 per the owner decision record
> ([decisions.md](../01-vision/decisions.md), D-2026-06-11-1…20): M0.5 marked DONE; M5
> gains Lua; M5.5, M7, M8, M9 added; M4/M6 scope expanded. Per-milestone revision notes
> inline. Standing directive: features are not cut or deferred because they are hard.*
>
> *Revised 2026-06-11 per the third-session record (D-2026-06-11-21…30): all spikes executed
> and all decisions made upfront. M1 retitled **Determinism foundation** — its spike is DONE
> (D-27/28, code in `spikes/`); M6 is the **flagship game v0.1** (D-24) and its
> license/distribution decision rows are removed (decided: proprietary forever D-21, own-site
> distribution D-22); M7 transport decided (quic-go star topology, D-26); M9 promoted from
> candidate to **committed** (D-23).*

---

## 1. Dependency graph

```
M0 (bootstrap)
 ├──► M0.5 (First Light demo) ── DONE 2026-06-11
 ├──► M1 (determinism foundation) ── spike DONE (D-27/28: fixed-point 32.32 + stackless
 │                               scheduler validated in spikes/); productionization ──► M3
 ├──► M2 (API manifest + spec) ─ Q2 decided (D-2); commonai mapped canonically (D-6) ──► M5
 │                                │
 │                                └─ engine-feature census informs M4 scope (R4)
 ├──► M3 (sim core)  ◄── M1
 └──► M4 (render core) ◄── M3 (reads sim state; needs sim entities to render)
                              │   terrain = heightmap + cliffs (D-7); instancing planned (D-18)
M5 (API v1 + Lua) ◄── M2 (spec) + M3 (sim) + M4 (render)
M5.5 (AI domain) ◄── M5 (AI natives bind against the finished canonical API)
M6 (vertical slice) ◄── M5.5 — G5.3 replay gate here is the lockstep-readiness proof
M7 (multiplayer: quic-go star, D-26 + replay viewer/observers) ◄── M6
M8 (World Editor + campaign UI) ◄── M7
M9 (world hub — committed, D-23; co-hosts the M7 relay) ◄── M8, hard-gated on the Lua sandbox (D-20)
```

Notes on the graph:

- **M0.5 is done** (shipped 2026-06-11, `cmd/firstlight`): a deliberately early playable
  proof, FSV-verified; its code is throwaway-tolerant and constrains nothing downstream.
- **M1 and M2 can run in parallel** after M0 — M1 is sim-internal productionization (its
  spike already ran, D-27/28) while manifest work is parser/spec work; they share no
  artifacts.
- **M2 must finish before M5 starts**, but its engine-feature census (R4) should land early
  enough to inform M4 planning — M2's manifest milestone is therefore scheduled to complete
  before M4's design phase begins.
- **M3 strictly follows M1**: both former blockers are decided and spike-validated — the
  fixed-point validation (D-27) and the stackless serializable scheduler (D-28) — so M3 now
  waits only on their productionized forms (`litd/fixed` + scheduler) landing in M1.
- **M4 strictly follows M3** for its data source (render reads sim state, never invents it),
  though render spikes (camera, GLB loading, animation playback) may begin against stub
  state earlier — they just cannot exit M4 without the real sim underneath.
- **M5 is the convergence point**: spec (M2) implemented over sim (M3) + render (M4), plus
  the Lua surface generated from the same manifest (D-8).
- **M5.5 follows M5** because the AI domain's natives bind against the finished canonical
  API and its scheduler reuses the (serializable) M3 scheduler machinery in a second
  isolated instance.
- **M6 is integration plus the persistence/packaging features** that need everything else
  in place: mid-game save/load (D-9) and the world archive format (D-14) — no new sim/render
  systems. The former D-19 distribution/license decision item is gone: decided upfront
  (proprietary forever, D-21; own-site distribution, D-22). M6 ships **the flagship game
  v0.1** (D-24), not a tech demo.
- **M7–M9 are strictly sequential**: M7's lockstep needs M6's replay-verification proof;
  M8's editor saves into M6's archive format and follows multiplayer per the decision
  record; M9 (committed, D-23) hosts archives, co-hosts the M7 session relay (D-26), and is
  hard-gated on the D-20 sandbox.

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

## 3. M0.5 — "First Light" demo — **DONE 2026-06-11**

*(Added 2026-06-11; shipped the same day — see PRD §7.)*

**PRD exit criteria.** Earliest playable proof, built before the full architecture: window
opens, terrain plane renders, one animated GLB unit on screen, right-click moves it
(straight-line, no pathfinding), drag-select highlights it. `Game.Screenshot()` works.
Verified per the FSV protocol (PRD §5.5).

**Status: DONE.** Shipped as `cmd/firstlight` (commit `913b7c5`). The `-autotest` mode
orders the unit to a known target, prints the state JSON, saves a screenshot, and exits
0/2/3 (pass/timeout/wrong position) — FSV evidence captured: screenshot of the unit at its
commanded position plus a state dump confirming sim coordinates match.

**Scope guard.** The demo's code is throwaway-tolerant: it seeds M3/M4 but must not
constrain them. Nothing in M3/M4 may inherit a First Light design by inertia.

**Depended on:** M0. **Blocks:** nothing (proof milestone, off the critical path).

---

## 4. M1 — Determinism foundation

**PRD exit criteria.** Spike already executed 2026-06-11 (D-27/D-28, `spikes/`): fixed-point
int64 32.32 validated (182 µs/2,000-entity tick = 1.8% of budget); stackless serializable
scheduler validated (mid-run save/restore bit-identical). Remaining M1 work: productionize
`litd/fixed` + scheduler, wire the 10k-tick hash test across the OS/arch CI matrix.

*Revised 2026-06-11 per D-2026-06-11-27/28 (supersedes the earlier D-1/D-9 spike framing):
the spike is **DONE** — executed the same day as the decisions, code in `spikes/`.
`spikes/fixedpoint` ran representative tick math (movement integration, distance/sqrt,
damage accumulation) for 2,000 entities at **182 µs/tick = 1.8% of the ≤ 10 ms budget**
(float64 baseline 28 µs — the 6.5× ratio is irrelevant at this absolute cost), with the
10k-tick state hash bit-stable across repeated runs, 4-hour timers and DPS accumulators
exact, 5 decimal orders of coordinate-range headroom, and zero allocs/tick (D-27).
`spikes/scheduler` gob-serialized the full scheduler state **mid-run**, restored it, and
advanced — traces and state bit-identical with the uninterrupted run, resume order
deterministic by `(wakeTick, seq)` (D-28). M1 is therefore no longer a decision milestone:
it productionizes validated designs.*

**Deliverables.**
- `litd/fixed`: the production form of the `spikes/fixedpoint` math — 32.32 type, trig
  tables, `SqrtU64`, 128-bit range tests per [Determinism §2.4](../04-simulation/determinism.md);
  the boundary R3's late-discovery fallback depends on, frozen early.
- The production stackless scheduler per [Tick & Scheduler §3](../04-simulation/tick-and-scheduler.md):
  descriptive suspension records, `(wakeTick, seq)` sleeper queue; the `spikes/scheduler`
  save → restore → resume round-trip ported into a permanent CI fixture.
- CI matrix job running 10k-tick hash comparisons across OS/arch (G5.1, G5.2) and across Go
  versions — the spike harness promoted, not rebuilt.
- Determinism lint ruleset v1: no `map` iteration in gameplay packages, no wall-clock reads,
  no unordered reductions (G5.4 enforcement starts here).

**Exit criteria.**
1. `litd/fixed` produces bit-identical hashes across the full CI matrix over ≥ 100 runs of
   the 10k-tick scenario (G5.1/G5.2) — expected trivially green per the D-27 spike evidence;
   the matrix run is the productionization proof, not a decision input.
2. Scheduler serialization round-trip green in CI on the production implementation,
   reproducing the `spikes/scheduler` bit-identical mid-run save/restore result (D-28). M3
   does not begin on an unserializable scheduler.
3. Hash-reproducibility test and determinism lints are permanent CI fixtures, not spike
   leftovers.

**Depends on:** M0 (CI matrix). **Blocks:** M3 (gameplay math builds on the productionized
`litd/fixed` and scheduler).

---

## 5. M2 — API manifest + spec

**PRD exit criteria.** `jassgen` outputs `api-manifest.json`; all 2,521 functions classified
D1–D5; public API spec doc signed off.

**Deliverables.**
- `tools/jassgen` v1 per [Tooling §2](./tooling.md): parses `common.j` (1,536 natives),
  `blizzard.j` (985 BJ functions), and the `.d.ts` files; emits `api-manifest.json` against
  the published schema; emits the audit report.
- Classification pass over all 2,521 functions: D1–D5 assignment, canonical Go mapping or
  tombstone, engine-feature tags (the R4 census). *Revised 2026-06-11 per D-2026-06-11-6:*
  all ~123 `common.ai` natives (plus the AI-related `common.j` natives) are **mapped
  canonically** into the AI-domain surface implemented at M5.5 — no `deferred-v2` tombstones
  for capability reasons.
- Public API specification document: the ~20 noun types, full exported signatures, options
  structs, event taxonomy replacing the trigger zoo (R-API-1…6), execution-model contract
  (R-EXEC-1…5), and the generated JASS→Go mapping table.
- Q2 evidence: sample port of a small published JASS map's logic using only the mapping
  table; findings recorded in the Q2 decision record.
- Generated API stubs (compiling, panicking bodies) for the canonical surface — the
  skeleton M5 fills in.

**Exit criteria.**
1. Audit report shows `unclassified == 0` over 2,521 functions (G1.1) and `unmapped == 0`
   — every entry mapped or tombstoned with an enumerated reason (G1.2, G1.4) — **plus**
   all `commonai` natives classified and canonically mapped with zero capability tombstones
   (D-2026-06-11-6).
2. Manifest regeneration is reproducible in CI: re-running `jassgen` on the vendored sources
   produces a byte-identical manifest (or the build fails).
3. API spec passes lint-level checks: type ceiling (G2.1), parameter-count rule (G2.3),
   no free functions with handle params (G2.4), zero G3N types (G2.2).
4. Engine-feature rollup reviewed against the M4 plan; any overflow triggers M4 re-planning
   per [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks).
5. Q2 and Q4 decision records signed off. *(Both decided 2026-06-11: D-2 idiomatic-only
   naming, D-6 full `commonai` port. M2 still validates D-2's mapping-table approach via
   the sample port; a painful port reopens Q2 with evidence.)*
6. API spec doc signed off by the owner — including the AI-domain surface shape (M5.5)
   and the Lua binding conventions (D-8), since both generate from this manifest.

**Depends on:** M0. **Blocks:** M5; informs M4 scope.

---

## 6. M3 — Sim core

**PRD exit criteria.** ECS, 20 Hz tick, movement, pathfinding, combat for 500 units within
budget (headless benchmark).

*Revised 2026-06-11 per D-2026-06-11-9/15/18: capacities provision the 1,000-unit stretch
target; the scheduler implementation is serializable; campaign-persistence hooks land here.*

**Deliverables.**
- ECS with struct-of-arrays component stores, capacities fixed at map load (R-SIM-3,
  R-GC-2); entity lifecycle (create/destroy/recycle) with pooled transients. Capacities,
  pathfinding structures, and budgets provisioned for **1,000 units + 1,000 projectiles**
  (D-18) — the 500-unit low-tier budget remains the gate, 1,000 the recommended-spec target.
- Fixed 20 Hz tick loop with the deterministic cooperative scheduler (R-EXEC-1): script
  coroutines, tick-quantized waits (R-EXEC-5), deterministic event dispatch order (R-EXEC-2)
  — implemented on the **serializable representation chosen at M1** (D-9): suspended
  coroutines, timers, and event subscriptions all serialize into the save format.
- Campaign-persistence hooks (D-15): cross-map persistent state (game-cache semantics, hero
  carry-over) built into the sim state model and save format now — retrofit is brutal,
  build-in is cheap. Campaign UI itself is M8.
- Movement + deterministic A*/flow-field pathfinding on the WC3-style grid, single-threaded
  within tick resolution (R-SIM-5).
- Combat: orders, attack acquisition, damage application, death events, projectile entities.
- Command-stream interface: ordered commands in → state out; replay recording and headless
  replay verification (R-SIM-4, G5.3).
- Headless benchmark scenario (500 units + 500 projectiles in sustained combat) wired into
  the CI benchmark harness ([Tooling §4](./tooling.md)); a 1,000-unit + 1,000-projectile
  stretch scenario runs alongside it and is tracked (D-18).
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
6. *(Added 2026-06-11 per D-2026-06-11-9.)* Scheduler serialization round-trip green on the
   real sim: save mid-scenario → load → run to completion reproduces the unbroken run's
   final hash — permanent CI fixture from here (full save/load UX ships at M6).
7. *(Added 2026-06-11 per D-2026-06-11-18.)* The 1,000-unit stretch scenario runs in the
   harness with results tracked; capacities hold without reallocation. The ≤ 10 ms gate
   applies to the 500-unit scenario on the low-tier reference CPU.

**Depends on:** M1 (math decision), M0. **Blocks:** M4 (state source), M5.

---

## 7. M4 — Render core

**PRD exit criteria.** GLB units/buildings/terrain rendered with animation, team color, RTS
camera; 60 FPS on reference machine.

*Revised 2026-06-11 per D-2026-06-11-7/17/18: terrain is heightmap + cliffs (supersedes the
Q3 tile-mesh default); the instancing patch is planned-in, not contingency; locale string
tables start here.*

**Deliverables.**
- `litd/render` reading interpolated sim state (R-SIM-1) — read-only by construction.
- GLB unit/building rendering with skeletal animation driven by sim state and the
  contractual clip set (`Idle`, `Walk`, `Attack`, `Death`; R-AST-3).
- WC3-fidelity terrain per D-2026-06-11-7: heightmap mesh, discrete cliff levels with
  ramps, texture splatting, chunked rendering aligned to the pathing grid. The sim-side
  grid abstraction is unchanged (R-SIM-5: the sim sees walkability and cliff levels, never
  the mesh). Splat and cliff texture sets come from `tools/assetgen`
  ([Tooling §5](./tooling.md), D-12).
- Locked RTS camera: ~34°-from-vertical perspective, fixed yaw, pitch/zoom clamps;
  orthographic mode behind a flag (R-RND-1).
- Team color via shader uniform (R-RND-7); lighting model: one directional + ambient,
  VFX point/spot cap ≤ 8 (R-RND-4); unlit low preset (R-RND-5); tuned near/far planes
  (R-RND-6).
- The known custom passes from [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks):
  fog of war, minimap, selection circles.
- **GPU instancing patch on the vendored fork, planned-in** (D-18 supersedes R2's
  conditional trigger: the 1,000-unit stretch target assumes it lands here, not as
  contingency). The M3/M4-boundary spike feeds straight into implementation.
- Locale string tables from M4 onward (D-17): every user-facing string — engine UI and
  world-author strings — flows through locale tables; v1 ships English, translations are
  pure data drops.
- Scripted render benchmark scene with draw-call and frame-time counters, wired into the CI
  harness, including the 1,000-unit stretch scene (D-18).

**Exit criteria.**
1. ≥ 60 FPS typical scene, ≥ 30 FPS at 500 units on the reference machine (G3.1/G3.2).
2. ≤ 300 draw calls at max army size (G3.9); the instancing patch is landed and measured
   *(revised 2026-06-11 per D-2026-06-11-18 — no longer conditional on the 250-call
   early-warning threshold; the threshold remains as verification)*, and the 1,000-unit
   stretch scene is measured on the recommended spec (G3.10).
3. Zero steady-state allocations per render frame (G3.8 render side).
4. All rendered assets passed the validation CLI; animation clips play correctly for every
   shipped unit model (R1 closure for the v1 asset set); generated terrain textures carry
   assetgen provenance (G4.7).
5. Terrain implements D-2026-06-11-7 (heightmap + cliffs + ramps + splatting) with the sim
   blind to the mesh (R-SIM-5) *(revised 2026-06-11; was the Q3 sign-off criterion)*.
6. Render package never mutates sim state — enforced by API shape (read-only views) and
   import-graph check.
7. *(Added 2026-06-11 per D-2026-06-11-17.)* Zero hard-coded user-facing strings: lint over
   `litd/render`/UI packages confirms all strings resolve through the locale tables.

**Depends on:** M3, M0; scope informed by M2's feature census. **Blocks:** M5.

---

## 8. M5 — API v1 + Lua

**PRD exit criteria.** Full canonical API implemented over sim+render; audit report shows
0 unmapped / 0 duplicated; Lua VM embedded (deterministic, hard-sandboxed), bindings
generated from `api-manifest.json`; worlds runtime-loadable.

*Revised 2026-06-11 per D-2026-06-11-8/13/20: the milestone gains the Lua creation surface
and the hard sandbox; doodad handle promotion lands with the API.*

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
- Embedded deterministic Lua VM (gopher-lua family, determinism-audited per
  [R7](../01-vision/risks-and-open-questions.md#r7--lua-vm-determinism-added-2026-06-11-per-d-2026-06-11-8));
  Lua bindings **generated from `api-manifest.json`** by `jassgen`
  ([Tooling §2](./tooling.md)) so the Go and Lua surfaces cannot drift (D-8).
- Hard sandbox (R-SEC-1, D-20): world Lua sees no io/os/net — game API only — with per-tick
  instruction and memory quotas (the quotas double as the M7 lockstep stall guard).
  Disk-loaded worlds get the same sandbox from M5; any sharing feature is gated on it.
- Worlds runtime-loadable: Lua + data loaded without a Go toolchain or recompile (the world
  archive *format* is defined at M6; M5 loads from directories).
- Doodad handles with promotion-on-first-touch (D-13): render-only storage by default;
  a doodad first addressed by script (show/hide, animate, reposition —
  `SetDoodadAnimation` analogues map canonically) is promoted to a handle, so the zero-cost
  case stays zero-cost.

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
6. *(Added 2026-06-11 per D-2026-06-11-8.)* Lua parity: the conformance suite (or its
   sim-side representative subset) runs from Lua and is green; binding regeneration in CI
   is byte-identical (G2.7); a Lua-scripted scenario passes the determinism hash matrix
   (G5.7).
7. *(Added 2026-06-11 per D-2026-06-11-20.)* Sandbox verified adversarially: escape-attempt
   tests (io/os/net reach, quota busting, runaway loops) fail loudly; per-tick instruction
   and memory quotas are enforced and deterministic.

**Depends on:** M2 + M3 + M4. **Blocks:** M5.5, M6.

---

## 9. M5.5 — AI domain *(added 2026-06-11 per D-2026-06-11-6)*

**PRD exit criteria.** Full `commonai` port: second sandboxed scheduler domain, isolated
contexts, command-stack messaging (R-EXEC-3); all AI natives canonical.

This milestone exists because D-6 reversed the Q4 deferral: the JASS AI domain is v1 scope,
scheduled after the core API (M5) and before the vertical slice — M6's melee opponent runs
on the real AI domain, not a Go stopgap.

**Deliverables.**
- Second scheduler domain: an isolated instance of the (serializable, D-9) cooperative
  scheduler hosting AI scripts in their own contexts — no shared globals with map scripts.
- Command-stack messaging boundary per R-EXEC-3: the only channel between the AI domain and
  the game; deterministic ordering across the boundary.
- All ~123 `common.ai` natives plus the AI-related `common.j` natives implemented against
  their canonical mappings from the M2 manifest — zero capability tombstones.
- `StartMeleeAI` analogues wired end to end: melee mode structurally complete with the real
  AI domain behind it.
- A reference melee AI script exercising the domain — the embryo of M6's opponent.

**Exit criteria.**
1. Audit report: every `commonai`-origin entry implemented (`unimplemented == 0` includes
   the AI natives); zero capability tombstones (D-6).
2. Isolation enforced by test: an AI context cannot read or write map-script state;
   communication observed only via command stacks (R-EXEC-3).
3. Determinism holds with the AI domain live: an AI-driven scripted scenario passes the
   cross-platform hash matrix, and AI-domain state serializes into the save round-trip
   (D-9 — AI state is part of mid-game saves).
4. The reference melee AI plays a headless skirmish to a win/lose outcome.

**Depends on:** M5. **Blocks:** M6.

---

## 10. M6 — Vertical slice = **LitD game v0.1**

**PRD exit criteria.** Playable skirmish vs the real AI domain: build, train, fight,
win/lose; **the flagship game, not a tech demo (D-24)** — art style/factions/lore
established; mid-game save/load shipping; world archive format defined; all
[§5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
budgets green in CI; replay verification (G5.3) green — lockstep-readiness proof for M7.

*Revised 2026-06-11 per D-2026-06-11-6/9/14: the opponent runs on the M5.5 AI domain;
save/load and the world archive format land here. Revised again 2026-06-11 per
D-2026-06-11-21/22/24: the former distribution + license decision item (D-19) is **removed**
— decided upfront (engine proprietary forever, D-21; own-site distribution only, D-22) —
and the slice is reframed as **v0.1 of the actual Light in the Dark game** (D-24): art
style, factions, and lore are established now (generative pipeline, D-12) and every
subsequent milestone ships a better version of the same game.*

**Deliverables.**
- A complete skirmish game built **purely against `litd/api`** (dogfooding gate: any needed
  capability missing from the public API is an M5 defect, not a private hook): worker
  economy, building construction, unit training, combat, win/lose conditions.
- Skirmish opponent running on the **real AI domain (M5.5)** *(revised 2026-06-11 per
  D-2026-06-11-6; supersedes the Go-scripted stopgap planned under the Q4 deferral)*.
- **Mid-game save/load shipping (D-9):** save at any tick — suspended coroutines, timers,
  event subscriptions, AI-domain state, campaign-persistence state all included — load and
  resume; built on the M3 serializable scheduler. Replays (command streams) remain the
  separate, complementary mechanism.
- **World archive format defined and exercised (D-14):** single-file zip — map data + Lua +
  custom assets + manifest with content hashes and engine-version requirements — loadable
  from disk, publicly documented, carrying the hosting metadata the committed M9 hub (D-23)
  needs from day one. The slice's map ships as an archive.
- **Flagship-game identity established (D-24):** art style, factions, and lore fixed and
  shipped in the slice; the game grows every milestone from here — engine and game prove
  each other. *(Replaces the former D-19 distribution/license decision deliverable: decided
  upfront per D-21/D-22, no decision work remains in M6.)*
- Full WC3-grade input in play: drag-select, control groups 0–9, smart orders, hotkeys,
  edge-pan + middle-drag camera (R-INP-1); HUD on G3N widgets via `g.UI()` (R-UI-1).
- Game data as JSON/TOML tables in `data/` (R-AST-1); all models validator-clean (R-AST-2/3).
- End-to-end benchmark run in CI covering every §5.3 budget, including cold start, map
  load, RAM ceiling, and artifact size.
- A full-match replay recorded and hash-verified headlessly (CI artifact, D-16) — the
  determinism thesis demonstrated on real gameplay, and the **hard lockstep-readiness gate
  for M7** (per D-5: M7 must not discover determinism debt).

**Exit criteria.**
1. A tester on the reference machine can play a skirmish from cold start to victory/defeat
   against the M5.5 AI domain.
2. All §5.3 budgets green in CI (G3.1–G3.9): ≥ 60/≥ 30 FPS, ≤ 10 ms tick, ≤ 5 s cold start,
   ≤ 10 s map load (128×128), ≤ 1.5 GB RAM, ≤ 300 MB binary + base assets, zero hot-path
   allocations, ≤ 300 draw calls; 1,000-unit stretch results recorded on the recommended
   spec (G3.10).
3. Full-match replay verification green on all three OSes — this is the hard gate M7 opens
   on (G5.3, D-5).
4. Zero usages of non-public engine internals in the slice's game code.
5. *(Added 2026-06-11 per D-2026-06-11-9.)* Mid-game save/load verified per FSV: save at an
   arbitrary tick, load, run to completion — final hash equals the unbroken run's; works
   across all three OSes.
6. *(Added 2026-06-11 per D-2026-06-11-14.)* The slice's map loads from a world archive;
   the archive format spec is published; content hashes and engine-version requirements
   validate on load.
7. Milestone-close review of [risks and open questions](../01-vision/risks-and-open-questions.md)
   completed. *(Revised 2026-06-11: the former "v2 candidate list" — netcode, Lua VM,
   `commonai`, heightmap terrain — is gone; all four are committed v1/roadmap scope per
   D-5/6/7/8.)*

**Depends on:** M5.5. **Blocks:** v1 release, M7.

---

## 11. M7 — Multiplayer (lockstep) + replay viewer *(added 2026-06-11 per D-2026-06-11-5/16)*

**PRD exit criteria.** 2–8 player LAN/online skirmish on **quic-go, star topology (D-26)**:
lockstep scheduler (command turns, adaptive input delay, stall handling), lobby/session
bootstrap, state-hash desync detection with diagnostic dump. Replays and netplay share the
command-stream format. In-client replay viewer + live observer slots.

*Revised 2026-06-11 per D-2026-06-11-26: the transport decision is no longer an M7-start
spike — decided upfront from the executed research memo.*

**Deliverables.**
- Transport (decided, D-26): **quic-go** (MIT, mature, RFC 9221 datagrams) over a **star
  topology** — LAN: a player's engine hosts in-process; internet: the same host loop runs on
  a **lightweight relay co-located with the M9 hub** (D-23), eliminating NAT traversal
  entirely (hole-punching QUIC is still IETF-draft in 2026; pion/webrtc is the recorded
  runner-up if relay economics ever fail). Build hash + seed exchanged at join; mismatch
  refuses the session.
- Lockstep scheduler over the existing command stream: **command turns every 2–4 sim
  ticks**, **adaptive input-delay buffer** (start 2 turns), reliable stream for turns +
  hashes, stall = pause + grace-period drop (D-26) — the Lua sandbox's per-tick quotas
  (D-20) double as the stall guard. Each client runs the full deterministic sim; only
  commands are exchanged, never state.
- Lobby/session bootstrap for 2–8 players.
- Desync detection: clients exchange the 64-bit `Game.StateHash()` (R-FSV-2) piggybacked
  ~1/s on the reliable stream (D-26); divergence detected immediately and bisected
  per-system via the sub-hash design, with a diagnostic dump.
- One format for replays and netplay: a replay *is* the recorded command stream of a
  session (D-5).
- **In-client replay viewer (D-16):** pause, speed control, free camera, per-player
  perspective — over the same command-stream machinery.
- **Live observer slots (D-16):** observers are replay viewers at zero delay; no separate
  observer pipeline.

**Exit criteria.**
1. A 2–8 player skirmish completes over LAN and over the open internet on the three desktop
   OSes, cross-OS in one session.
2. An injected fault (forced divergent tick on one client) is detected within N ticks,
   bisected to the offending system, and produces the diagnostic dump.
3. The replay viewer plays back a full M6 match with pause/speed/free-camera/per-player
   perspective; an observer joins a live match and sees it at zero delay.
4. No per-client state or local-player branches inside the sim tick (the D-5 "survives
   lockstep" rule), verified by lint and review; netcode lives outside `litd/sim`.
5. Stall handling verified: a deliberately slowed client triggers the input-delay/stall
   path, not a desync.

**Depends on:** M6 (replay-verification gate). **Blocks:** M8.

---

## 12. M8 — World Editor + campaign UI *(added 2026-06-11 per D-2026-06-11-10/15)*

**PRD exit criteria.** In-engine visual editor: terrain sculpt/paint, unit/doodad
placement, map metadata, world-archive save; campaign menu/mission-flow UI.

**Deliverables.**
- In-engine visual editor built on the public API and the world archive format: heightmap
  terrain sculpting and texture painting (the D-7 terrain), unit/doodad placement (doodad
  handles per D-13), map metadata editing, save to world archive (D-14).
- Campaign menu and mission-flow UI (D-15) over the persistence architecture built since
  M3: mission unlock flow, hero carry-over, game-cache-semantics state across maps.
- Scope guard: trigger-GUI authoring is explicitly **not** in M8 — Lua (M5) covers logic
  until a later milestone; this is recorded scope, not silent deferral.
- Editor strings flow through the locale tables (D-17) like all other UI.

**Exit criteria.**
1. A complete playable map is authored start-to-finish in the editor — terrain sculpted and
   painted, units and doodads placed, metadata set — saved as a world archive, then loaded
   and played in the engine (FSV: play the authored map, verify state + screenshot).
2. A campaign of ≥ 2 missions with hero carry-over is assembled and played through the
   campaign UI; persistent state survives across maps and through mid-game save/load.
3. The editor uses only the public API and the archive format — no private engine hooks
   (same dogfooding gate as M6).
4. NG5 holds: the editor is RTS-shaped authoring tooling; no general-purpose-engine
   features rode in with it.

**Depends on:** M7. **Blocks:** M9 (committed, D-23).

---

## 13. M9 — World hub (committed) *(added 2026-06-11 per D-2026-06-11-14, hard-gated per D-2026-06-11-20; committed per D-2026-06-11-23)*

**PRD exit criteria.** Hosted world repository + in-game browser over the v1 archive
format: static-friendly index (no account needed to download), accounts/ratings later;
co-hosts the M7 session relay; hard-gated on the Lua sandbox.

*Revised 2026-06-11 per D-2026-06-11-23: promoted from "candidate" to **committed, M9
firm**. Detailed scope is still set at M8 close, but the milestone itself is no longer
conditional.* What is fixed now:

- The v1 world archive format (M6) carries hosting metadata from day one, so M9 needs no
  format break.
- **Architecture (D-23):** static-friendly index — no account needed to download;
  accounts/ratings are a later layer on top.
- **Relay co-hosting (D-23/D-26):** the hub backend co-hosts the lightweight session relay
  that M7 internet play runs through — one operated service, two duties.
- **Hard gate (D-20, non-negotiable):** no sharing feature ships unless world Lua runs in
  the no-io/no-os/no-net VM with per-tick instruction and memory quotas — worlds cannot
  touch the player's machine. The M9 hub blocks on a security review of the sandbox.

**Provisional exit criteria** (to be firmed at M8 close).
1. Browse, download, and play a hosted world entirely in-client, over the unmodified v1
   archive format, with no account required to download.
2. Sandbox security review passed; archive content hashes and engine-version requirements
   enforced on download and load.
3. Hosted worlds carry provenance/attribution metadata surfaced in the browser.
4. The co-hosted M7 session relay runs on the same backend (D-26).

**Depends on:** M8, D-20 sandbox audit. **Blocks:** nothing (terminal milestone).

---

## 14. Cross-milestone gates summary

| Gate | Introduced | Enforced through |
|---|---|---|
| License/provenance scans (G4.x, incl. generated-asset provenance G4.7) | M0 (G4.7 from M4) | every milestone |
| Import-graph separation (sim ↛ render) | M0 | every milestone |
| Determinism hash matrix (G5.1/G5.2) | M1 | every milestone |
| Scheduler serialization round-trip (D-9, R8; spike already green, D-28) | M1 (production), M3 (full sim) | every milestone |
| Manifest regeneration reproducibility (incl. Lua bindings from M5) | M2 | every milestone |
| Tick budget + zero-alloc tick (G3.3/G3.8); 1,000-unit stretch tracked (G3.10) | M3 | every milestone |
| FPS, draw-call, zero-alloc frame (G3.1/G3.2/G3.9) | M4 | every milestone |
| Locale-table coverage of user-facing strings (D-17) | M4 | every milestone |
| Audit 0-unmapped/0-duplicated (G1.x) | M5 | every milestone |
| Lua sandbox escape/quota tests + Lua determinism (G5.7, R-SEC-1) | M5 | every milestone |
| AI-domain isolation tests (R-EXEC-3) | M5.5 | every milestone |
| Full §5.3 budget suite; save/load + replay verification (G5.3, hard M7 gate) | M6 | release gate |
| Desync-detection fault injection | M7 | M7 onward |

Tooling that powers these gates — `jassgen`, the asset-validation CLI, `assetgen`, and the
CI benchmark harness — is specified in [Tooling](./tooling.md).
