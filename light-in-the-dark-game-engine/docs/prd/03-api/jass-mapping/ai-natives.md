# AI Natives — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Status per
> [D-2026-06-11-6](../../01-vision/decisions.md) (supersedes the same-day deferral D-2026-06-11-4)
> and **R-EXEC-3**: the JASS AI domain is a **full v1 port**, shipped as its own milestone
> **M5.5** — a second sandboxed scheduler domain with isolated script contexts (no shared
> globals with map scripts) and command-stack messaging. Every function in this category maps
> canonically; none carries a `deferred-v2` tombstone for capability reasons.

*Revised 2026-06-11 per D-2026-06-11-6.*

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

## Canonical Go surface (v1, M5.5)

Per **R-EXEC-3**, the JASS AI runtime is an isolated execution domain: max 6
threads/player, **no shared globals** with the map script, communication via
integer-pair command stacks. The port honors that isolation natively:

```go
type AIController interface {
    Tick(view AIView, cmd AICommander) // runs in the second sandboxed scheduler domain
}
func (g *Game) AttachAI(p Player, ai AIController, d Difficulty) // StartMeleeAI / StartCampaignAI
func (g *Game) PauseAI(p Player, b bool)                         // PauseCompAI
func (g *Game) AIDifficulty(p Player) Difficulty                 // GetAIDifficulty
func (g *Game) CommandAI(p Player, command, data int)            // typed channel under the hood

// AIView: read-only, AI-legal queries (own units, visible enemies, the GetUnitCount* family).
// AICommander: build/train/harvest/attack-wave intents (SetBuildUnit, AttackMoveKillA,
// StartGetEnemyBase analogues) plus the receive side of the command stack
// (CommandsWaiting / GetLastCommand / PopLastCommand → a typed inbox on AICommander).
// AI sleep/wait natives map to the same scheduler wait verbs as map scripts, quantized
// to ticks (R-EXEC-5) inside the AI domain's own job space.
```

The integer-pair command stack (`CommandAI` / `CommandsWaiting`/`GetLastCommand`/
`PopLastCommand`) becomes a typed message queue — the Go-channel equivalent named by
R-EXEC-3. Each AI player gets its own scheduler instance (same deterministic scheduler
type, separate job space) running in a dedicated phase of the tick
([Execution model §6](../execution-model.md#6-ai-domain-isolation-r-exec-3)).

## Dedup rules applied

| Rule | Application |
|---|---|
| **D1–D3** | Applied in the M2 manifest as everywhere else: `...Loc` and difficulty-preset wrappers collapse onto their general forms before implementation at M5.5 |
| **D4** | The melee AI *strategy scripts* themselves (build orders, attack waves in Blizzard's `.ai` files) are content, not API — v1 ships original equivalents as `data/` strategy tables consumed by the standard melee `AIController` |
| **Canonical, milestone M5.5** | All ~131 functions (8 common.j + 123 common.ai) carry canonical Go mappings in the manifest with implementation milestone M5.5 — no `deferred-v2` tombstones for capability reasons (D-2026-06-11-6), satisfying §4.2(b) with mappings rather than deferrals |

The earlier plan to ship v1 no-op stubs for the map-script-side hooks (`StartMeleeAI`,
`PauseCompAI`, `GetAIDifficulty`) is **obsolete**: these map canonically to
`AttachAI`/`PauseAI`/`AIDifficulty`, and M6's melee opponent runs on the real AI domain
(see [game-state-and-melee](game-state-and-melee.md)) — computer slots are live, not idle.
*Revised 2026-06-11 per D-2026-06-11-6.*

## Subsystem dependencies

- **sim**: AI runs in a sandboxed scheduler *inside* the deterministic tick (AI decisions are sim inputs and must be replay-identical — same R-SIM-2 bar as player commands). No wall-clock, no goroutine races; budgeted CPU slice within the 10 ms tick.
- **render**: none (AI must work headless, R-SIM-4 — also how AI-vs-AI CI soak tests run).
- **asset**: AI build-order/strategy tables in `data/` rather than compiled `.ai` scripts.

## Porting hazards

1. **Determinism vs "AI thread sleeps"**: JASS AI scripts busy-loop with `Sleep`. The AI-domain scheduler must quantize AI waits to ticks exactly like triggers (R-EXEC-5), or AI becomes the replay-divergence source.
2. **Broken-in-WC3 natives**: the JASS manual notes string/callback natives misbehave in the AI context — do not replicate those bugs; the isolation boundary (AIView/AICommander) makes them unrepresentable instead.
3. **Information leakage**: JASS AI natives could query beyond fog in places. `AIView` should be fog-honest by default with an explicit cheating-difficulty escape hatch — gameplay decision to make consciously, not inherit.
4. **Scope boundary with the sim core**: unit-level *behaviors* (guard positions, return-fire, auto-acquire) are **not** "AI natives" — they're core sim combat behavior in [units](units.md) scope and ship with the M3 sim core. Only *strategic* computer-player control waits for M5.5. `RemoveGuardPosition`/`RecycleGuardPosition` sit on this boundary: classified v1-sim (creep camp behavior is needed for melee maps).
