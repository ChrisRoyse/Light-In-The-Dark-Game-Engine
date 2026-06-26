# Units — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 API shape](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~235** | `*Unit*` families plus orders, waygates, widget accessors, corpse/decay |
| `blizzard.j` BJs | **~125** | `...BJ` passthroughs, `...Loc` variants, `GetLastCreatedUnit` last-handle pattern |

The single largest category: unit lifecycle, state, movement, orders, queries, and the
`widget` supertype shared with items/destructables.

## Representative JASS signatures

```jass
native CreateUnit          takes player id, integer unitid, real x, real y, real face returns unit
native KillUnit            takes unit whichUnit returns nothing
native RemoveUnit          takes unit whichUnit returns nothing
native SetUnitState        takes unit whichUnit, unitstate whichUnitState, real newVal returns nothing
native SetUnitX            takes unit whichUnit, real newX returns nothing
native SetUnitPosition     takes unit whichUnit, real newX, real newY returns nothing
native SetUnitMoveSpeed    takes unit whichUnit, real newSpeed returns nothing
native IssuePointOrder     takes unit whichUnit, string order, real x, real y returns boolean

function KillUnitBJ takes unit whichUnit returns nothing
function SetUnitLifeBJ takes unit whichUnit, real newValue returns nothing
function GetLastCreatedUnit takes nothing returns unit
function IssuePointOrderLocBJ takes unit whichUnit, string order, location whichLocation returns boolean
```

## Canonical Go surface

```go
type Unit struct{ /* opaque handle into sim ECS */ }

func (g *Game) CreateUnit(owner Player, typ UnitType, pos Vec2, facing Angle, opts ...UnitOption) Unit

func (u Unit) Kill()
func (u Unit) Remove()
func (u Unit) Position() Vec2
func (u Unit) SetPosition(p Vec2)        // collapses SetUnitX/SetUnitY/SetUnitPosition/SetUnitPositionLoc
func (u Unit) Facing() Angle
func (u Unit) SetFacing(a Angle)
func (u Unit) Life() float64             // D5 typed accessors over the unitstate table
func (u Unit) SetLife(v float64)
func (u Unit) Mana() float64
func (u Unit) SetMana(v float64)
func (u Unit) MoveSpeed() float64
func (u Unit) SetMoveSpeed(v float64)
func (u Unit) Order(ord Order, target OrderTarget) bool  // point/target/immediate/instant unified
func (u Unit) Owner() Player
func (u Unit) SetOwner(p Player, changeColor bool)
func (u Unit) Is(pred UnitPredicate) bool // IsUnitType/IsUnitAlly/IsUnitInRange... predicate set
```

`CreateUnit` returns the created unit directly — the `GetLastCreatedUnit` /
`bj_lastCreatedUnit` global-side-channel pattern is deleted wholesale.

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | ~60 BJs are pure passthroughs — dropped | `KillUnitBJ(u)` → `Unit.Kill()` |
| **D2** | Reordered/defaulted creates collapse onto one ctor with options | `CreateUnitAtLoc`, `CreateUnitAtLocSaveLast` → `Game.CreateUnit(...)` |
| **D3** | Coordinate-pair vs `location` vs axis-only families collapse onto `Vec2` | `SetUnitX`/`SetUnitY`/`SetUnitPosition`/`SetUnitPositionLoc` → `SetPosition(Vec2)`; all `Issue*OrderLoc*` → `Order(...)` |
| **D4** | Real-logic BJs kept once in `helpers` | rescue behavior (`InitRescuableBehaviorBJ`), delayed decay suppression (`DelayedSuspendDecay*`) |
| **D5** | `GetUnitState`/`SetUnitState` × `UNIT_STATE_*` → typed accessors | `Unit.Life()`, `Unit.SetMaxMana(v)` backed by one state table |

The 12-way order matrix (`IssueImmediateOrder`, `IssuePointOrder`, `IssueTargetOrder`,
`IssueInstantPointOrder`, … each × string/`ById`) is a D3 collapse onto one
`Order(ord Order, target OrderTarget)` where `OrderTarget` is a sum type
(none | point `Vec2` | widget). Order strings and order IDs unify on a typed `Order`
constant set.

## Subsystem dependencies

- **sim** (primary): ECS component stores for transform, life/mana, movement, order queue, combat stats. Every method here is a sim mutation/query; deterministic per R-SIM-2.
- **render**: reads transform + animation state only (unit model, team color uniform per R-RND-7, selection circle). Never written from here (R-API-6).
- **asset**: `UnitType` rows from `data/` JSON tables (R-AST-1); GLB model + clip names `Idle/Walk/Attack/Death` (R-AST-3).

## Porting hazards

1. **Widget supertype.** JASS `unit`/`item`/`destructable` all inherit `widget` (shared life, `TriggerRegisterDeathEvent`, `AddSpecialEffectTarget`). Go has no inheritance — define a small `Widget` interface implemented by all three, used only where genuinely polymorphic (death events, effect attach).
2. **Invalid-handle semantics.** WC3 silently no-ops on dead/null unit handles. Per R-API-5 the zero-value `Unit` must be a safe no-op object, with debug-mode asserts.
3. **`SetUnitPosition` vs `SetUnitX/Y`** differ in WC3: the former respects pathing/collision, the latter teleports raw. The collapse must keep both capabilities: `SetPosition(p)` (pathed) + option `Teleport()` — capability preserved, not averaged away.
4. **Order queue determinism.** Orders issued inside event handlers must enqueue in deterministic order (R-EXEC-2); the `bj_lastIssuedOrder` style globals do not exist.
5. **Corpse/decay states** (`UNIT_STATE_*`, raise/decay timing) are gameplay-visible; tick-quantize all decay timers (R-EXEC-5).
