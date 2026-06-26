# Public API — Package Architecture and Layering

> Expands [PRD §4.1 (Architecture: two layers, one implementation)](../../PRD.md#41-architecture-two-layers-one-implementation).
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Siblings** | [Deduplication policy](deduplication-policy.md) · [Public API design](public-api-design.md) · [Execution model](execution-model.md) · [Naming & style](naming-and-style.md) |

---

## 1. The four packages

The engine is organized as one public package and three internal subsystems. The diagram in
[PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation) is normative; this section
gives each layer its contract.

```
┌──────────────────────────────────────────────────┐
│ litd/api        — public, idiomatic Go API       │  ← what game devs see
├──────────────────────────────────────────────────┤
│ litd/sim        — deterministic simulation core  │  ← ECS, fixed tick, no rendering
│ litd/render     — G3N presentation layer         │  ← reads sim state, never writes
│ litd/asset      — GLB/atlas/audio pipeline       │
└──────────────────────────────────────────────────┘
```

### 1.1 `litd/api` — the public surface

The only package a game developer imports. It contains the ~20 noun types
(`Game`, `Player`, `Unit`, `Timer`, …; enumerated in [Public API design §2](public-api-design.md#2-the-public-type-inventory)),
the event system, and the options structs. It owns *no state of its own*: every public type is a
thin, copyable handle (an entity ID plus a back-pointer to the `Game`) whose methods translate
directly into `litd/sim` commands and queries. The package is deliberately boring — its job is
ergonomics and stability, not logic.

`litd/api/helpers` sits alongside it for D4-class BJ logic that survives deduplication
(`helpers.PolledWait`, melee-setup utilities — see
[Deduplication policy §5](deduplication-policy.md#5-d4--bj-adds-real-logic-keep-once-in-helpers)).
Helpers are implemented *only* in terms of `litd/api` exports — they are the first proof that the
core API loses no power.

### 1.2 `litd/sim` — the deterministic core

The struct-of-arrays ECS, the 20 Hz fixed tick, pathfinding, combat resolution, the order queue,
the cooperative script scheduler ([Execution model](execution-model.md)), and the single seeded
PRNG ([PRD R-SIM-2](../../PRD.md#51-simulation-core-deterministic)). Everything inside `litd/sim`
obeys the determinism rules: no wall clock, no map iteration in gameplay code, no free-running
goroutines, fixed-point or strictly ordered float math (M1 spike decides). `litd/sim` compiles and
runs with **no GPU, no window, and no cgo path exercised** — see §4 (headless mode).

### 1.3 `litd/render` — the G3N presentation layer

Owns the G3N scene graph, the RTS camera, animation playback, team-color shaders, fog-of-war and
selection-circle passes, and frame interpolation between sim states
([PRD §5.2](../../PRD.md#52-rendering-g3n-presentation-layer)). It consumes a **read-only snapshot
view** of sim state each frame. The render layer may lag, drop frames, or be absent entirely; the
simulation neither knows nor cares.

### 1.4 `litd/asset` — the content pipeline

GLB loading (core glTF 2.0 profile only, per [PRD R-FMT-1](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only)),
texture atlas management, `.ogg` audio, and the JSON/TOML game-data tables
([PRD R-AST-1](../../PRD.md#6-asset--data-pipeline)). It serves two distinct consumers with two
distinct products: *gameplay-relevant data* (unit stats, collision radii, pathing footprints)
flows to `litd/sim`; *presentation data* (meshes, animations, textures, sounds) flows to
`litd/render`. The split matters: a corrupt texture must never be able to change a collision
radius, because that would let an asset difference desync a deterministic match.

## 2. Import rules

The dependency graph is a strict DAG, enforced mechanically (a `go list`-based CI lint, in place
from M0):

```
litd/api ──────► litd/sim ◄────── litd/asset (data tables)
    │                ▲
    │                │ (read-only snapshots)
    └──────────► litd/render ◄─── litd/asset (meshes/audio)
                     │
                     └──────────► G3N (vendored, repoes/engine)
```

| Rule | Statement | Enforcement |
|---|---|---|
| **IMP-1** | `litd/sim` imports neither `litd/render` nor `litd/api` nor G3N. Its allowed imports are the standard library (minus `time` for gameplay), `litd/asset`'s data-table types, and internal math packages. | CI import lint; build tag `litd_headless` must compile `litd/sim` without cgo |
| **IMP-2** | `litd/render` may import `litd/sim` (read-only snapshot types) and G3N. It exposes no symbol that returns or accepts a G3N type across the `litd/api` boundary. | CI import lint + API-surface lint (R-API-6) |
| **IMP-3** | `litd/api` imports `litd/sim` always and `litd/render` only behind constructor options; no public signature mentions either. | godoc/API-diff tooling ([Naming & style §4](naming-and-style.md#4-versioning-and-stability-policy)) |
| **IMP-4** | `litd/asset` imports neither `sim` nor `render` nor `api`. It defines the data types both consume. | CI import lint |
| **IMP-5** | Nothing outside `litd/render` imports G3N. G3N stays vendored in `repoes/engine` and reaches the build through exactly one package. | CI import lint |

The render layer communicates *back* to the sim only through the same public command funnel as
everything else: player input collected by `litd/render` (clicks, drags, hotkeys per
[PRD R-INP-1](../../PRD.md#54-audio-ui-input)) is translated into ordered sim commands — the same
command stream a replay file or a future network session would inject. There is no privileged
side door.

## 3. Why sim never imports render

This is the load-bearing rule of the whole architecture, and it is worth being explicit about the
four reasons it exists:

1. **Determinism (G5, R-SIM-2).** Render state is inherently non-deterministic: frame timing
   varies with GPU load, driver behavior, and window events. If simulation code could *see* any
   of it — a camera position, an animation phase, a "is this unit on screen" query — identical
   command streams would stop producing identical state hashes. Cutting the import means the
   compiler, not code review, guarantees the sim has nothing non-deterministic to read.
2. **Testability (R-SIM-4).** The 10k-tick state-hash reproducibility test (M1) and all gameplay
   benchmarks run headless in CI on machines with no GPU. That is only possible if `litd/sim`
   links without OpenGL, OpenAL, or cgo.
3. **Performance isolation (§5.3 budgets).** The sim tick has a 10 ms budget and a zero-alloc
   rule (R-GC-1). Render work — scene-graph mutation, uniform uploads — must not be able to creep
   into the tick path. A one-way snapshot boundary makes the tick's cost profile closed.
4. **Replaceability (R-API-6).** G3N is a moderate-activity dependency
   ([PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n)). Because exactly one package
   touches it and zero public signatures expose it, swapping or heavily patching the renderer is
   a contained operation, not a rewrite.

The reverse rule — *render never mutates simulation* — is enforced by the snapshot mechanism:
`litd/sim` publishes an immutable per-tick snapshot (double-buffered, preallocated per R-GC-2);
`litd/render` interpolates between the last two snapshots for smooth 60 FPS presentation over the
20 Hz sim ([PRD R-SIM-1](../../PRD.md#51-simulation-core-deterministic)). The snapshot types
export no setters.

## 4. Headless mode

Headless operation is a first-class build configuration, not a test shim:

- `litd.NewGame(cfg)` with `cfg.Headless = true` (or the `litd_headless` build tag, which removes
  `litd/render` from the binary entirely) constructs a game with sim + asset data tables only.
  The full public API works; calls that are presentation-only (`Sound.Play`, camera methods, UI
  frames) become deterministic no-ops that still validate their arguments in debug mode — the
  same zero-value-handle philosophy as
  [R-API-5](public-api-design.md#35-r-api-5--error-semantics-and-zero-value-handles).
- Headless mode drives: CI gameplay tests, the M1 determinism spike, the M3 500-unit benchmark,
  replay verification (re-run command stream, compare state hash), and — at M7
  (D-2026-06-11-5) — dedicated lockstep servers.
- The asset pipeline in headless mode loads *only* the gameplay-relevant data tables (stats,
  footprints), never meshes or audio, keeping CI fast and GPU-free.

A useful invariant falls out: **a headless run and a rendered run of the same command stream
produce the same state hash.** This is a standing CI test from M3, and it is the practical proof
that the import rules are doing their job.

## 5. "Two layers, one implementation"

The phrase from PRD §4.1 has a precise meaning: there is exactly **one** implementation of every
capability, living in `litd/sim` (or `litd/render` for presentation), and exactly **one** public
expression of it, living in `litd/api`. The deduplication policy
([deduplication-policy.md](deduplication-policy.md)) collapses the 2,521 JASS functions *before*
they reach the public layer, so `litd/api` never contains two routes to the same effect. The
audit report (M2/M5) checks this end to end: JASS symbol → manifest classification → canonical Go
symbol → implementing sim/render entry point, each link unique.

This also dictates what `litd/api` is allowed to contain: translation and ergonomics only. If a
method needs a loop, a conditional on game rules, or state, that logic belongs in `litd/sim`
(canonical) or `litd/api/helpers` (D4 convenience), never inline in the API layer where it could
fork behavior between callers.

## 6. The Lua binding layer (v1, M5)

*Revised 2026-06-11 per D-2026-06-11-8/20 — promoted from "v2 candidate" to committed v1
scope, shipping with M5. Revised again 2026-06-11 per D-2026-06-11-25: the VM is concretely
decided, no longer a "family" candidate.*

[PRD §5.6](../../PRD.md#56-moddingscripting) commits a deterministic Lua VM — a **vendored
fork of [yuin/gopher-lua](https://github.com/yuin/gopher-lua)** (D-2026-06-11-25;
LITD-PATCH discipline like the g3n fork, with four patches: instruction-budget hook in
`mainLoop`, deterministic mathlib replacement, coroutine/LState persister, LState pooling +
golden cross-arch CI test — see [Determinism §2.6](../04-simulation/determinism.md) and
[Tooling §6](../09-roadmap/tooling.md)) — as the v1 runtime-loadable creation surface,
delivered in M5 alongside the Go API. The architecture
makes this binding a *mechanical projection* of `litd/api`, not a second API — and the
**sandbox boundary (R-SEC-1)** is part of the layering itself:

```
┌────────────────────────────────────────────────────┐
│ world.lua (runtime-loaded, from a world archive)   │  v1 (M5) — UNTRUSTED
├────────────────────────────────────────────────────┤  ← sandbox boundary (R-SEC-1):
│ litd/luabind  — generated bindings + hard sandbox  │  v1 (M5)   no io/os/net, game API only,
├────────────────────────────────────────────────────┤            per-tick instr+mem quotas
│ litd/api      — canonical Go API (unchanged)       │  v1 (M5)
├────────────────────────────────────────────────────┤
│ litd/sim · litd/render · litd/asset                │  v1
└────────────────────────────────────────────────────┘
```

Everything above the sandbox boundary is untrusted world content; everything below is
engine. World Lua cannot touch the player's machine (D-2026-06-11-20): `litd/luabind`
exposes the game API and nothing else, and enforces per-tick instruction and memory quotas
that double as the M7 lockstep stall guard
([Execution model §7](execution-model.md#7-the-lua-execution-surface-d-2026-06-11-8-r-sec-1)).
Any world-sharing feature (disk-loaded worlds from M5, the M9 hub) is hard-gated on this
sandbox.

Design commitments that make the binding mechanical:

1. **Generated, not hand-written.** The `api-manifest.json` produced by `tools/jassgen`
   ([PRD R-AST-4](../../PRD.md#6-asset--data-pipeline)) already records every canonical Go symbol
   with its full signature. The Lua binding generator consumes the same manifest, so the Lua
   surface is the Go surface by construction — one capability inventory, two language skins.
   This is why R-API-3's options structs and R-API-2's value types matter beyond Go ergonomics:
   plain structs and value math marshal into Lua tables trivially, whereas leaked G3N types,
   channels, or goroutine-coupled objects would not.
2. **Scheduler reuse.** Lua coroutines map one-to-one onto the deterministic cooperative
   scheduler ([Execution model §2](execution-model.md#2-the-deterministic-cooperative-scheduler)
   — stackless descriptive suspension, decided and spike-validated per D-2026-06-11-28).
   A Lua script that calls `PolledWait` suspends as a scheduler job exactly like a Go handler
   closure does; resume order rules are shared, so a mixed Go/Lua map remains deterministic.
3. **Sandboxing at the binding, not the core (R-SEC-1).** `litd/luabind` strips Lua's `os`,
   `io`, networking, and nondeterministic `math.random` (replaced by the sim PRNG), and meters
   each context against per-tick instruction and memory quotas — the same isolation philosophy
   as the AI domain ([Execution model §6](execution-model.md#6-ai-domain-isolation)). `litd/api`
   itself needs no changes because it already exposes no ambient authority: every capability
   hangs off the `Game` object you were given.
4. **No version skew.** The Lua binding versions with the Go API
   ([Naming & style §4](naming-and-style.md#4-versioning-and-stability-policy)); a world declares
   the API version it targets, and the manifest's tombstone records double as the Lua
   deprecation table.

M5's exit criterion makes this concrete: Lua VM embedded, bindings generated from
`api-manifest.json`, hard sandbox in place, worlds runtime-loadable
([PRD §7](../../PRD.md#7-milestones)). Go remains the systems language; Lua is the creation
surface — creators and AI coding agents author worlds without a Go toolchain or recompile.

## 7. World archives: the content unit (D-2026-06-11-14)

*Added 2026-06-11 per D-2026-06-11-14.*

The unit of content the api/asset layers load is the **world archive**: a single zip-based
file containing map data, Lua scripts, custom assets, and a manifest with content hashes and
engine-version requirements. The format is defined in v1 (M6) and documented publicly; it
carries hosting metadata from day one so the committed M9 hosted hub (D-2026-06-11-23) needs no format change.

How it maps onto the layering:

- **`litd/asset`** opens and validates the archive (manifest hashes, engine-version check,
  asset validation per R-AST-2) and routes its contents along the existing split: gameplay
  data tables → `litd/sim`, meshes/audio → `litd/render`. A tampered or mismatched archive
  fails loudly at load, never mid-match.
- **`litd/api`** (via `litd/luabind`) receives the archive's Lua entry point and runs it
  inside the §6 sandbox — the archive is precisely the untrusted content the sandbox boundary
  exists for.
- The M8 World Editor saves to this same format; content hashes make archives
  lockstep-safe (all M7 clients verify they loaded byte-identical world content before the
  first tick).

## 8. Acceptance criteria for this section

- CI import lint (IMP-1…IMP-5) green from M0.
- `go build -tags litd_headless ./...` succeeds with no cgo on a GPU-less runner from M0.
- Headless-vs-rendered state-hash equivalence test green from M3.
- Zero G3N types in `litd/api` signatures, verified by the API-surface lint from M2.
- Lua bindings regenerate from `api-manifest.json` with zero hand-edits; sandbox-escape and
  quota-breach test suites (R-SEC-1) green from M5. *Added 2026-06-11 per D-2026-06-11-8/20.*
- World-archive round trip (build → hash-verify → load headless → state hash matches a
  loose-files load of the same content) green from M6. *Added 2026-06-11 per D-2026-06-11-14.*
