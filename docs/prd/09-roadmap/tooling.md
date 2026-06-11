# Roadmap — Tooling Specifications

> Specifies the four tools the PRD requires: the `tools/jassgen` parser/generator
> (R-AST-4, [PRD §6](../../PRD.md#6-asset--data-pipeline)), the asset-validation CLI
> (R-FMT-2, R-AST-2/3), the CI benchmark harness that enforces the
> [§5.3 budgets](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
> and GC discipline (R-GC-1…5), and the `tools/assetgen` generative asset pipeline
> (R-AST-5, D-2026-06-11-12). Milestone ownership per [Milestones](./milestones.md);
> deduplication policy per [PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule).
>
> *Revised 2026-06-11 per D-2026-06-11-6/8/12: `jassgen` gains the Lua binding generator as
> a fifth output; `commonai` natives are mapped canonically, no longer tombstoned
> `deferred-v2`; `tools/assetgen` added (§5).*
>
> *Revised 2026-06-11 per D-2026-06-11-21/25: the vendored **gopher-lua fork** joins the
> tooling/vendoring landscape (§6) with its four LITD patches; the license scan rule is
> tightened to a permissive-only allowlist with copyleft hard-excluded (§1).*

---

## 1. Tooling principles

- **Generated, never hand-edited.** `api-manifest.json`, the audit report, API stubs, and
  the JASS→Go mapping table are build outputs. Human judgment enters through a reviewed
  overrides file, not by editing outputs.
- **Reproducible.** Re-running any tool on the same vendored inputs yields byte-identical
  outputs (stable ordering, no timestamps in payloads); CI verifies this on every run.
- **Gates, not reports.** Every tool has a non-zero exit mode wired into CI; a tool that
  only prints warnings does not protect a budget.
- **Plain Go, zero exotic deps.** Tools live under `tools/`, build with the ordinary
  toolchain, and respect the G4 license allowlist.
- **License scan: permissive-only allowlist, copyleft hard-excluded** *(revised 2026-06-11
  per D-2026-06-11-21)*. The G4.1 CI scan is an **allowlist** (BSD/MIT/Apache family), not a
  blocklist; GPL/AGPL/LGPL is a hard exclusion anywhere in the tree — dependencies, tools,
  and vendored forks alike. The engine is proprietary permanently (D-21), so a single
  copyleft dependency is a shipping blocker, and the scan has no waiver path for it.

---

## 2. `tools/jassgen` — JASS API parser and generator

### 2.1 Purpose

`jassgen` turns the vendored source material into the machine-readable manifest the whole
API program runs on. It is the realization of the requested "components/blizzard JSON" —
which [do not exist upstream](../01-vision/overview.md#5-source-material-the-componentsjson--blizzardjson-clarification) —
generated from the files that do:

| Input | Path | Role |
|---|---|---|
| `common.j` | `repoes/war3-types/scripts/common.j` | 1,536 native declarations (authoritative for natives) |
| `blizzard.j` | `repoes/war3-types/scripts/blizzard.j` | 985 BJ functions, **with bodies** — bodies drive D1/D2 detection |
| `common.d.ts` | `repoes/war3-types/core/common.d.ts` | Type enrichment: handle subtype hierarchy, nullability |
| `blizzard.d.ts` | `repoes/war3-types/core/blizzard.d.ts` | Type enrichment for BJ signatures |
| `commonai.d.ts` | `repoes/war3-types/core/commonai.d.ts` | AI natives — **mapped canonically** into the AI-domain surface (M5.5) per [D-2026-06-11-6](../01-vision/decisions.md#d-2026-06-11-6--commonai-full-v1-port-supersedes-d-2026-06-11-4); *revised 2026-06-11 — previously tombstoned `deferred-v2` per Q4* |
| `overrides.toml` | `tools/jassgen/overrides.toml` | Hand-reviewed classification/mapping decisions (in git, code-reviewed) |

### 2.2 Pipeline

```
.j / .d.ts ──► parse ──► merge/enrich ──► classify (D1–D5) ──► apply overrides
                                                                    │
        ┌──────────────────┬──────────────────┬─────────────────┬───┴────────────┐
        ▼                  ▼                  ▼                 ▼                ▼
api-manifest.json    audit report       Go API stubs      JASS→Go table    Lua bindings
                  (md + machine JSON)    (litd/api)          (docs)        (D-8, M5)
```

1. **Parse.** Two small recursive-descent parsers: one for JASS declarations
   (`native`/`function`/`constant`, parameter lists, `takes ... returns ...`) including
   `blizzard.j` function *bodies*; one for the `.d.ts` declaration subset. No general
   TypeScript compiler — the declaration files are mechanical.
2. **Merge/enrich.** Join `.j` and `.d.ts` entries by name; record discrepancies (the known
   1,536 vs 1,534 delta between `common.j` and `common.d.ts` must be explained entry by
   entry in the audit report, not papered over).
3. **Classify.** Mechanical heuristics propose a D1–D5 class per
   [PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule):
   - **D1** if a BJ body is a single `call`/`return` of one native with arguments passed
     through unmodified.
   - **D2** if the body is a single native call with reordered/constant-defaulted arguments.
   - **D3** proposed for native families matched by signature/name patterns
     (`...Loc` variants, X/Y/position splits) — always confirmed via overrides.
   - **D4** if the BJ body contains real control flow or state.
   - **D5** for getter/setter state-key families (`GetUnitState`/`SetUnitState` style).
4. **Apply overrides.** `overrides.toml` wins over heuristics; every override carries a
   `reason` string. Tombstones (`deprecated`, `gameplay-irrelevant`, `superseded`,
   `deferred-v2`) can only come from overrides — the tool never tombstones on its own
   (G1.4's human-review rule).
5. **Emit.** Five outputs, all deterministic *(revised 2026-06-11 per D-2026-06-11-8: Lua
   bindings added)*: the manifest, the audit report (markdown for humans + JSON for CI
   gates), compiling panic-body stubs for canonical symbols (M2 deliverable), the JASS→Go
   mapping table for the docs (G2.6), and the **Lua bindings** for the embedded VM (M5
   deliverable) — generated from the same manifest entries as the Go surface, so the two
   surfaces cannot drift (G2.7); zero hand-written binding code.

### 2.3 Audit report and CI gates

The audit report is the acceptance instrument for
[G1](../01-vision/goals-and-non-goals.md#g1--full-api-power-zero-duplication). Machine-readable
counters, each a CI gate:

| Counter | Gate (from milestone) |
|---|---|
| `total` (must equal 2,521 for `common.j`+`blizzard.j`) | M2 |
| `unclassified` == 0 | M2 |
| `unmapped` == 0 (every entry mapped or tombstoned) | M2 |
| `duplicateTargets` == 0 (two sources → one Go symbol only when marked as deliberate D1/D3/D5 collapse) | M5 |
| `helperShadowsCore` == 0 (G1.5) | M5 |
| `unimplemented` == 0 (stub bodies remaining) | M5 (core), M5.5 (`commonai` origin) |
| `commonai` capability tombstones == 0 — all AI natives mapped canonically (D-2026-06-11-6; *revised 2026-06-11, replaces the blanket `deferred-v2` plan*) | M2 |
| `featureTags` rollup (the [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks) engine-feature census) | reviewed at M2, gates M4 planning |

Additionally, CI re-runs `jassgen` and fails if any output differs from the committed
version (reproducibility gate, M2 onward).

### 2.4 `api-manifest.json` schema

Proposed JSON Schema (draft 2020-12), abbreviated to the load-bearing parts. *Revised
2026-06-11 per D-2026-06-11-6: `goMapping.package` gains `litd/ai` — the AI-domain surface
implemented at M5.5 (package name provisional) — since `commonai` natives now map
canonically:*

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "litd:api-manifest",
  "type": "object",
  "required": ["schemaVersion", "sources", "functions"],
  "properties": {
    "schemaVersion": { "const": 1 },
    "sources": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["file", "sha256", "declCount"],
        "properties": {
          "file": { "type": "string" },
          "sha256": { "type": "string" },
          "declCount": { "type": "integer" }
        }
      }
    },
    "functions": {
      "type": "array",
      "items": { "$ref": "#/$defs/functionEntry" }
    }
  },
  "$defs": {
    "functionEntry": {
      "type": "object",
      "required": ["name", "origin", "signature", "classification", "disposition"],
      "properties": {
        "name": { "type": "string" },
        "origin": { "enum": ["common.j", "blizzard.j", "commonai"] },
        "signature": {
          "type": "object",
          "required": ["params", "returns"],
          "properties": {
            "params": {
              "type": "array",
              "items": {
                "type": "object",
                "required": ["name", "jassType"],
                "properties": {
                  "name": { "type": "string" },
                  "jassType": { "type": "string" },
                  "tsType": { "type": "string" }
                }
              }
            },
            "returns": { "type": "string" }
          }
        },
        "classification": { "enum": ["D1", "D2", "D3", "D4", "D5"] },
        "classifiedBy": { "enum": ["heuristic", "override"] },
        "disposition": { "enum": ["mapped", "tombstoned"] },
        "goMapping": {
          "type": "object",
          "required": ["symbol", "package"],
          "properties": {
            "symbol": { "type": "string" },
            "package": { "enum": ["litd/api", "litd/api/helpers", "litd/ai"] },
            "collapsesWith": { "type": "array", "items": { "type": "string" } },
            "notes": { "type": "string" }
          }
        },
        "tombstone": {
          "type": "object",
          "required": ["reason", "detail"],
          "properties": {
            "reason": { "enum": ["deprecated", "gameplay-irrelevant", "superseded", "deferred-v2"] },
            "detail": { "type": "string" }
          }
        },
        "featureTags": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Engine capabilities implied (fog-of-war, minimap, weather, cinematics, ...)"
        }
      },
      "allOf": [
        { "if": { "properties": { "disposition": { "const": "mapped" } } },
          "then": { "required": ["goMapping"] } },
        { "if": { "properties": { "disposition": { "const": "tombstoned" } } },
          "then": { "required": ["tombstone"] } }
      ]
    }
  }
}
```

### 2.5 Example entries (one per classification)

```jsonc
// D1 — pure passthrough BJ: dropped, native canonical
{ "name": "KillUnitBJ", "origin": "blizzard.j",
  "signature": { "params": [{ "name": "whichUnit", "jassType": "unit" }], "returns": "nothing" },
  "classification": "D1", "classifiedBy": "heuristic",
  "disposition": "mapped",
  "goMapping": { "symbol": "Unit.Kill", "package": "litd/api",
                 "collapsesWith": ["KillUnit"] } },

// D2 — BJ reorders/defaults params: canonical takes the full set
{ "name": "CreateUnitAtLocSaveLast", "origin": "blizzard.j",
  "signature": { "params": [
      { "name": "id", "jassType": "player" }, { "name": "unitid", "jassType": "integer" },
      { "name": "loc", "jassType": "location" }, { "name": "face", "jassType": "real" }],
    "returns": "unit" },
  "classification": "D2", "classifiedBy": "heuristic",
  "disposition": "mapped",
  "goMapping": { "symbol": "Game.CreateUnit", "package": "litd/api",
                 "notes": "owner, type, Vec2 pos, facing; options struct for tail params" } },

// D3 — native family collapsed onto the most general form
{ "name": "SetUnitPositionLoc", "origin": "common.j",
  "signature": { "params": [
      { "name": "whichUnit", "jassType": "unit" }, { "name": "whichLocation", "jassType": "location" }],
    "returns": "nothing" },
  "classification": "D3", "classifiedBy": "override",
  "disposition": "mapped",
  "goMapping": { "symbol": "Unit.SetPosition", "package": "litd/api",
                 "collapsesWith": ["SetUnitX", "SetUnitY", "SetUnitPosition"],
                 "notes": "location heap variants collapse into value-type Vec2 (R-API-2)" } },

// D4 — BJ with real logic: kept once, in helpers
{ "name": "PolledWait", "origin": "blizzard.j",
  "signature": { "params": [{ "name": "duration", "jassType": "real" }], "returns": "nothing" },
  "classification": "D4", "classifiedBy": "heuristic",
  "disposition": "mapped",
  "goMapping": { "symbol": "helpers.PolledWait", "package": "litd/api/helpers",
                 "notes": "tick-quantized per R-EXEC-5; suspends onto sim scheduler" } },

// D5 — state getter/setter collapsed onto typed accessors
{ "name": "GetUnitState", "origin": "common.j",
  "signature": { "params": [
      { "name": "whichUnit", "jassType": "unit" }, { "name": "whichUnitState", "jassType": "unitstate" }],
    "returns": "real" },
  "classification": "D5", "classifiedBy": "override",
  "disposition": "mapped",
  "goMapping": { "symbol": "Unit.Life", "package": "litd/api",
                 "collapsesWith": ["SetUnitState", "GetUnitStateSwap", "SetUnitLifeBJ", "SetUnitManaBJ"],
                 "notes": "typed accessors Life/SetLife/Mana/SetMana...; one state table behind them" } },

// commonai — mapped canonically per D-2026-06-11-6 (revised 2026-06-11: this entry was
// previously the deferred-v2 tombstone example; capability tombstones for commonai are gone.
// Symbol name illustrative; the AI-domain surface is specced at M2, implemented at M5.5.)
{ "name": "StartUnitAbilityOrder", "origin": "commonai",
  "signature": { "params": [], "returns": "boolean" },
  "classification": "D3", "classifiedBy": "override",
  "disposition": "mapped",
  "goMapping": { "symbol": "AIPlayer.OrderAbility", "package": "litd/ai",
                 "notes": "AI domain (M5.5): isolated scheduler domain, command-stack messaging (R-EXEC-3)" } },

// Tombstoned example — reasons remain for genuinely dead surface, never for capability
{ "name": "DoNothing", "origin": "blizzard.j",
  "signature": { "params": [], "returns": "nothing" },
  "classification": "D1", "classifiedBy": "override",
  "disposition": "tombstoned",
  "tombstone": { "reason": "gameplay-irrelevant",
                 "detail": "empty placeholder callback; Go closures need no no-op sentinel" } }
```

---

## 3. Asset-validation CLI (`tools/assetcheck`)

### 3.1 Purpose

Build-time gate (never runtime, per R-FMT-2) ensuring every file entering `assets/`
satisfies the format, budget, and licensing rules. Runs locally (`go run ./tools/assetcheck ./assets`)
and as a required CI job from M0.

### 3.2 Checks

| Check | Rule | Source |
|---|---|---|
| Format | Models are `.glb` only; reject MDX/MDL/FBX/OBJ/DAE in `assets/` | R-FMT-1, NG2 |
| glTF profile | Core glTF 2.0; only `KHR_materials_unlit` permitted in `extensionsUsed`; any other KHR extension → fail | R-FMT-1, [PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only) |
| Compression | No Draco/Meshopt buffers (G3N cannot decode) | R-FMT-3 |
| Triangle budget | Units ≤ 1,500 tris; buildings ≤ 4,000 (category from the asset manifest entry) | R-RND-2 |
| Texture/atlas | Texture count and size against the one-shared-atlas pattern (≤ 1024×1024 per faction/biome atlas); flag per-model unique textures | R-RND-2 |
| Animation clips | Unit models must contain `Idle`, `Walk`, `Attack`, `Death` (optional `Spell`, `Portrait`); names exact | R-AST-3 |
| Audio | `.ogg` only | R-AUD-1 |
| Provenance | Every asset has a `assets/MANIFEST` entry with pack, URL, and license (CC0 or free-commercial); unlisted file → fail | G4.2 |

### 3.3 Behavior

- Output: one line per finding (`path: RULE-ID: message`), summary table, exit non-zero on
  any failure. `--json` mode for CI annotation.
- `--ingest` mode for M0 pack ingestion: produces the extension/clip census used as the
  [R1 detection signal](../01-vision/risks-and-open-questions.md#r1--g3n-gltf-gaps-skinning-edge-cases-extensions).
- No bypass flags for format or license rules; budget rules accept a per-asset waiver only
  via a reviewed `waivers.toml` with reason and expiry milestone.

---

## 4. CI benchmark harness

### 4.1 Purpose

Turns the [§5.3 budgets](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
and GC rules into failing builds. Three suites, introduced at the milestones where their
subject exists (see the [gates summary](./milestones.md#14-cross-milestone-gates-summary)).

### 4.2 Suites

**Headless sim suite (from M3).** Runs on every CI machine — deterministic, GPU-free.
- Scenario: scripted 500-unit + 500-projectile sustained combat on a 128×128 map; a
  1,000-unit + 1,000-projectile stretch scenario runs alongside and is tracked, not gated
  (D-2026-06-11-18 — 500 remains the low-tier gate).
- Metrics: worst-case and p99 tick time (budget ≤ 10 ms, asserted with a CI-machine
  calibration factor; the reference-machine number is authoritative), `testing.AllocsPerRun`
  on the tick path (budget: 0; R-GC-1/5), state-hash determinism across the OS/arch matrix
  (G5.1–G5.3).

**Render suite (from M4).** Requires a GPU runner; the authoritative pass is a periodic run
on the physical reference machine (dual-core 2 GHz, UHD 620, 4 GB RAM), with a per-commit
proxy run on a capped CI GPU runner.
- Scenes: "typical" (mixed economy + small battle), "worst case" (500 units on screen), and
  the 1,000-unit stretch scene (D-2026-06-11-18; measured on the recommended spec, tracked
  not gated).
- Metrics: FPS (≥ 60 typical / ≥ 30 worst), draw calls (≤ 300; early-warning at 250 per
  [R2](../01-vision/risks-and-open-questions.md#r2--no-gpu-instancing-in-g3n--draw-call-ceiling)),
  allocs/frame (0), active VFX light count (≤ 8; R-RND-4).

**End-to-end suite (from M6).** Full product run on the reference machine.
- Metrics: cold start ≤ 5 s, 128×128 map load ≤ 10 s, full-match RSS ≤ 1.5 GB, release
  artifact (binary + base assets) ≤ 300 MB, full-match replay hash verification.

### 4.3 Mechanics

- Budgets live in one `benchmarks/budgets.toml` file — the single place a budget number
  exists; suites read it, docs reference it.
- Every run appends to a results history (JSON lines, stored as CI artifacts) so trend
  signals — the early-warning detection for R2/R3 — are queryable, not anecdotal.
- Regression policy per R-GC-5: any allocs/tick or allocs/frame above zero fails; any
  budgeted metric exceeding its budget fails; metrics within budget but trending ≥ 10%
  worse over a rolling window raise a non-blocking warning for triage.
- Reference-machine runs are scheduled (nightly + milestone close) and post results to the
  same history; a milestone cannot close on proxy-runner numbers alone.

---

## 5. Generative asset pipeline (`tools/assetgen`) *(added 2026-06-11 per D-2026-06-11-12, R-AST-5)*

### 5.1 Purpose

Fills the asset categories with no CC0 source — hero portraits, spell VFX textures, voice
lines, UI icons, and the terrain splat/cliff texture sets the D-7 heightmap terrain needs —
with **build-time** generative output (image models, TTS), hand-curated and committed as
ordinary owned assets. Zero runtime AI inference anywhere (G4.6 unaffected); generation
never happens in CI or at runtime. Quality and provenance risk tracked as
[R9](../01-vision/risks-and-open-questions.md#r9--generative-pipeline-quality-and-provenance-added-2026-06-11-per-d-2026-06-11-12).

### 5.2 Pipeline

```
generation spec ──► generate ──► curate (human accept/reject) ──► post-process ──► assetcheck ──► commit
(assetgen.toml)   (image/TTS,        mandatory gate             (atlas pack,      (full §3       (asset +
                   local or API)                                 .ogg encode)       gate)          provenance)
```

1. **Spec.** Each asset (or asset family) is described in a reviewed `assetgen.toml` entry:
   category, generator/model, prompt and parameters, output constraints (dimensions, style
   tags, atlas target). The spec file is in git — regeneration is reproducible *intent*,
   even where generator output is not bit-stable.
2. **Generate.** Image/TTS generation at asset-build time on a developer machine; raw
   candidates land in a scratch area, never directly in `assets/`.
3. **Curate.** Mandatory human gate: explicit per-asset accept. Nothing reaches `assets/`
   uncurated. Reject counts are logged per category (the R9 quality signal).
4. **Post-process.** Accepted outputs converted to engine formats: atlas-packed textures
   (R-RND-2), `.ogg` audio (R-AUD-1).
5. **Validate + commit.** Outputs pass the full `tools/assetcheck` gate (§3) like any other
   asset — no special-casing — and are committed with an `assets/MANIFEST` provenance entry.

### 5.3 Rules

- **Build-time only.** No generation in CI, no generation at runtime; the engine never
  links an inference dependency (impossible anyway under the G4.1 allowlist).
- **Provenance is mandatory (G4.7).** Every accepted asset's manifest entry records:
  generator/model + version, generation parameters (or `assetgen.toml` ref), date, and
  curator sign-off. The G4.2 CI scan fails on any generated asset missing these fields.
- **Output licensing must be commercially clear.** A generator whose output terms are
  unclear or restrictive is dropped, and its in-tree outputs are identified via the
  provenance manifest and reviewed for replacement (R9 trigger).
- **Curation is not optional** and has no bypass flag — same posture as `assetcheck`'s
  format rules.

---

## 6. Vendored gopher-lua fork *(added 2026-06-11 per D-2026-06-11-25)*

### 6.1 Purpose

The M5 Lua VM is a **forked `yuin/gopher-lua`, vendored in `repoes/`** under the same
LITD-PATCH discipline as the g3n fork (patches marked `// LITD-PATCH`, never silently
rebased to upstream). gopher-lua won on the three hard requirements (D-25): VM-level
coroutines are plain Go heap data (serializable — the only credible pure-Go option;
arnodel/golua uses goroutines = unserializable, Shopify/go-lua has no coroutines at all),
`pairs()` iteration is insertion-ordered (never ranges a Go map), and number→string
formatting is pure-Go strconv. MIT-licensed, inside the §1 permissive allowlist.

### 6.2 The four LITD patches

| # | Patch | Serves |
|---|---|---|
| 1 | **Instruction-budget counter in `mainLoop`** | R-SEC-1 per-tick quota + M7 lockstep tick budget — counted work, never timed |
| 2 | **Deterministic `mathlib` replacement** (fixed-point/table-based; `math.random` → sim PRNG) | Go's `math` package is not cross-arch bit-identical ([golang/go#20319](https://github.com/golang/go/issues/20319)) |
| 3 | **Coroutine/LState persister** (call frames, registry, upvalues; protos by chunk-id) | R-SIM-6 serializable scheduler — mid-game saves of suspended Lua coroutines |
| 4 | **LState/callframe pooling + golden cross-arch determinism CI test** | R-GC-1 zero-alloc tick; the CI test holds the fork to the same hash-matrix evidence standard as the Go sim (G5.7) |

### 6.3 Maintenance rules

- Same posture as `repoes/engine`: we own the fork; upstream bumps re-apply the LITD-PATCH
  set and re-run the golden cross-arch test before merge (the R7 audit trigger in
  [Risks](../01-vision/risks-and-open-questions.md)).
- Performance context (D-25): gopher-lua at ~5–10× C Lua is adequate — hot paths are Go sim
  code, not script bytecode.
- The fork is a runtime dependency, so it sits under the §1 license scan like everything
  else; its patches introduce no new dependencies.

---

## 7. Related documents

- [Overview](../01-vision/overview.md) — components map and the manifest's role as the
  generated "components/blizzard JSON".
- [Goals and Non-Goals](../01-vision/goals-and-non-goals.md) — the criterion IDs these tools gate.
- [Risks and Open Questions](../01-vision/risks-and-open-questions.md) — detection signals
  these tools produce.
- [Decisions](../01-vision/decisions.md) — the 2026-06-11 decision record (D-6, D-8, D-12,
  D-21, D-25) behind this document's revisions.
- [Milestones](./milestones.md) — when each tool and gate ships.
- [PRD](../../PRD.md) — source of truth.
