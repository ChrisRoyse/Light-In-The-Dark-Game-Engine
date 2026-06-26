# Product Requirements Document — Light in the Dark Game Engine

| | |
|---|---|
| **Status** | Draft v1.0 |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Repo** | `light-in-the-dark-game-engine` |

---

## 1. Overview

Light in the Dark (LitD) is a Warcraft III–inspired RTS game engine written in pure Go, rendered with the [G3N](https://github.com/g3n/engine) OpenGL engine. The public scripting API is a faithful, deduplicated Go port of the Warcraft III JASS API surface (as documented by [cipherxof/war3-types](https://github.com/cipherxof/war3-types)), redesigned to be the simplest possible API that loses no power.

The engine is **deterministic by construction** (no AI/ML at runtime, no proprietary tech), uses only **free, open-source assets and dependencies**, and targets **low-tier hardware** as the primary performance baseline.

### 1.0 Purpose: a world-building and idea-explanation platform

LitD is not only a game engine. The end goal is a tool for **building worlds and explaining ideas** — a creativity amplifier where games are one output among simulations, interactive explanations, and imagined worlds. The WC3/Blizzard API is the starting vocabulary because it is a proven, complete grammar for describing interactive worlds (units, regions, triggers, timers, cinematics).

Two consequences shape every design decision:

1. **Beginner-simple surface, full-power core.** A beginner must be able to create with a handful of obvious calls; an expert (or the engine itself) must find *no missing capability*. No feature of the WC3 API surface is truncated — complexity is folded into canonical functions and options structs (§4.2), never amputated. Simplicity is achieved by *shape*, not by *subsetting*.
2. **AI-agent authorability is a first-class use case.** AI coding agents will write Go (later Lua, §5.5) against this API to generate playable worlds from user intent. This requires: a deterministic headless sim (R-SIM-4) agents can test against, machine-verifiable outputs (state hashes, framebuffer screenshots — §5.5), exhaustive godoc on every public symbol, and the generated `api-manifest.json` (§6.4) doubling as a machine-readable API catalog for agent context. Provided assets (§3.3, expanded over time) become the palette agents compose with.

### 1.1 Source-material clarification

The originally requested `components.json` and `blizzard.json` **do not exist** in the war3-types repository. The equivalent source material, which this PRD adopts as the API basis, is:

| File | Contents | Count |
|---|---|---|
| `repoes/war3-types/scripts/common.j` | Native (engine-level) JASS API | **1,536 natives** |
| `repoes/war3-types/scripts/blizzard.j` | Blizzard helper ("BJ") wrapper functions | **985 functions** |
| `repoes/war3-types/core/common.d.ts` | Typed version of common.j (1,534 declarations) | — |
| `repoes/war3-types/core/blizzard.d.ts` | Typed version of blizzard.j (985 declarations) | — |
| `repoes/war3-types/core/commonai.d.ts` | AI scripting natives | — |

If JSON manifests are required downstream, they will be **generated** from these files by a parser tool (see §6.4).

---

## 2. Goals and Non-Goals

### 2.1 Goals

1. **G1 — Full API power, zero duplication.** Port the complete WC3 API capability surface to Go. Every duplicated native/BJ pair collapses into exactly one canonical function — the most general ("complex") version — so no capability is lost and no code is repeated.
2. **G2 — Smallest possible public API.** The public layer must be the smoothest, least intrusive API achievable: idiomatic Go, small interface count, options structs instead of parameter explosions, no leaked internals.
3. **G3 — Low-tier hardware performance.** Smooth gameplay (60 FPS render, fixed 20 Hz simulation tick) on integrated GPUs (Intel UHD-class) and 4 GB RAM machines.
4. **G4 — Zero-cost asset and tech stack.** All runtime dependencies open source and **permissive only** (BSD/MIT/Apache — copyleft GPL/AGPL/LGPL is a hard exclusion, D-21); all game models CC0-licensed. No proprietary MDX/MDL Blizzard formats, no paid middleware, no runtime AI inference. The engine itself is **proprietary, permanently** (D-21); the public surface is the world archive format spec + Lua API docs. Distribution: own site only (D-22).
5. **G5 — Determinism.** Identical inputs produce identical simulation states across runs (prerequisite for replays, lockstep multiplayer, and testing).
6. **G6 — Creative platform.** The engine serves world-building and idea-explanation, not only games: beginner-simple authoring surface with zero capability truncation (§1.0).
7. **G7 — Verifiable by agents.** Every observable outcome (sim state, rendered frame, audio events) is inspectable by a non-human author: state-hash dumps, framebuffer screenshot capture, and structured logs are engine features, not debug afterthoughts (§5.5).

### 2.2 Non-Goals (v1)

- Recreating Warcraft III content, campaigns, balance data, or art (IP risk; only the *API shape* is ported — JASS function signatures are facts about an interface, not copyrightable expression, but we re-implement all bodies from scratch).
- MDX/MDL model loading (proprietary Blizzard formats — explicitly out).
- Mobile/console targets. Desktop Windows/Linux/macOS only (G3N's supported platforms).
- Netcode before milestone M7. Multiplayer itself is **committed** (lockstep, see §9.5 and M7): v1 milestones M0–M6 ship no transport/lobby code, but determinism (G5) and the command-stream design are load-bearing prerequisites for M7, not optional groundwork.
- General-purpose engine competing with Unity/Godot. This is an RTS-shaped engine.

---

## 3. Research Findings (basis for technical decisions)

### 3.1 Rendering dimensionality: low-poly 3D, not 2D/2.5D

Warcraft III itself is **true 3D with a constrained RTS camera**, not 2.5D — it used very low-poly models for its era ([Hive Workshop discussion](https://www.hiveworkshop.com/threads/what-makes-wc3-graphically-so-heavy.263661/)). Its main performance failing was *lack of culling/LOD optimization*, not the choice of 3D ([same source](https://www.hiveworkshop.com/threads/what-makes-wc3-graphically-so-heavy.263661/)).

Decision: **low-poly true 3D with a locked (yaw-fixed, pitch-clamped, zoom-clamped) perspective camera.**

Rationale:
- G3N is a 3D engine; its sprite/2D support is incidental (sprite sheets), not a 2D pipeline. Fighting the engine to do 2D wastes its scene graph, lighting, and glTF pipeline.
- Pre-rendered 2D sprites (the Starcraft 1 / [Strike Tactics approach](https://striketactics.net/devblog/3d-vs-2d-visuals-rts-games)) trade GPU cost for large texture memory and per-angle sprite baking — worse on 4 GB RAM low-tier targets and a massive asset-pipeline cost (8+ directions × N animations × N units).
- A fixed RTS camera makes 3D cheap: stable view frustum, predictable overdraw, aggressive far-plane culling, and no need for high-poly detail (camera never gets close). G3N enables **view-frustum culling by default** ([g3n#269](https://github.com/g3n/engine/issues/269)).
- The CC0 asset ecosystem (§3.3) is overwhelmingly low-poly 3D glTF — near-zero asset cost.

### 3.2 Model format: glTF 2.0 binary (.glb), core profile only

G3N ships loaders for **glTF (.gltf/.glb), Wavefront OBJ, and COLLADA (.dae)** ([G3N README](https://github.com/g3n/engine)).

glTF/GLB is the unambiguous winner for runtime delivery:
- Benchmarks show glTF loads fastest with lowest memory among OBJ/FBX/STL/glTF ([KoreaScience study](https://koreascience.kr/article/JAKO201909258119836.pdf)).
- GLB stores mesh data in unified GPU-ready topology — readable directly into GPU buffers with no intermediate processing, ~5× smaller and >10× faster to parse than text formats like OBJ ([Threekit analysis](https://www.threekit.com/blog/gltf-vs-fbx-which-format-should-i-use)).
- It is the open Khronos standard; OBJ has no animation/rigging support and COLLADA is effectively legacy ([Alpha3D comparison](https://www.alpha3d.io/kb/3d-modelling/gltf-vs-obj/)).

**Constraint discovered:** G3N's glTF loader is *partial* — extensions such as `KHR_materials_specular` and `KHR_materials_ior` are unsupported ([g3n#296](https://github.com/g3n/engine/issues/296)). Mitigation:
- **R-FMT-1:** All game assets MUST be core glTF 2.0 GLB — no KHR extensions except `KHR_materials_unlit` (which G3N supports: `loader/gltf/khr_materials_unlit.go`).
- **R-FMT-2:** An asset-validation CLI tool rejects assets using unsupported extensions at build time, not runtime.
- **R-FMT-3:** No Draco/Meshopt compression in v1 (G3N loader doesn't decode it); low-poly models are small enough uncompressed.

### 3.3 Assets: CC0 low-poly fantasy/RTS packs (zero cost)

| Pack | Contents | License | Format |
|---|---|---|---|
| [Quaternius — Ultimate Fantasy RTS](https://quaternius.com/packs/ultimatefantasyrts.html) | 128 animated, textured RTS models (units + buildings, evolution stages) | Free, commercial OK | glTF |
| [KayKit — Medieval Hexagon Pack](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0) | 200+ tiles/buildings/props, 4 team colors, single 1024² gradient atlas (downsamples to 128²) | CC0 | glTF/FBX |
| [KayKit — Medieval Builder Pack](https://opengameart.org/content/kaykit-medieval-builder-pack-10) | 200+ medieval scenery/buildings | CC0 | glTF/OBJ/DAE |
| [Kenney — Castle Kit](https://kenney-assets.itch.io/castle-kit) / [Hexagon Kit](https://kenney-assets.itch.io/hexagon-kit) / [Retro Medieval Kit](https://opengameart.org/content/retro-medieval-kit) | ~70–120 models each, siege weapons, terrain | CC0 | glTF/OBJ |

These cover units, buildings, terrain, and props for a WC3-style fantasy RTS with **$0 asset budget**. The KayKit single-atlas texturing style is the performance model to standardize on (§5.2).

### 3.4 Engine viability and risks (G3N)

- Confirmed features used: hierarchical scene graph, ambient/directional/point/spot lights, PBR + unlit materials, perspective & orthographic cameras, animation framework, morph targets, OpenAL spatial audio, GLSL custom shaders, integrated GUI widgets ([G3N README](https://github.com/g3n/engine)).
- Requires OpenGL driver + GCC-compatible C compiler (cgo) at build time.
- **Risks:** partial glTF 2.0 support (mitigated §3.2); physics engine is experimental (we don't need it — RTS sim is custom, §5.1); no built-in GPU instancing path documented (mitigated by draw-call budget + mesh merging, §5.2); project activity is moderate (mitigation: vendored fork in `repoes/engine`, we maintain patches).

---

## 4. Public API Design

### 4.1 Architecture: two layers, one implementation

```
┌──────────────────────────────────────────────────┐
│ litd/api        — public, idiomatic Go API       │  ← what game devs see
├──────────────────────────────────────────────────┤
│ litd/sim        — deterministic simulation core  │  ← ECS, fixed tick, no rendering
│ litd/render     — G3N presentation layer         │  ← reads sim state, never writes
│ litd/asset      — GLB/atlas/audio pipeline       │
└──────────────────────────────────────────────────┘
```

Hard rule: **simulation never imports render; render never mutates simulation.** This keeps the sim deterministic and headless-testable.

### 4.2 Deduplication policy (the "complex version only" rule)

The WC3 API contains massive duplication: 985 `blizzard.j` BJ functions wrap the 1,536 `common.j` natives, often trivially (e.g. `KillUnitBJ(u)` → `KillUnit(u)`, `SetUnitLifeBJ` → `SetUnitState(u, UNIT_STATE_LIFE, v)`). Porting rules, applied mechanically:

| Case | Rule | Example |
|---|---|---|
| **D1.** BJ is pure passthrough to a native | Drop the BJ entirely. Native is canonical. | `IssueTargetOrderBJ` → gone; `Unit.Order()` exists once |
| **D2.** BJ reorders/defaults parameters | Drop the BJ; canonical Go function takes the full ("complex") parameter set, with an options struct for defaults. | `CreateUnitAtLocSaveLast` → `Game.CreateUnit(owner, typ, pos, facing)` |
| **D3.** Native family differs only by type/arity (`SetUnitX`/`SetUnitY`/`SetUnitPosition`, `...Loc` variants) | One canonical function on the most general form. `location` (heap point) variants collapse into value-type `Vec2`. | `Unit.SetPosition(Vec2)` only |
| **D4.** BJ adds real logic (e.g. `PolledWait`, group-enum helpers, `MeleeStartingUnits`) | Keep the logic once, as a documented helper in a separate `litd/api/helpers` package — clearly layered *on top of* core, never shadowing it. | `helpers.PolledWait(d)` |
| **D5.** Getter/setter pairs across states | Collapse onto typed accessors. | `GetUnitState`/`SetUnitState` → `Unit.Life()`, `Unit.SetLife(v)` etc., backed by one state table |

Acceptance criterion: a generated audit report proves every one of the 2,521 source functions is either (a) mapped to exactly one canonical Go symbol or (b) explicitly tombstoned with a reason (deprecated, gameplay-irrelevant, superseded). **No capability silently dropped, no symbol implemented twice.**

### 4.3 API shape: handles → typed objects

JASS's flat handle-based API (`unit`, `trigger`, `timer`, … — 60+ handle subtypes in `common.d.ts`) becomes small typed Go objects grouped by noun. Target: **~20 public types** covering all 1,536 natives' capability.

```go
// Illustrative surface — final signatures defined in the API spec (M2).
g := litd.NewGame(cfg)

p := g.Player(0)
u := g.CreateUnit(p, "footman", litd.Vec2{X: 128, Y: 256}, litd.Deg(270))

u.SetLife(u.MaxLife() * 0.5)
u.Order(litd.OrderAttackMove, target.Position())

g.OnEvent(litd.EventUnitDeath, func(e litd.Event) {
    fmt.Println(e.Unit().Name(), "died")
})

g.After(30*time.Second, func() { g.Defeat(p, "time out") })
```

Design rules:
- **R-API-1:** Methods on nouns (`Unit.SetLife`), never free functions with handle params.
- **R-API-2:** Value types for math (`Vec2`, `Angle`); no heap `location` objects, no manual `RemoveLocation` — GC handles lifetime, sim pools internally.
- **R-API-3:** Variadic options structs for the long-tail parameters (the "complex version" stays one function without 12 positional args).
- **R-API-4:** Events replace the trigger/condition/action/filter object zoo (`trigger`, `triggercondition`, `boolexpr`, `filterfunc`, `conditionfunc` → one `OnEvent` + Go closures).
- **R-API-5:** Errors are returned/panic-free in hot paths; invalid handles return zero-value no-op objects (WC3 semantics) with a debug-mode assert.
- **R-API-6:** Public API package has **zero** G3N types in its signatures (least intrusive layer; rendering swappable in principle).

---

### 4.4 Execution-model semantics (from the JASS runtime)

The [JASS Manual library documentation](https://jass.sourceforge.net/doc/library.shtml) documents runtime behavior the function signatures alone don't capture. These semantics shape the Go design:

| JASS semantics | Implication for LitD |
|---|---|
| "Threads" are **cooperative coroutines** scheduled by the game loop — they yield only at `Sleep`/`TriggerSleepAction`/opcode limits; globals are shared without locks because exactly one thread runs at a time | **R-EXEC-1:** Script logic runs on a **deterministic cooperative scheduler inside the sim tick** — never free-running goroutines. Goroutines may be used as the coroutine mechanism only with strict one-at-a-time handoff and deterministic resume order. Determinism (R-SIM-2) depends on this. |
| Trigger actions spawn a new thread per firing; conditions (`boolexpr`) must be wait-free | **R-EXEC-2:** `OnEvent` handlers run synchronously at the event point in deterministic registration order; a handler that calls a wait (`helpers.PolledWait`, §4.2 D4) suspends onto the scheduler and resumes on a later tick. Condition-style filters must be pure functions (enforced by API shape: filters take state, return bool, no game-mutation access). |
| AI scripts: max 6 threads/player, separate script contexts, **no shared globals** with the map script; communication via integer-pair command stacks; string/callback natives broken in AI context | **R-EXEC-3:** Confirms `commonai` is a separate isolated execution domain — supports the §9.4 draft decision to defer computer-player AI to v2. When ported, AI runs in its own sandboxed scheduler with message-queue communication (typed Go channel equivalent of the command stack), not shared state. |
| Collections (group/force/destructable enum) are **callback-based** with implicit current-element accessors (`GetEnumUnit`) | **R-EXEC-4:** Canonical Go API replaces callback-enum with slice/iterator returns (`g.UnitsIn(rect, filter) []Unit`) — same capability, no hidden thread-local "current element" state. Collapses the `ForGroup`/`GetEnumUnit`/`FirstOfGroup`-loop duplication per §4.2 D3. |
| `Sleep` granularity is coarse and tick-quantized; main AI thread must never return | **R-EXEC-5:** All waits quantize to sim ticks (50 ms at 20 Hz). No sub-tick timing exists in the public API — prevents scripts from depending on render-rate timing. |

## 5. Engine Requirements

### 5.1 Simulation core (deterministic)

- **R-SIM-1:** Fixed timestep, **20 ticks/s** (WC3-compatible cadence), decoupled from render. Render interpolates between sim states.
- **R-SIM-2:** Bit-for-bit determinism: same map + same ordered command stream → same state hash. Enforced by:
  - all gameplay math in fixed-point (`int32` 16.16 or `int64` 32.32) or strictly ordered float ops — decision spike in M1;
  - a single seeded PRNG owned by the sim; no `map` iteration in gameplay code (Go map order is random) — keyed slices/ordered structures only;
  - no wall-clock or goroutine-race inputs inside a tick.
- **R-SIM-3:** Data-oriented ECS layout (struct-of-arrays component stores) for units/missiles/buffs — cache-friendly on low-tier CPUs. Capacities provisioned for **1,000 active units + 1,000 missiles** (D-18 stretch target on recommended spec); the §5.3 low-tier budgets are guaranteed at 500.
- **R-SIM-6:** The cooperative scheduler is **serializable** (D-9): suspended coroutines, timers, and event subscriptions serialize into the save format — mid-game save/load is v1 scope, and campaign cross-map persistence (D-15) rides the same mechanism.
- **R-SIM-4:** Headless mode: sim runs and replays verify with no GPU/window (CI-testable).
- **R-SIM-5:** Pathfinding deterministic A*/flow-field on the WC3-style grid; no threads inside tick resolution in v1 (parallelism only across full-tick boundaries if ever added).
- **R-SIM-7 (deliberate WC3 divergence):** Abilities are **dynamically built and registered**, never hardcoded: an ability definition is an effect *pipeline* composed from plugin-codeable effect mechanisms held in a deterministic registry (engine standard library + world-registered Go/Lua plugins; the registered set folds into the content hash, so mismatched plugin sets refuse to join rather than desync). Missiles are **independent first-class sim objects** — pooled entities with their own guidance programs and impact behaviors, spawnable and queryable through the public API, not cosmetic attack attachments. Spec: [combat-and-orders.md §3.5, §5.1](prd/04-simulation/combat-and-orders.md).

### 5.2 Rendering (G3N presentation layer)

- **R-RND-1:** Locked RTS perspective camera (WC3 default ~34° from vertical, fixed yaw, zoom-clamped). Orthographic mode available behind a flag (cheaper; pre-WC3 look).
- **R-RND-2:** Asset budget per unit model: ≤ 1,500 triangles; buildings ≤ 4,000; one shared 1024×1024 atlas texture per faction/biome (KayKit pattern, downsampled to 256² on low preset).
- **R-RND-3:** Draw-call budget: ≤ 300 draw calls/frame at max army size. Achieved via shared-material batching and static terrain chunk merging; GPU instancing investigated as a G3N patch in M3 (engine is vendored).
- **R-RND-4:** Lighting: 1 directional (sun) + ambient only in gameplay; point/spot lights reserved for spell VFX with a hard cap (≤ 8 active).
- **R-RND-5:** Unlit/`KHR_materials_unlit` material path as the "low" graphics preset — skips PBR entirely.
- **R-RND-6:** Frustum culling (G3N default) + tuned near/far planes; far plane hugs the camera bounding box of the visible map area.
- **R-RND-7:** Team color via shader parameter (one extra uniform), not per-team textures.

### 5.3 Performance budgets (acceptance gates, low-tier reference machine: dual-core 2 GHz, Intel UHD 620, 4 GB RAM)

| Metric | Budget |
|---|---|
| Render frame rate | ≥ 60 FPS typical scene; ≥ 30 FPS worst case (500 units on screen) |
| Sim tick (20 Hz) | ≤ 10 ms worst case (50% headroom) |
| Cold start to main menu | ≤ 5 s |
| Map load (128×128) | ≤ 10 s |
| RAM (full match) | ≤ 1.5 GB |
| Binary + base assets | ≤ 300 MB |

Budgets are CI-enforced from M3 onward via headless sim benchmarks and a scripted render benchmark scene.

### 5.3.1 Go garbage-collection discipline

Go's GC is acceptable for a 20 Hz sim + 60 FPS render **only if steady-state allocation is near zero**. GC pressure is treated as a budgeted resource, not an afterthought:

- **R-GC-1:** Zero heap allocations per sim tick and per render frame at steady state (excluding map load, unit creation bursts). Enforced in CI with `testing.AllocsPerRun` benchmarks on the tick and frame paths.
- **R-GC-2:** All transient gameplay objects (missiles, buffs, events, order queue entries) come from preallocated pools; ECS component stores are preallocated struct-of-arrays slices that never reallocate mid-match (capacity fixed at map load).
- **R-GC-3:** Value types everywhere in hot paths (`Vec2`, `Angle`, event payloads); no interface boxing or closures allocated inside the tick loop; string building/logging is debug-mode only.
- **R-GC-4:** GC tuning is a fallback, not a strategy: `GOGC`/`debug.SetGCPercent` and soft memory limit may be set at startup, but budgets must pass with defaults.
- **R-GC-5:** CI fails on regression: any change increasing allocs/tick or allocs/frame above zero baseline is rejected.

### 5.4 Audio, UI, input

- **R-AUD-1:** OpenAL via G3N; `.ogg` only (free codec). 3D positional for world sounds, 2D for UI.
- **R-UI-1:** In-game HUD built on G3N's integrated GUI widgets; exposed through the same public API (`g.UI()...`) mirroring WC3 frame natives' capability (collapsed per §4.2 rules).
- **R-INP-1:** WC3-grade input model: drag-select, control groups 0–9, smart/right-click orders, hotkeys, edge-pan + middle-drag camera.

### 5.5 Verification protocol (FSV)

All milestone and task acceptance follows the **Full State Verification protocol** (`prompts/fsv.md`): never trust return values alone — identify the source of truth, execute, then independently read the source of truth and present evidence; manually audit ≥3 edge cases with before/after state.

Engine support required to make FSV possible (these are product features, per G7):

- **R-FSV-1:** `Game.Screenshot(path)` — capture the current framebuffer to PNG at any time, headless-renderable, so a verifying agent can *look at the screen* and confirm what is rendered.
- **R-FSV-2:** `Game.StateDump()` / `Game.StateHash()` — full serialized sim state and stable hash at any tick (also serves R-SIM-2 determinism testing).
- **R-FSV-3:** Structured event log (every command ingested, every event fired, tick-stamped) so cause → effect chains are traceable.
- **R-FSV-4:** No mocks in engine tests; no fallbacks that mask failure — errors are loud, logged with cause, and fail the run (fsv.md policy).

### 5.6 Modding/scripting

*Revised per D-2026-06-11-8/20.*

- Systems/engine code is Go against `litd/api` (compiled, zero interpreter cost).
- **v1 (M5) ships an embedded deterministic Lua VM** (gopher-lua family, determinism-audited) exposing the same canonical API; bindings are **generated from `api-manifest.json`** so Go and Lua surfaces cannot drift. Worlds are runtime-loadable — creators and AI coding agents author without a Go toolchain.
- **Hard sandbox (R-SEC-1):** world Lua has no io/os/net access — game API only — with per-tick instruction and memory quotas (the quotas double as the lockstep stall guard). Worlds cannot touch the player's machine. Any world-sharing feature is gated on this sandbox.
- M4 onward: every user-facing string (engine UI and world-author strings) flows through locale string tables; v1 ships English, translations are data drops (D-17).

---

## 6. Asset & Data Pipeline

- **R-AST-1:** Source of truth for game data (unit stats, abilities, upgrades) is plain JSON/TOML tables in `data/` — the WC3 "SLK/object data" analogue — loaded once, immutable at runtime.
- **R-AST-2:** All models pass the asset-validation CLI (core-glTF check, triangle budget, atlas usage, missing-animation check) before entering `assets/`.
- **R-AST-3:** Standard animation clip names contractually required per unit model: `Idle`, `Walk`, `Attack`, `Death` (+ optional `Spell`, `Portrait`). Validator enforces.
- **R-AST-4 (parser tool):** `tools/jassgen` parses `common.j`/`blizzard.j` + the `.d.ts` files into a machine-readable `api-manifest.json` (the requested "components/blizzard JSON", generated): every function, its classification (D1–D5, §4.2), and its canonical Go mapping. This manifest drives code generation of API stubs, **the Lua binding generator (D-8)**, and the §4.2 audit report.
- **R-AST-5 (generative pipeline, D-12):** asset categories with no CC0 source (hero portraits, spell VFX textures, voice lines, UI icons, terrain splat/cliff texture sets) are produced by `tools/assetgen` — image/TTS generation run at **asset-build time only**, hand-curated, committed as owned assets with provenance entries. Zero runtime AI inference (G4.6 unaffected).

---

## 7. Milestones

| # | Milestone | Exit criteria |
|---|---|---|
| **M0** | Repo bootstrap | Go module, vendored G3N, CI (lint/test/headless), asset packs downloaded + validated |
| **M0.5** | **"First Light" demo** | Earliest playable proof, built before the full architecture: window opens, terrain plane renders, one animated GLB unit on screen, right-click moves it (straight-line, no pathfinding), drag-select highlights it. `Game.Screenshot()` works. **Verified per FSV protocol (§5.5): screenshot evidence of the unit at its commanded position + state dump confirming sim coordinates match.** Code is throwaway-tolerant — it seeds M3/M4 but must not constrain them. |
| **M1** | Determinism foundation | **Spike already executed 2026-06-11 (D-27/D-28, `spikes/`): fixed-point int64 32.32 validated (182 µs/2,000-entity tick = 1.8% of budget); stackless serializable scheduler validated (mid-run save/restore bit-identical).** Remaining M1 work: productionize `litd/fixed` + scheduler, wire the 10k-tick hash test across the OS/arch CI matrix |
| **M2** | API manifest + spec | `jassgen` outputs `api-manifest.json`; all 2,521 functions classified D1–D5; public API spec doc signed off |
| **M3** | Sim core | ECS, 20 Hz tick, movement, pathfinding, combat for 500 units within budget (headless benchmark) |
| **M4** | Render core | GLB units/buildings/terrain rendered with animation, team color, RTS camera; 60 FPS on reference machine |
| **M5** | API v1 + Lua | Full canonical API implemented over sim+render; audit report shows 0 unmapped / 0 duplicated. **Lua VM embedded (deterministic, hard-sandboxed: no io/os/net, per-tick instruction+memory quotas), bindings generated from `api-manifest.json`; worlds runtime-loadable** (D-8, D-20) |
| **M5.5** | **AI domain** | Full `commonai` port (D-6): second sandboxed scheduler domain, isolated contexts, command-stack messaging (R-EXEC-3); all AI natives canonical |
| **M6** | Vertical slice = **LitD game v0.1** (*First Flame*; identity spec [`prd/10-game/identity.md`](prd/10-game/identity.md), D-32) | Playable skirmish (Vigil vs Unbound, Beacon/Flicker map) vs the real AI domain: build, train, fight, win/lose; **the flagship game, not a tech demo (D-24)** — art style/factions/lore established; **mid-game save/load shipping (D-9)**; **world archive format defined (D-14)**; all §5.3 budgets green in CI; **replay verification (G5.3) green — lockstep-readiness proof for M7** |
| **M7** | **Multiplayer (lockstep)** | 2–8 player skirmish on **quic-go, star topology (D-26)**: LAN = player-hosted in-process, internet = lightweight relay (co-located with M9 hub, no NAT traversal needed); command turns every 2–4 ticks, adaptive input-delay buffer, stall = pause + grace drop; state hash piggybacked ~1/s, mismatch → desync dump. Replays and netplay share the command-stream format (D-5). **In-client replay viewer + live observer slots (D-16)** |
| **M8** | **World Editor** | In-engine visual editor: terrain sculpt/paint, unit/doodad placement, map metadata, world-archive save; campaign menu/mission-flow UI (D-10, D-15) |
| **M9** | **World hub (committed, D-23)** | Hosted world repository + in-game browser over the v1 archive format: static-friendly index (no account to download), accounts/ratings later; co-hosts the M7 session relay; hard-gated on the Lua sandbox (D-14, D-20) |

---

## 8. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| G3N glTF gaps (skinning edge cases, extensions) | Medium | High | Core-profile-only assets; vendored engine, patch loader; fallback to [qmuntal/gltf](https://github.com/qmuntal/gltf) parser feeding G3N meshes (suggested in [g3n#296](https://github.com/g3n/engine/issues/296)) |
| No GPU instancing in G3N → draw-call ceiling | Medium | Medium | Batching/merging first; instancing patch in vendored fork if budget missed |
| Float non-determinism across CPUs | Medium | High | M1 spike; fixed-point fallback decided before sim code is written |
| API surface underestimation (some natives need engine features G3N lacks, e.g. fog of war) | High | Medium | Fog-of-war, minimap, selection circles are custom shaders/render passes — scheduled inside M4 |
| WC3 IP proximity | Low | High | Only API *shape* ported; all implementations, names-where-feasible, data, and assets original/CC0; no Blizzard formats or content |
| G3N project staleness | Medium | Medium | Vendored in-repo (`repoes/engine`); we own maintenance |

---

## 9. Open Questions — DECIDED 2026-06-11

All four questions plus the multiplayer question are decided; full rationale in
[`docs/prd/01-vision/decisions.md`](prd/01-vision/decisions.md).

1. **Sim math → fixed-point `int64` 32.32** (D-2026-06-11-1). M1 spike now validates performance/precision of fixed-point rather than arbitrating; reopens only if the tick budget fails.
2. **Naming → idiomatic Go only** (D-2026-06-11-2); generated JASS→Go mapping table is the migration aid.
3. **Terrain → heightmap + cliffs in v1** (D-2026-06-11-7, supersedes D-3): WC3-fidelity heightmap mesh, cliff levels, ramps, splatting in M4; sim-side grid abstraction unchanged.
4. **`commonai` → FULL v1 port** (D-2026-06-11-6, supersedes D-4): second sandboxed scheduler domain, all AI natives canonical, own milestone M5.5; M6 melee opponent runs on the real AI domain.
5. **Multiplayer → committed, lockstep, milestone M7** (D-2026-06-11-5). Deterministic lockstep over the existing command stream (commands exchanged, not state); `StateHash` doubles as desync detector; replay verification (G5.3) becomes a hard M6 exit criterion.

**Third-session decisions (D-2026-06-11-21..30): everything upfront, zero deferred decisions.** All spikes executed same-day (`spikes/`, results in the decision record): fixed-point 32.32 validated (D-27), stackless serializable scheduler validated (D-28), pathfinding architecture set — amortized expansion budget + HPA* + path sharing + flow fields for big groups (D-29), g3n instancing patch confirmed viable (D-30). Researched and decided: Lua VM = vendored gopher-lua fork with 4 LITD patches (D-25); transport = quic-go star topology with hub-co-located relay (D-26). Product: engine **proprietary permanently**, world-format + Lua API docs public (D-21); **own-site distribution only** (D-22); world hub **committed M9** (D-23); the M6 slice is **the real LitD game v0.1**, growing every milestone (D-24).

**Second-session decisions (D-2026-06-11-6..20, full record in [`decisions.md`](prd/01-vision/decisions.md)).** Standing directive: features are not cut or deferred because they are hard. Highlights: Lua scripting in v1 (M5, hard-sandboxed, bindings generated from the manifest); full mid-game save/load in v1 (serializable scheduler from M3); doodads get full WC3 parity (handle promotion on first script touch); World Editor committed as M8; campaign persistence architecture in v1 with campaign UI at M8; open world-archive format in v1 with hosted hub as M9 candidate; replay viewer + observer slots at M7; string tables from M4 (English shipped); 1,000-unit stretch target (500 stays the low-tier guarantee); asset gaps filled by a build-time generative pipeline (zero runtime AI); web/WASM stays out; distribution + license decided at M6 (repo private until then).

---

## 10. Sources

- [JASS Manual — library/runtime semantics (threads, triggers, enumeration, AI command stacks)](https://jass.sourceforge.net/doc/library.shtml)
- [G3N engine README — features, loaders, requirements](https://github.com/g3n/engine)
- [g3n#296 — partial glTF 2.0 extension support](https://github.com/g3n/engine/issues/296)
- [g3n#269 — frustum culling default](https://github.com/g3n/engine/issues/269)
- [KoreaScience — 3D file format performance study (glTF fastest)](https://koreascience.kr/article/JAKO201909258119836.pdf)
- [Threekit — glTF vs FBX](https://www.threekit.com/blog/gltf-vs-fbx-which-format-should-i-use)
- [Alpha3D — glTF vs OBJ vs FBX comparison](https://www.alpha3d.io/kb/3d-modelling/gltf-vs-obj/)
- [Hive Workshop — WC3 rendering/optimization analysis](https://www.hiveworkshop.com/threads/what-makes-wc3-graphically-so-heavy.263661/)
- [Strike Tactics — 3D vs 2D visuals in RTS](https://striketactics.net/devblog/3d-vs-2d-visuals-rts-games)
- [Quaternius — Ultimate Fantasy RTS pack](https://quaternius.com/packs/ultimatefantasyrts.html)
- [KayKit — Medieval Hexagon Pack](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0)
- [Kenney — CC0 asset library](https://kenney.nl/assets)
