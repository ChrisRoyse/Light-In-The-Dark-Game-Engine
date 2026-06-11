# Goals and Non-Goals — Expanded with Success Criteria

> Expands [PRD §2 (Goals and Non-Goals)](../../PRD.md#2-goals-and-non-goals). Each goal
> gains measurable success criteria and a verification method. Criteria reference the
> performance budgets of [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
> and the milestones in [Milestones](../09-roadmap/milestones.md); nothing here loosens or
> contradicts the PRD.

---

## 1. Goals

### G1 — Full API power, zero duplication

**Statement.** Port the complete WC3 API capability surface to Go. Every duplicated
native/BJ pair collapses into exactly one canonical function — the most general ("complex")
version — so no capability is lost and no code is repeated.

**What "complete" means.** The unit of accounting is the 2,521-function source surface:
1,536 `common.j` natives + 985 `blizzard.j` BJ functions (see the
[source-material clarification](./overview.md#5-source-material-the-componentsjson--blizzardjson-clarification)).
Every one of them is classified D1–D5 per the
[deduplication policy](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule)
and either mapped to exactly one canonical Go symbol or explicitly tombstoned with a
machine-readable reason (`deprecated`, `gameplay-irrelevant`, `superseded`, `deferred-v2`).

**Success criteria.**

| # | Criterion | Measured by | Gate |
|---|---|---|---|
| G1.1 | 100% of the 2,521 source functions carry a D1–D5 classification in `api-manifest.json` | `jassgen` audit report: `unclassified == 0` | M2 |
| G1.2 | Every classified function has exactly one canonical Go mapping **or** a tombstone with reason | Audit report: `unmapped == 0` | M2 (spec), M5 (implemented) |
| G1.3 | No canonical Go symbol is the target of two semantically distinct source functions unless the manifest marks them as a deliberate collapse (D1/D3/D5) | Audit report duplicate-target check | M5 |
| G1.4 | Zero capabilities silently dropped: every tombstone reason is one of the enumerated values and is human-reviewed | Audit report + PR review of tombstone diffs | M5 |
| G1.5 | D4 helpers (`litd/api/helpers`) never shadow a canonical core function | `jassgen` lint: helper names disjoint from core symbol set | M5 |

**Verification method.** The audit report is generated, not hand-maintained
([Tooling §2](../09-roadmap/tooling.md)); CI fails if regeneration changes counts.

### G2 — Smallest possible public API

**Statement.** The public layer must be the smoothest, least intrusive API achievable:
idiomatic Go, small interface count, options structs instead of parameter explosions, no
leaked internals.

*Revised 2026-06-11 per D-2026-06-11-8: the canonical surface is now exposed twice — idiomatic
Go and embedded Lua (M5) — but counted once. Lua bindings are **generated from
`api-manifest.json`**, never hand-written, so the Lua surface adds no independently
maintained symbols and cannot drift from the Go surface.*

**Success criteria.**

| # | Criterion | Measured by | Gate |
|---|---|---|---|
| G2.1 | Public noun types in `litd/api`: target ~20, hard ceiling 30 (excluding option/enum/value types) | `go doc` extraction script counted in CI | M5 |
| G2.2 | Zero G3N types in any exported `litd/api` signature (R-API-6) | Static check: import graph + exported-signature scan | M5 |
| G2.3 | No exported function with > 5 positional parameters; long tails go through options structs (R-API-3) | API lint in CI | M2 spec, M5 code |
| G2.4 | Methods on nouns only: zero exported free functions taking a handle-like first parameter (R-API-1) | API lint | M5 |
| G2.5 | Trigger/condition/filter object zoo fully replaced: no exported `Trigger`, `BoolExpr`, `FilterFunc` analogues; events are `OnEvent` + closures (R-API-4) | API spec review + manifest mapping check | M2/M5 |
| G2.6 | A WC3 modder can find the Go equivalent of any JASS function in one lookup | Generated JASS→Go mapping table published with docs; 100% coverage of non-tombstoned entries | M5 |
| G2.7 | Lua surface parity without drift: every canonical symbol reachable from Lua via bindings generated from `api-manifest.json`; zero hand-written binding code (D-2026-06-11-8) | `jassgen` binding-generator output; CI regeneration reproducibility check | M5 |

### G3 — Low-tier hardware performance

**Statement.** Smooth gameplay (60 FPS render, fixed 20 Hz simulation tick) on integrated
GPUs (Intel UHD-class) and 4 GB RAM machines.

**Success criteria.** These are exactly the [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
budgets, restated as CI gates with their enforcement mechanism (see
[Tooling §4](../09-roadmap/tooling.md) for the benchmark harness):

| # | Criterion | Budget | Measured by | Gate |
|---|---|---|---|---|
| G3.1 | Render frame rate, typical scene | ≥ 60 FPS | Scripted render benchmark scene on reference machine | M4 onward |
| G3.2 | Render frame rate, worst case (500 units on screen) | ≥ 30 FPS | Same harness, max-army scene | M4/M6 |
| G3.3 | Sim tick at 20 Hz, worst case (500 units + 500 projectiles) | ≤ 10 ms (50% headroom) | Headless sim benchmark in CI | M3 onward |
| G3.4 | Cold start to main menu | ≤ 5 s | Timed launch in benchmark harness | M6 |
| G3.5 | Map load (128×128) | ≤ 10 s | Benchmark harness | M6 |
| G3.6 | RAM, full match | ≤ 1.5 GB | RSS sampling during benchmark match | M6 |
| G3.7 | Binary + base assets | ≤ 300 MB | Release artifact size check in CI | M6 |
| G3.8 | Steady-state allocations per sim tick and per render frame | 0 (R-GC-1) | `testing.AllocsPerRun` CI benchmarks; regression above zero baseline fails the build (R-GC-5) | M3 onward |
| G3.9 | Draw calls per frame at max army size | ≤ 300 (R-RND-3) | Render-stats counter asserted in benchmark scene | M4 onward |
| G3.10 | Stretch target (recommended-spec machine, not the low-tier reference): 1,000 units + 1,000 projectiles within the same tick and draw-call budgets; ECS capacities, pathfinding, and budgets provisioned for this scale from M3; render side assumes the instancing patch lands in M4 (D-2026-06-11-18) | Headless + render benchmark stretch scenes tracked in CI; recommended-spec pass at M6 | M3 onward (capacities), M4/M6 (render) |

*Revised 2026-06-11 per D-2026-06-11-18: G3.10 added. The 500-unit rows (G3.2/G3.3) remain
the low-tier reference-machine **gates**; 1,000 units is the recommended-spec stretch
target, never traded against them.*

**Verification method.** Budgets are CI-enforced from M3 onward (headless) and M4 onward
(render). "Passes on the developer's machine" is not acceptance; the reference machine
profile is the contract.

### G4 — Zero-cost asset and tech stack

**Statement.** All runtime dependencies open source (BSD/MIT/Apache); all game models
CC0-licensed. No proprietary MDX/MDL Blizzard formats, no paid middleware, no runtime AI
inference.

**Success criteria.**

| # | Criterion | Measured by | Gate |
|---|---|---|---|
| G4.1 | 100% of `go.mod` dependencies (transitive) carry BSD/MIT/Apache-family licenses | License scan in CI (e.g. `go-licenses`-style check) with allowlist | M0 onward |
| G4.2 | 100% of files under `assets/` have a recorded CC0 (or equivalently free, commercial-OK) provenance entry | `assets/MANIFEST` provenance file checked by the asset-validation CLI | M0 onward |
| G4.3 | Zero MDX/MDL or other Blizzard-format files in the repo or the asset pipeline | Asset-validation CLI rejects non-GLB model formats (R-FMT-1/R-AST-2) | M0 onward |
| G4.4 | All assets pass core-glTF validation (no unsupported KHR extensions; `KHR_materials_unlit` excepted) | Asset-validation CLI ([Tooling §3](../09-roadmap/tooling.md)) | M0 onward |
| G4.5 | Total cash cost of runtime stack and shipped assets | $0 — no paid middleware, fonts, codecs (`.ogg` only, R-AUD-1) | Continuous |
| G4.6 | No runtime AI/ML inference anywhere in the engine | Code review policy; no inference deps possible under G4.1 allowlist | Continuous |
| G4.7 | Generated assets carry full provenance (D-2026-06-11-12): every `tools/assetgen` output is produced at **asset-build time only**, hand-curated, and committed with a provenance entry recording generator, parameters, and curation sign-off; G4.6 intact — zero runtime inference | Provenance manifest check extended to generated assets (CI); assetgen run log | Continuous from first generated asset (M4 terrain textures) |

*Revised 2026-06-11 per D-2026-06-11-12: G4.7 added — asset categories with no CC0 source
(portraits, spell VFX textures, voice lines, UI icons, terrain splat/cliff sets) are filled
by the build-time generative pipeline rather than cut.*

### G5 — Determinism

**Statement.** Identical inputs produce identical simulation states across runs
(prerequisite for replays, lockstep multiplayer, and testing).

**Success criteria.**

| # | Criterion | Measured by | Gate |
|---|---|---|---|
| G5.1 | 10,000-tick scripted sim run reproduces an identical state hash across ≥ 100 consecutive runs on one machine | M1 spike harness, kept as a permanent CI test | M1 |
| G5.2 | Same state hash across OS/CPU matrix (Windows/Linux/macOS; amd64/arm64) for the same scripted run | CI matrix job comparing hashes | M1 (spike), M3 (full sim) |
| G5.3 | Replay verification: a recorded command stream replayed headlessly reaches the same final hash as the original session | Headless replay test (R-SIM-4) | M3 onward |
| G5.4 | Zero nondeterminism sources in gameplay code: no `map` iteration, no wall-clock reads, no free-running goroutines inside a tick (R-SIM-2, R-EXEC-1) | Static lint ruleset over `litd/sim` + code review | M3 onward |
| G5.5 | Event handlers fire in deterministic registration order; waits quantize to ticks (R-EXEC-2, R-EXEC-5) | Scheduler unit tests with order assertions | M3/M5 |
| G5.6 | Sim math strategy (fixed-point vs ordered-float) decided and documented before any gameplay system is written | M1 decision record (see [Risks and Open Questions §2.1](./risks-and-open-questions.md)) | M1 |
| G5.7 | Lua surface determinism (D-2026-06-11-8): a Lua-scripted scenario passes the same cross-platform hash matrix as G5.2; the sandbox (R-SEC-1, D-2026-06-11-20) exposes only the generated game API — no `os`/`io`/stdlib nondeterminism reachable — and its per-tick instruction + memory quotas are themselves deterministic (they double as the lockstep stall guard) | CI matrix job with Lua-driven run; sandbox-surface lint; quota enforcement tests (see [R7](./risks-and-open-questions.md#r7--lua-vm-determinism-added-2026-06-11-per-d-2026-06-11-8)) | M5 |

*Revised 2026-06-11 per D-2026-06-11-8/20: G5.7 added — the embedded Lua VM sits inside the
deterministic boundary and is held to the same evidence standard as the Go sim.*

---

## 2. Non-Goals (v1) — expanded

Non-goals carry success criteria too: the measurable form of a non-goal is that the
excluded thing demonstrably is not in the v1 tree.

### NG1 — No Warcraft III content recreation

Only the *API shape* is ported — JASS function signatures are facts about an interface, not
copyrightable expression. All function bodies are re-implemented from scratch; all data and
assets are original or CC0.

- **Criteria:** zero Blizzard-derived art, sound, balance tables, campaign text, or map
  files anywhere in the repo (G4.2/G4.3 scans double as enforcement); no code copied from
  leaked or decompiled WC3 sources — implementations derive from public interface
  documentation only; identifier names follow Go conventions, with the JASS→Go table as the
  bridge rather than name-cloning where it would imply content copying.

### NG2 — No MDX/MDL model loading

Proprietary Blizzard formats are explicitly out, permanently for v1 and presumptively forever.

- **Criteria:** the asset pipeline accepts `.glb` only (R-FMT-1); no MDX/MDL parsing code
  exists in the tree; the validation CLI has no bypass flag for model format.

### NG3 — No mobile/console targets

Desktop Windows/Linux/macOS only — G3N's supported platforms.

- **Criteria:** CI builds and tests exactly the three desktop targets; no mobile/console
  build tags, input abstractions, or platform shims accepted into v1; reference-machine
  budgets are never traded off to enable a hypothetical port.

### NG4 — No netcode before M7 (multiplayer itself is committed)

*Revised 2026-06-11 per decision D-2026-06-11-5: multiplayer is a committed lockstep
milestone (M7), not a v2 maybe.* Milestones M0–M6 still ship no transport/lobby/sync code —
but determinism (G5) and the command stream are now load-bearing prerequisites for a
committed feature.

- **Criteria:** no network transport, lobby, or sync code in v1; the command-stream
  abstraction (ordered commands in → deterministic state out, G5.3) exists and is tested,
  because it is also the replay mechanism; lockstep readiness is demonstrated by replay
  verification (G5.3), which is a hard M6 exit criterion gating M7; from M3 onward, sim
  changes must answer "does this survive lockstep?" (no per-client state or local-player
  branches inside the tick).

### NG5 — Not a general-purpose engine

This is an RTS-shaped engine; it does not compete with Unity/Godot.

*Checked 2026-06-11 against D-2026-06-11-10: the M8 World Editor does not erode this
non-goal — it is RTS-shaped authoring tooling for LitD worlds (terrain sculpt, unit/doodad
placement, world-archive save), not general-purpose engine tooling. The criteria below are
unchanged.*

- **Criteria:** feature requests outside the RTS shape (free cameras, general rigid-body
  physics, non-RTS genre scaffolding) are declined by default in triage; G3N's experimental
  physics is not wired in (R-SIM note, [PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n));
  the public type ceiling (G2.1) acts as a structural brake on scope creep.

---

## 3. Goal interactions and precedence

When goals tension against each other, precedence for v1 decisions is:

1. **G5 (determinism)** — a fast or pretty engine that desyncs is worthless for the product
   thesis; determinism constraints (R-EXEC-1, R-SIM-2) override convenience everywhere.
2. **G1 (full power, zero duplication)** — capability loss is never an acceptable
   simplification; if G2 minimalism would drop power, the function stays (as the complex
   canonical form, per D2/D3).
3. **G3 (low-tier performance)** — budgets are gates; features that bust them get redesigned
   (e.g. instancing patch vs more draw calls, [PRD §8](../../PRD.md#8-risks)).
4. **G2 (smallest API)** — minimality yields to the three above but to nothing else;
   "one more convenience wrapper" is how 985 BJ functions happened.
5. **G4 (zero cost)** — rarely in tension; when it is (no Draco compression because G3N
   can't decode it, R-FMT-3), the free-and-supported option wins over the optimal-but-costly one.

See [Risks and Open Questions](./risks-and-open-questions.md) for where these tensions are
expected to surface, and [Milestones](../09-roadmap/milestones.md) for when each criterion
becomes a hard gate.
