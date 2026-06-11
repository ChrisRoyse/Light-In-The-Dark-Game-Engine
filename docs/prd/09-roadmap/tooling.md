# Roadmap — Tooling Specifications

> Specifies the three tools the PRD requires: the `tools/jassgen` parser/generator
> (R-AST-4, [PRD §6](../../PRD.md#6-asset--data-pipeline)), the asset-validation CLI
> (R-FMT-2, R-AST-2/3), and the CI benchmark harness that enforces the
> [§5.3 budgets](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
> and GC discipline (R-GC-1…5). Milestone ownership per [Milestones](./milestones.md);
> deduplication policy per [PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule).

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
| `commonai.d.ts` | `repoes/war3-types/core/commonai.d.ts` | AI natives — classified and tombstoned per [Q4](../01-vision/risks-and-open-questions.md#q4--commonaidts-ai-natives-port-in-v1-or-defer) |
| `overrides.toml` | `tools/jassgen/overrides.toml` | Hand-reviewed classification/mapping decisions (in git, code-reviewed) |

### 2.2 Pipeline

```
.j / .d.ts ──► parse ──► merge/enrich ──► classify (D1–D5) ──► apply overrides
                                                                    │
              ┌─────────────────────────────────────────────────────┤
              ▼                    ▼                     ▼           ▼
      api-manifest.json     audit report        Go API stubs   JASS→Go table
                          (md + machine JSON)   (litd/api)     (docs)
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
5. **Emit.** Four outputs, all deterministic: the manifest, the audit report (markdown for
   humans + JSON for CI gates), compiling panic-body stubs for canonical symbols (M2
   deliverable), and the JASS→Go mapping table for the docs (G2.6).

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
| `unimplemented` == 0 (stub bodies remaining) | M5 |
| `featureTags` rollup (the [R4](../01-vision/risks-and-open-questions.md#r4--api-surface-underestimation-natives-needing-engine-features-g3n-lacks) engine-feature census) | reviewed at M2, gates M4 planning |

Additionally, CI re-runs `jassgen` and fails if any output differs from the committed
version (reproducibility gate, M2 onward).

### 2.4 `api-manifest.json` schema

Proposed JSON Schema (draft 2020-12), abbreviated to the load-bearing parts:

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
            "package": { "enum": ["litd/api", "litd/api/helpers"] },
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

// Tombstoned example — commonai deferral per Q4
{ "name": "StartUnitAbilityOrder", "origin": "commonai",
  "signature": { "params": [], "returns": "boolean" },
  "classification": "D3", "classifiedBy": "override",
  "disposition": "tombstoned",
  "tombstone": { "reason": "deferred-v2",
                 "detail": "AI domain deferred per Q4; isolated scheduler design (R-EXEC-3)" } }
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
subject exists (see the [gates summary](./milestones.md#9-cross-milestone-gates-summary)).

### 4.2 Suites

**Headless sim suite (from M3).** Runs on every CI machine — deterministic, GPU-free.
- Scenario: scripted 500-unit + 500-projectile sustained combat on a 128×128 map.
- Metrics: worst-case and p99 tick time (budget ≤ 10 ms, asserted with a CI-machine
  calibration factor; the reference-machine number is authoritative), `testing.AllocsPerRun`
  on the tick path (budget: 0; R-GC-1/5), state-hash determinism across the OS/arch matrix
  (G5.1–G5.3).

**Render suite (from M4).** Requires a GPU runner; the authoritative pass is a periodic run
on the physical reference machine (dual-core 2 GHz, UHD 620, 4 GB RAM), with a per-commit
proxy run on a capped CI GPU runner.
- Scenes: "typical" (mixed economy + small battle) and "worst case" (500 units on screen).
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

## 5. Related documents

- [Overview](../01-vision/overview.md) — components map and the manifest's role as the
  generated "components/blizzard JSON".
- [Goals and Non-Goals](../01-vision/goals-and-non-goals.md) — the criterion IDs these tools gate.
- [Risks and Open Questions](../01-vision/risks-and-open-questions.md) — detection signals
  these tools produce.
- [Milestones](./milestones.md) — when each tool and gate ships.
- [PRD](../../PRD.md) — source of truth.
