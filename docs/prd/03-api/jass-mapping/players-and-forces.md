# Players & Forces — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 API shape](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~85** | `Player`, `GetPlayer*`/`SetPlayer*` state, alliances, `force` CRUD + enum |
| `blizzard.j` BJs | **~90** | alliance preset helpers, `GetPlayersAllies/Enemies/Matching`, slot-state helpers |

## Representative JASS signatures

```jass
constant native Player              takes integer number returns player
constant native GetLocalPlayer      takes nothing returns player
constant native GetPlayerState      takes player whichPlayer, playerstate whichPlayerState returns integer
native SetPlayerState           takes player whichPlayer, playerstate whichPlayerState, integer value returns nothing
native GetPlayerName            takes player whichPlayer returns string
native SetPlayerAlliance        takes player sourcePlayer, player otherPlayer, alliancetype whichAllianceSetting, boolean value returns nothing
native CreateForce              takes nothing returns force

function SetPlayerAllianceStateBJ takes player sourcePlayer, player otherPlayer, integer allianceState returns nothing
function GetPlayersAllies takes player whichPlayer returns force
function CreateForceBJ takes nothing returns force
```

## Canonical Go surface

```go
type Player struct{ /* opaque slot index */ }

func (g *Game) Player(slot int) Player
func (g *Game) LocalPlayer() Player          // render-only context; see hazard 1
func (g *Game) Players(filter PlayerFilter) []Player  // replaces force enumeration

func (p Player) Name() string
func (p Player) SetName(s string)
func (p Player) Gold() int                    // D5 accessors over PLAYER_STATE_RESOURCE_*
func (p Player) SetGold(v int)
func (p Player) Lumber() int
func (p Player) SetLumber(v int)
func (p Player) FoodUsed() int
func (p Player) FoodCap() int
func (p Player) Race() Race
func (p Player) Controller() Controller       // user / computer / neutral / rescuable
func (p Player) SetAlliance(other Player, a AllianceFlags)  // one bitset call
func (p Player) IsAlly(other Player) bool
func (p Player) IsEnemy(other Player) bool
func (p Player) StartLocation() Vec2
```

The JASS `force` handle type is **deleted**: per R-EXEC-4, forces were just sets of
players with callback enumeration (`ForForce`/`GetEnumPlayer`). Canonical Go uses
`[]Player` slices and `PlayerFilter` closures.

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough BJs dropped | `GetPlayerNameBJ` → `Player.Name()` |
| **D2** | preset-combination BJs collapse onto the full call | `SetPlayerAllianceStateAllyBJ`, `...VisionBJ`, `...ControlBJ`, `...FullControlBJ` → `SetAlliance(other, flags)` with `AllianceFlags` bitset |
| **D3** | `ForForce` + `GetEnumPlayer` + `CountPlayersInForceBJ` → slice returns | `g.Players(filter)` then `len(...)`/`range` |
| **D4** | real-logic helpers kept once | `GetPlayersAllies` → `helpers.Allies(p)`; melee slot/team availability checks live in [game-state-and-melee](game-state-and-melee.md) helpers |
| **D5** | `GetPlayerState`/`SetPlayerState` × `PLAYER_STATE_*` → typed accessors | `Player.Gold()`, `Player.SetFoodCap(v)`, tax-rate accessors |

## Subsystem dependencies

- **sim** (primary): player table (resources, alliances, tech, controller type) is plain sim state; alliance changes are deterministic commands in the tick stream.
- **render**: player color → team-color shader uniform (R-RND-7); name labels in UI.
- **asset**: race/start-location data from map data tables (R-AST-1).

## Porting hazards

1. **`GetLocalPlayer` is the classic desync footgun.** In WC3 it forks logic per client; any sim mutation inside a local block desyncs. In LitD, `LocalPlayer()` is only legal in render/UI-layer callbacks — the API enforces this by exposing it on a render-context object, not on sim-facing `Game` methods. Compile-time separation per §4.1 hard rule.
2. **Alliance is asymmetric** in WC3 (A allied to B ≠ B allied to A). `SetAlliance` must stay one-directional; helper `helpers.SetAllianceMutual` provides the BJ convenience once (D4).
3. **Neutral players** (hostile, passive, victim, extra) are fixed high slots (12–15 classic, up to 27 in Reforged). Decide slot count constant in M2 spec; expose as named accessors `g.NeutralHostile()` etc.
4. **Player leave/defeat events** interact with game-state natives — event payloads here, end-game logic in [game-state-and-melee](game-state-and-melee.md).
5. **Handicap and tax-rate** states are obscure but capability-bearing — map them (D5 accessors), don't tombstone.
