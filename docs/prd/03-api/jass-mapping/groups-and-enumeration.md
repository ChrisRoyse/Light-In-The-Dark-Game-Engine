# Groups & Enumeration — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.4 R-EXEC-4](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~31** | `group` CRUD, `GroupEnumUnits*` family, `ForGroup`, `FirstOfGroup`, group orders, `GetEnumUnit`/`GetFilterUnit` |
| `blizzard.j` BJs | **~38** | `GetUnitsInRectAll/Matching`, `GetUnitsOfPlayerAll`, `ForGroupBJ`, `CountUnitsInGroup`, `GroupPickRandomUnit`, recycled-group machinery |

## Representative JASS signatures

```jass
native CreateGroup              takes nothing returns group
native GroupAddUnit             takes group whichGroup, unit whichUnit returns boolean
native GroupEnumUnitsInRect     takes group whichGroup, rect r, boolexpr filter returns nothing
native GroupEnumUnitsInRange    takes group whichGroup, real x, real y, real radius, boolexpr filter returns nothing
native ForGroup                 takes group whichGroup, code callback returns nothing
native FirstOfGroup             takes group whichGroup returns unit
constant native GetEnumUnit     takes nothing returns unit
constant native GetFilterUnit   takes nothing returns unit

function ForGroupBJ takes group whichGroup, code callback returns nothing
function GetUnitsInRectAll takes rect r returns group
function GroupPickRandomUnit takes group whichGroup returns unit
function CountUnitsInGroupEnum takes nothing returns nothing
```

## Canonical Go surface

Per **R-EXEC-4** the callback-enum machinery (`group` scratch containers, `boolexpr`
filters, `ForGroup` + thread-local `GetEnumUnit`, destroy-after-use leak patterns)
collapses into slice/iterator queries on `Game`:

```go
type UnitFilter func(Unit) bool

func (g *Game) UnitsIn(r Rect, f UnitFilter) []Unit          // GroupEnumUnitsInRect
func (g *Game) UnitsInRange(center Vec2, radius float64, f UnitFilter) []Unit
func (g *Game) UnitsOf(p Player, f UnitFilter) []Unit        // ...OfPlayer / ...OfType...
func (g *Game) SelectedUnits(p Player) []Unit                // GroupEnumUnitsSelected
func (g *Game) AllUnits(f UnitFilter) []Unit

// Persistent named sets (the surviving use of `group` as a *capability*):
type UnitSet struct{ /* ordered, deterministic set */ }
func (g *Game) NewUnitSet() *UnitSet
func (s *UnitSet) Add(u Unit) bool
func (s *UnitSet) Remove(u Unit) bool
func (s *UnitSet) Contains(u Unit) bool
func (s *UnitSet) Units() []Unit
func (s *UnitSet) Order(ord Order, target OrderTarget) bool  // GroupPointOrder etc.

// helpers (D4):
func helpers.RandomUnit(units []Unit) Unit  // GroupPickRandomUnit, seeded sim PRNG
func helpers.ClosestUnit(units []Unit, to Vec2) Unit
```

Composite filters (`And`/`Or`/`Not` boolexprs) are plain Go `&&`/`||` inside one
closure — the combinator natives are tombstoned (see [triggers-and-events](triggers-and-events.md)).

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | `ForGroupBJ` and other passthroughs dropped | `range g.UnitsIn(...)` |
| **D2** | "All" convenience wrappers collapse onto nil-filter | `GetUnitsInRectAll(r)` → `g.UnitsIn(r, nil)` |
| **D3** | XY vs Loc vs rect enum variants → `Vec2`/`Rect` value types | `GroupEnumUnitsInRangeOfLoc` → `UnitsInRange(Vec2, r, f)` |
| **D4** | real logic kept once in helpers | `GroupPickRandomUnit` → `helpers.RandomUnit`; the blizzard.j group-recycling pool (`bj_wantDestroyGroup`) becomes internal slice pooling — invisible to users |
| **D5** | n/a (no state-table pairs here) | — |

`FirstOfGroup` loops (the JASS idiom for iteration-with-removal) are tombstoned:
`range` over a returned slice is the replacement; no capability lost.

## Subsystem dependencies

- **sim** (primary): spatial queries hit the sim's spatial index (grid/quadtree shared with combat target acquisition, R-SIM-3). Query results are deterministic-ordered (entity creation index, never map order — R-SIM-2). Result slices come from per-tick arena pools (R-GC-1/2).
- **render**: none.
- **asset**: none.

## Porting hazards

1. **Result ordering is gameplay-visible** (`FirstOfGroup`, "pick random", "order all"): define and document one canonical sort (ascending entity id) and never expose spatial-index internal order.
2. **Slice aliasing**: returned `[]Unit` slices are pooled — define lifetime ("valid until end of current handler") or copy-on-return for the public API; benchmark decides (R-GC-1 vs safety). Helpers like `helpers.Wait` interacting with held slices is the trap case.
3. **Snapshot vs live semantics**: WC3 enum snapshots membership at enum time; units dying mid-iteration remain in the group as null-ish handles. LitD: queries return snapshots; dead units stay in the slice as valid handles whose methods no-op (R-API-5).
4. **Filter purity**: filters run inside spatial-index traversal — they must not mutate sim state (R-EXEC-2 purity rule); enforce by passing read-only views in debug builds.
5. **`UnitSet` determinism**: persistent sets must be insertion-ordered structures, not Go maps (R-SIM-2).
