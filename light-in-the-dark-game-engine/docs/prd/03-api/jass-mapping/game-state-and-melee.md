# Game State, Map Control & Melee — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.1 sim core](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~105** | game speed/pause/end, time of day, map flags/placement/start locations, victory/defeat, save/load, cheats, replays, tournament, food/bounty, `Preload`, automation-test natives |
| `blizzard.j` BJs | **~72** | the melee game-mode library (`MeleeStartingUnits`, `MeleeStartingResources`, victory/defeat dialogs), `InitBlizzard`, stock updates, day/night presets |

## Representative JASS signatures

```jass
native SetGameSpeed     takes gamespeed whichspeed returns nothing
native SetMapFlag       takes mapflag whichMapFlag, boolean value returns nothing
native EndGame          takes boolean doScoreScreen returns nothing
native SetTimeOfDay     takes real whatTime returns nothing
native VersionGet       takes nothing returns version
native Preload          takes string filename returns nothing
native PauseGame        takes boolean flag returns nothing
native SetFloatGameState takes fgamestate whichFloatGameState, real value returns nothing

function MeleeStartingUnits takes nothing returns nothing
function MeleeStartingResources takes nothing returns nothing
function MeleeVictoryDialogBJ takes player whichPlayer, boolean leftGame returns nothing
function CheckInitPlayerSlotAvailability takes player whichPlayer returns boolean
function InitBlizzard takes nothing returns nothing
```

## Canonical Go surface

```go
// Match lifecycle:
func (g *Game) Pause(b bool)
func (g *Game) SetSpeed(s GameSpeed)
func (g *Game) EndMatch(showScore bool)
func (g *Game) Victory(p Player)                // CachePlayerHeroData + RemovePlayer(VICTORY) collapse
func (g *Game) Defeat(p Player, reason string)
func (p Player) RemoveFromMatch(result MatchResult)

// Clock:
func (g *Game) TimeOfDay() float64              // 0..24, D5 over GetFloatGameState(GAME_STATE_TIME_OF_DAY)
func (g *Game) SetTimeOfDay(h float64)
func (g *Game) SetTimeOfDayScale(s float64)
func (g *Game) SuspendTimeOfDay(b bool)
func (g *Game) ElapsedTime() time.Duration      // ticks → duration

// Map/match configuration (read at load, mostly immutable after):
func (g *Game) MapFlag(f MapFlag) bool
func (g *Game) SetMapFlag(f MapFlag, b bool)
func (g *Game) StartLocation(i int) Vec2
func (g *Game) Teams() int
func (g *Game) IsReplay() bool

// Persistence — full v1 scope (D-2026-06-11-9): mid-game saves ship at M6 on the
// serializable scheduler; campaign cross-map persistence (D-2026-06-11-15) rides the
// same pipeline via SaveData (see hashtable-and-gamecache):
func (g *Game) SaveMatch(name string) error     // SaveGame + the SaveGameExists/sync dance
func (g *Game) LoadMatch(name string) error

// Melee game mode — blizzard.j's biggest real-logic block, kept per D4 as a
// composable standard mode built ONLY on the public API:
package melee // litd/api/helpers/melee
func Standard(g *litd.Game) // visibility, starting units/resources/heroes, AI, victory conditions
func StartingUnits(g *litd.Game)
func StartingResources(g *litd.Game)
func VictoryDefeatConditions(g *litd.Game)
```

*Revised 2026-06-11 per D-2026-06-11-6:* `melee.Standard`'s computer slots run on the **real
AI domain from M5.5** — they attach the standard melee `AIController` via `g.AttachAI`
([ai-natives](ai-natives.md)), not a Go stopgap and not the previously planned no-op stubs.
M6's vertical-slice opponent is this AI.

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough BJs dropped | `PauseGameOn/Off` → `Pause(b)` |
| **D2** | preset wrappers collapse | `SetTimeOfDayScalePercentBJ` → `SetTimeOfDayScale(s)` |
| **D3** | dawn/dusk constant variants → plain values | `bj_TOD_DAWN` style → exported `litd.Dawn = 6.0` constants |
| **D4** | **the melee library is the flagship D4 case**: ~40 BJs of real game-mode logic (slot checks, starting units per race, creep camp AI, victory dialogs) kept once as the `melee` package | `MeleeStartingUnits` → `melee.StartingUnits(g)` |
| **D5** | `Get/SetFloatGameState`/`Get/SetIntegerGameState` × constants → typed accessors | `TimeOfDay()`, divine-intervention counters |

Tombstoned: cheat natives (single-player cheats — replaced by a debug console),
tournament natives (Battle.net tournament infra), `VersionGet`/campaign-availability
(WC3 SKU detection), `Preload*` (asset pipeline preloads declaratively),
`InitBlizzard` (engine boots itself), automation-test natives (Go tests instead).
Each gets an explicit manifest reason per §4.2's acceptance criterion.

## Subsystem dependencies

- **sim** (primary): game clock, time-of-day, pause, match result state are sim state; `Victory`/`Defeat` emit events and stop command acceptance deterministically. Save/load serializes the full sim state **including the scheduler's suspension records** — suspended coroutines, pending timers, event subscriptions (D-2026-06-11-9) — plus the campaign `SaveData` store (D-2026-06-11-15); the same serializer backs the determinism hash (R-SIM-2/R-SIM-4), and replays (command streams) remain a separate, complementary mechanism.
- **render**: score screen, victory/defeat dialogs (via [ui-frames-and-dialogs](ui-frames-and-dialogs.md)); day/night lighting interpolation reads `TimeOfDay()`.
- **asset**: map metadata (start locations, teams, flags) from the map file; melee starting-unit tables per race in `data/` (R-AST-1).

## Porting hazards

1. **Save/load is a sim-architecture feature, not an API feature** — versioned snapshot of every ECS store, RNG seed, scheduler continuations (suspended `helpers.Wait` coroutines!). *Revised 2026-06-11 per D-2026-06-11-9 — this is now committed design, not a risk:* the cooperative scheduler is **serializable from day one** — suspended coroutines, timers, and event subscriptions all serialize into the save format ([Execution model §2.1 S-5 / §2.3](../execution-model.md)); the representation is constrained at M1, implemented at M3, and full mid-game save/load ships at M6. The remaining hazard is only discipline: every new sim/scheduler feature must extend the save schema in the same change.
2. **`SetGameSpeed`/`SetTimeOfDayScale` change tick-to-game-time mapping**: keep sim tick rate fixed at 20 Hz and scale *game-time per tick* — otherwise every duration in the engine needs re-quantizing (breaks R-EXEC-5).
3. **Melee package is the dogfood gate**: if `melee.Standard` can't be written purely on the public API, the API is missing capability — schedule it as the M5 acceptance test.
4. **Victory/defeat for ALL players** must resolve in one deterministic pass (WC3 had ordering quirks with simultaneous defeat); define tie rules.
5. **Pause semantics**: WC3 pause stops sim but UI/camera stay live — matches §4.1 split naturally (render loop continues; sim ticks halt), but waits/timers must not drift (they're tick-based, so they freeze correctly by construction).
