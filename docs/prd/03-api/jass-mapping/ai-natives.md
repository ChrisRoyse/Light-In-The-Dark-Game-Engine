# AI Natives — JASS → Go Mapping (DEFERRED TO v2)

> Part of the [JASS API mapping](README.md). Status per PRD [§9.4 (open question, draft: defer)](../../../PRD.md)
> and **R-EXEC-3**: computer-player AI is **out of v1 scope**. This file documents the
> surface so the manifest can classify it (tombstoned "v2"), per the §4.2 acceptance
> criterion that nothing is *silently* dropped.

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~8** | the map-script-side hooks: `StartMeleeAI`, `StartCampaignAI`, `CommandAI`, `PauseCompAI`, `GetAIDifficulty`, guard-position natives |
| `repoes/war3-types/scripts/common.ai` natives | **123** | the separate AI-script context: build/train/harvest orders, attack-wave management, `GetUnitCount*`, sleep/wait, command-stack pop |
| `blizzard.j` BJs | **~1** | difficulty plumbing (most melee-AI BJs counted under [game-state-and-melee](game-state-and-melee.md)) |

## Representative JASS signatures

```jass
// common.j (map-script side):
native StartMeleeAI    takes player num, string script returns nothing
native CommandAI       takes player num, integer command, integer data returns nothing
native PauseCompAI     takes player p, boolean pause returns nothing
native GetAIDifficulty takes player num returns aidifficulty
native RemoveGuardPosition takes unit hUnit returns nothing

// common.ai (AI-script side, separate context):
native SetMeleeAI        takes nothing returns nothing
native SetBuildUnit      takes integer qty, integer unitid returns nothing
native AttackMoveKillA   takes unit target returns nothing
native GetUnitCountDone  takes integer unitid returns integer
native StartGetEnemyBase takes nothing returns nothing
native CommandsWaiting   takes nothing returns integer
native GetLastCommand    takes nothing returns integer
native PopLastCommand    takes nothing returns nothing
```

## v2 canonical Go surface (sketch only — NOT in the v1 API)

Per **R-EXEC-3**, the JASS AI runtime is an isolated execution domain: max 6
threads/player, **no shared globals** with the map script, communication via
integer-pair command stacks. The v2 design honors that isolation natively:

```go
// v2 SKETCH — subject to its own PRD:
type AIController interface {
    Tick(view AIView, cmd AICommander) // runs in its own sandboxed scheduler
}
func (g *Game) AttachAI(p Player, ai AIController, d Difficulty) // StartMeleeAI
func (g *Game) PauseAI(p Player, b bool)
func (g *Game) CommandAI(p Player, command, data int)  // typed channel under the hood

// AIView: read-only, AI-legal queries (own units, visible enemies, counts).
// AICommander: build/train/attack-wave intents (SetBuildUnit, AttackMoveKillA analogues).
```

The integer-pair command stack (`CommandAI` / `CommandsWaiting`/`GetLastCommand`/
`PopLastCommand`) becomes a typed message queue — the Go-channel equivalent named by
R-EXEC-3.

## Dedup rules (manifest classification only in v1)

| Rule | Application |
|---|---|
| **D1–D3** | Applied on paper in the manifest: `...Loc` and difficulty-preset wrappers noted against their general forms so the v2 port starts deduplicated |
| **D4** | The melee AI *strategy scripts* themselves (build orders, attack waves in Blizzard's `.ai` files) are content, not API — v2 ships original equivalents in the `melee` package |
| **Tombstone "v2"** | All ~131 functions (8 common.j + 123 common.ai) carry manifest status `deferred-v2`, satisfying §4.2(b): explicitly tombstoned with reason, not silently dropped |

v1 exception: the **map-script-side hooks** (`StartMeleeAI`, `PauseCompAI`,
`GetAIDifficulty`) get v1 stub mappings that no-op with a logged warning, so
`melee.Standard` (see [game-state-and-melee](game-state-and-melee.md)) is structurally
complete — computer slots simply idle in v1 skirmish.

## Subsystem dependencies (v2)

- **sim**: AI runs in a sandboxed scheduler *inside* the deterministic tick (AI decisions are sim inputs and must be replay-identical — same R-SIM-2 bar as player commands). No wall-clock, no goroutine races; budgeted CPU slice within the 10 ms tick.
- **render**: none (AI must work headless, R-SIM-4 — also how AI-vs-AI CI soak tests run).
- **asset**: AI build-order/strategy tables in `data/` rather than compiled `.ai` scripts.

## Porting hazards (recorded now for v2)

1. **Determinism vs "AI thread sleeps"**: JASS AI scripts busy-loop with `Sleep`. The v2 scheduler must quantize AI waits to ticks exactly like triggers (R-EXEC-5), or AI becomes the replay-divergence source.
2. **Broken-in-WC3 natives**: the JASS manual notes string/callback natives misbehave in the AI context — do not replicate those bugs; the isolation boundary (AIView/AICommander) makes them unrepresentable instead.
3. **Information leakage**: JASS AI natives could query beyond fog in places. v2 `AIView` should be fog-honest by default with an explicit cheating-difficulty escape hatch — gameplay decision to make consciously, not inherit.
4. **Scope creep into v1**: unit-level *behaviors* (guard positions, return-fire, auto-acquire) are **not** "AI natives" — they're core sim combat behavior in [units](units.md) scope and ship in v1. Only *strategic* computer-player control is deferred. `RemoveGuardPosition`/`RecycleGuardPosition` sit on this boundary: classified v1-sim (creep camp behavior is needed for melee maps).
