# Public API — Naming, Style, Documentation, and Stability

> Supports [PRD §4 (Public API Design)](../../PRD.md#4-public-api-design) and resolves
> [PRD §9 open question 2](../../PRD.md#9-open-questions) (JASS-flavored aliases vs idiomatic Go).
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Siblings** | [Architecture](architecture.md) · [Deduplication policy](deduplication-policy.md) · [Public API design](public-api-design.md) · [Execution model](execution-model.md) |

---

## 1. Position on the naming question

PRD §9.2 asks: keep JASS-flavored names as aliases (`CreateUnit` free function) or go purely
idiomatic? **Decision (per the PRD draft, confirmed here): idiomatic only.** No alias layer —
aliases are exactly the duplication the
[deduplication policy](deduplication-policy.md) exists to remove, and two names per capability
would double the godoc surface and split searchability. Migrating WC3 modders are served instead
by the generated JASS→Go mapping table (§3), which is cheaper to maintain (it falls out of the
manifest) and more honest (it explains *why* names changed, including tombstones).

## 2. Go naming conventions for the port

### 2.1 General rules

All standard Go conventions apply (Effective Go, Go Code Review Comments); the rules below are
the port-specific decisions.

| Rule | Convention | JASS → Go example |
|---|---|---|
| **N-1** | Receiver absorbs the noun: strip the receiver's name from the method. | `GetUnitName(u)` → `u.Name()`, not `u.UnitName()` |
| **N-2** | No `Get` prefix on getters (Go standard). Setters keep `Set`. | `GetUnitState(u, UNIT_STATE_LIFE)` → `u.Life()`; `SetUnitState(...)` → `u.SetLife(v)` |
| **N-3** | Predicates read as assertions; drop the `Is` where the receiver makes it read naturally, keep it where dropping is ambiguous. | `IsUnitPaused(u)` → `u.Paused()`; `IsUnitAlly(u, p)` → `u.IsAlly(p)` |
| **N-4** | JASS SCREAMING_SNAKE constant families become typed Go constants with the type name as prefix. | `UNIT_STATE_LIFE` → internal to D5 accessors; `ATTACK_TYPE_MELEE` → `litd.AttackMelee`; `EVENT_UNIT_DEATH` → `litd.EventUnitDeath` |
| **N-5** | The `BJ` suffix, `bj_` globals, and `Swap`/`SaveLast` mangles never appear in any Go name. | `SetUnitLifeBJ`, `GetUnitStateSwap`, `CreateUnitAtLocSaveLast` → all collapse per [D2/D5](deduplication-policy.md) |
| **N-6** | Initialisms follow Go style: `ID`, `UI`, `HP` only if used at all; four-char rawcodes (`'hfoo'`) are not exposed — unit types are string ids from the `data/` tables ([PRD R-AST-1](../../PRD.md#6-asset--data-pipeline)). | `unitid integer` ('hfoo') → `litd.UnitType("footman")` |
| **N-7** | Option constructors read as modifiers: `litd.DamageRanged()`, `litd.ForPlayer(p)`, `litd.WithColorChange()` — verb-less, declarative ([R-API-3](public-api-design.md#33-r-api-3--options-structs-for-the-long-tail)). | — |
| **N-8** | Event names: `Event` + noun + past-tense/state verb, matching JASS's taxonomy without its prefixes. | `EVENT_PLAYER_UNIT_DEATH` → `litd.EventUnitDeath` (player scoping is an option, not a name) |
| **N-9** | Package vocabulary: one public package `litd` (import path `litd/api`, package name `litd`), plus `helpers` and content-flavored subpackages (`helpers/melee`). No stuttering: `litd.Unit`, never `litd.LitdUnit`/`api.APIUnit`. | — |
| **N-10** | Durations are `time.Duration` in signatures; angle constructors are `litd.Deg`/`litd.Rad`; never bare `float64` for either ([R-API-2](public-api-design.md#32-r-api-2--value-types-for-math-no-heap-location-no-manual-cleanup)). | `real face` (degrees) → `litd.Deg(270)` |

### 2.2 Where JASS names survive

A JASS name survives only when it is genuinely the best name: `PolledWait` stays
`helpers.PolledWait` because WC3 modders know exactly what it means and no Go-native name says it
better. The bar: the surviving name must be the one we would have chosen anyway. Expected
survivors are nouns (`Unit`, `Timer`, `Region`) and a handful of terms of art (`Order`,
`PolledWait`, melee terminology). Everything else renames per N-1…N-10.

## 3. The JASS→Go mapping table

The migration document for WC3 modders, generated from `api-manifest.json`
([Deduplication policy §8](deduplication-policy.md#8-the-audit-report)) into
`docs/api/jass-to-go.md` on every manifest change. Never hand-edited; CI fails if it is stale.

### 3.1 Format

One row per source function — all 2,521, including tombstones, so every search hits:

| JASS function | Rule | LitD equivalent | Notes |
|---|---|---|---|
| `KillUnit` | D1 | `u.Kill()` | |
| `SetUnitLifeBJ` | D5 | `u.SetLife(v)` | clamps at 0, as the BJ did |
| `SetUnitPositionLoc` | D3 | `u.SetPosition(pos)` | `location` → `litd.Vec2` |
| `UnitDamageTargetBJ` | D2 | `u.Damage(t, amt)` | BJ defaults = zero options |
| `PolledWait` | D4 | `helpers.PolledWait(d)` | suspends on scheduler; no polling |
| `GetLastCreatedUnit` | — (tombstone: superseded) | use the return value of `g.CreateUnit` | |
| `Preload` | — (tombstone: gameplay-irrelevant) | asset pipeline handles loading | |
| `AICaptain...` (commonai) | AI domain | canonical AI-domain API, milestone M5.5 ([ai-natives](jass-mapping/ai-natives.md)) | full v1 port, isolated per [R-EXEC-3](execution-model.md#6-ai-domain-isolation). *Revised 2026-06-11 per D-2026-06-11-6* |

### 3.2 Structure of the published doc

1. **Front matter:** the five collapse rules in one paragraph each, linking to the
   [deduplication policy](deduplication-policy.md); the execution-model "short version"
   ([Execution model §8](execution-model.md#8-what-the-modder-must-know-the-short-version)).
2. **Concept crosswalk:** a short table for the *pattern-level* migrations that one-row mappings
   can't teach — trigger zoo → `OnEvent` + closures, `ForGroup` loops → slices,
   `location` lifecycle → `Vec2` values, `bj_lastCreated*` → return values, real-time vs
   game-time waits → game time only.
3. **The full A–Z table** (generated), `common.j` and `blizzard.j` sections, searchable.
4. **Tombstone appendix:** every tombstone with its reason and replacement guidance.

Acceptance: a modder who knows a JASS function name can reach the canonical Go spelling (or the
explicit reason it doesn't exist) in one search, for 100% of the 2,521 functions.

## 4. Godoc standards

Godoc is the API spec's user-facing form; the M2 sign-off reviews godoc text, not just
signatures.

- **G-1 (capability docs):** every exported symbol has a doc comment beginning with its name (Go
  convention) and stating behavior in sim terms — tick quantization, zero-value-handle behavior,
  and event-ordering guarantees are documented on the symbols where they bite, with links to the
  package-level execution-model summary.
- **G-2 (provenance line):** every method carries a machine-inserted provenance line generated
  from the manifest, e.g. `// JASS: SetUnitState(UNIT_STATE_LIFE), SetUnitLifeBJ,
  SetUnitLifePercentBJ.` This is the inline version of the mapping table and is regenerated, not
  hand-maintained.
- **G-3 (defaults are contractual):** option-bearing methods document the zero-option behavior
  explicitly ("With no options, behaves like the WC3 melee attack default: …"), since the
  [deduplication policy](deduplication-policy.md#3-d2--bj-reorders-or-defaults-parameters-drop-the-bj-canonical-takes-the-full-set)
  makes BJ defaults reachable only by omission.
- **G-4 (examples compile):** runnable `Example` functions for each noun type and for the five
  modder-contract rules; `go test` executes them headless
  ([Architecture §4](architecture.md#4-headless-mode)), so documentation rot is a test failure.
- **G-5 (no internals):** doc comments never reference `litd/sim`, `litd/render`, G3N, ECS
  indices, or scheduler mechanics beyond their observable guarantees. The docs describe the
  contract; [execution-model.md](execution-model.md) describes the machine.
- **G-6 (debug-mode notes):** behaviors that differ in debug mode (invalid-handle asserts,
  filter-purity sampling, the non-yielding-job watchdog) are flagged with a uniform
  `Debug mode:` paragraph.

## 5. Versioning and stability policy

### 5.1 Pre-1.0 (M0–M6)

The module is `v0.x`: the API may change between milestones, but **not silently** — every
breaking change must update the manifest, regenerate the mapping table and audit report in the
same commit, and appear in `CHANGELOG.md`. The M2 API spec freeze is a *shape* freeze (types,
patterns, R-API rules); individual signatures may still move until M5.

### 5.2 v1.0 and the stability contract

At M5/M6 exit, `litd/api` (including `helpers`) is declared stable under Go module semantics:

- **V-1 (semver, Go-flavored):** no breaking change to any exported identifier of `litd/api`
  within major version 1. Breaking means: removal, signature change, or a *behavioral* change to
  documented semantics — including tick quantization, event ordering, and zero-value-handle
  behavior, which are contract, not implementation
  ([Execution model](execution-model.md)). A v2 module path (`litd/v2`) is required to break.
- **V-2 (additive evolution by design):** the patterns chosen for v1 exist to make almost
  everything additive — new functional options on existing verbs, new methods on nouns, new
  event kinds, new helper packages all land in minor releases. This is the long-game payoff of
  R-API-3.
- **V-3 (internals are not API):** `litd/sim`, `litd/render`, `litd/asset` live under
  `internal/` or are documented as unstable; only `litd/api` and `helpers` carry the guarantee.
  ([Architecture §2](architecture.md#2-import-rules) makes this mechanical.)
- **V-4 (determinism versioning):** any change that alters simulation outcomes for identical
  command streams — even a bug fix — bumps an explicit `SimVersion` constant; replays and saved
  state embed it and refuse silent cross-version replay. API stability and sim-behavior
  stability are tracked separately because RTS balance fixes must remain possible without an
  API major version.
- **V-5 (deprecation, not deletion):** within v1, superseded symbols gain a `// Deprecated:`
  godoc tag pointing at the replacement and remain functional for the remainder of the major
  version. The manifest records deprecations exactly like tombstones, so the mapping table and
  audit report stay total.
- **V-6 (API diff gate):** CI runs an API-surface diff (`apidiff`-style) on every PR; any change
  to the exported surface of `litd/api` requires a changelog entry and, post-1.0, fails the
  build if it is non-additive.
- **V-7 (Lua parity, v1/M5):** the Lua binding ships in v1 at M5
  ([Architecture §6](architecture.md#6-the-lua-binding-layer-v1-m5)); its surface versions
  in lockstep with the Go API from the same manifest; a map script declares the API version it
  targets. *Revised 2026-06-11 per D-2026-06-11-8.*

### 5.3 What is explicitly *not* guaranteed

Performance characteristics (beyond the CI-gated budgets), debug-mode output text, the contents
of `internal/` packages, the render layer's visual output, and the order of fields in options
structs (construct them with field names). Documenting the non-guarantees now prevents them from
hardening into accidental contract.

## 6. Acceptance criteria for this section

- `golangci-lint` config with naming rules (N-1…N-10 encodable subset) green from M0.
- Mapping-table generator wired to the manifest from M2; staleness is a CI failure.
- Godoc coverage lint: 100% of exported symbols documented, provenance lines present, examples
  compile and pass headless, from M5.
- API-diff gate active from the M2 shape freeze; `SimVersion` plumbing present from M3 (first
  replay-capable build).
