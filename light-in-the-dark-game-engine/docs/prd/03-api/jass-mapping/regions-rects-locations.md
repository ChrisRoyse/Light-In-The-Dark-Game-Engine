# Regions, Rects & Locations — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 R-API-2](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~39** | `Rect`/`region`/`location` CRUD, region cell add/remove, containment tests, `GetLocationZ`, `GetWorldBounds` |
| `blizzard.j` BJs | **~30** | `OffsetLocation`, `RectFromCenterSizeBJ`, `GetRectCenter`, `RegionAddBJ`, random-point-in-rect helpers |

## Representative JASS signatures

```jass
native Rect             takes real minx, real miny, real maxx, real maxy returns rect
native CreateRegion     takes nothing returns region
native RegionAddRect    takes region whichRegion, rect r returns nothing
native IsPointInRegion  takes region whichRegion, real x, real y returns boolean
native Location         takes real x, real y returns location
native RemoveLocation   takes location whichLocation returns nothing
native MoveLocation     takes location whichLocation, real newX, real newY returns nothing
native GetLocationX     takes location whichLocation returns real

function OffsetLocation takes location loc, real dx, real dy returns location
function RectFromCenterSizeBJ takes location center, real width, real height returns rect
function GetRandomLocInRect takes rect whichRect returns location
```

## Canonical Go surface

Per **R-API-2** this category mostly *evaporates into value types* — the biggest
net-deletion category relative to its source count:

```go
// Value types — no handles, no Remove*, no leaks:
type Vec2 struct{ X, Y float64 }   // replaces `location` entirely
type Rect struct{ Min, Max Vec2 }

func NewRect(min, max Vec2) Rect
func RectAround(center Vec2, w, h float64) Rect   // RectFromCenterSizeBJ
func (r Rect) Center() Vec2
func (r Rect) Contains(p Vec2) bool
func (r Rect) Offset(d Vec2) Rect                 // MoveRectTo* family

func (v Vec2) Add(o Vec2) Vec2                    // OffsetLocation
func (v Vec2) Polar(dist float64, a Angle) Vec2   // PolarProjectionBJ (see math-strings-conversion)

// Region survives as the *trigger-area* capability (enter/leave events need
// engine-side cell tracking — a value type can't do that):
type Region struct{ /* opaque handle: cell set in sim */ }
func (g *Game) NewRegion() Region
func (r Region) AddRect(rc Rect)
func (r Region) RemoveRect(rc Rect)
func (r Region) AddCell(p Vec2)
func (r Region) Contains(p Vec2) bool
func (r Region) ContainsUnit(u Unit) bool

func (g *Game) WorldBounds() Rect                  // GetWorldBounds / bj_mapInitialPlayableArea
func (g *Game) TerrainHeight(p Vec2) float64       // GetLocationZ
func (g *Game) RandomPointIn(r Rect) Vec2          // GetRandomLocInRect, seeded sim PRNG
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthroughs dropped | `RegionAddBJ` → `Region.AddRect` |
| **D2** | center+size constructors collapse | `RectFromCenterSizeBJ` → `RectAround` |
| **D3** | **the headline rule here**: every `...Loc` API in *all other categories* collapses because `location` → `Vec2`; the entire location CRUD (`Location`, `RemoveLocation`, `MoveLocation`, `GetLocationX/Y`) is tombstoned as superseded by a struct literal | `Location(x,y)` → `Vec2{x,y}` |
| **D4** | random-point and rect-math helpers kept once | `GetRandomLocInRect` → `g.RandomPointIn(r)` (must use sim PRNG) |
| **D5** | getter pairs become struct fields | `GetRectMinX`... → `r.Min.X` |

Tombstone count is high by design: ~20 of the 39 natives exist only to manage
`location`/`rect` heap lifetime — Go values + GC delete the entire problem (R-API-2).

## Subsystem dependencies

- **sim** (primary): `Region` cell sets live on the sim's 32×32-style grid for O(1) enter/leave detection during movement resolution; emits `EventRegionEnter/Leave` through the event bus. `TerrainHeight` reads the sim heightfield.
- **render**: none (rects/regions are invisible; debug overlay optional).
- **asset**: world bounds + camera bounds from map data.

## Porting hazards

1. **`GetLocationZ` is async-ish and render-coupled in WC3** (reads the visible terrain, returns stale values for non-local players — a known desync source). LitD: `TerrainHeight` reads the *sim* heightfield, fully deterministic; document the divergence.
2. **Region granularity**: WC3 regions are cell-quantized (32-unit grid), so `RegionAddRect` then `Contains` on the rect edge differs from exact float math. Pick cell size, document quantization, keep enter/leave events consistent with it.
3. **Float vs fixed-point**: `Vec2` is shown as `float64` but M1's determinism spike may switch sim math to fixed-point — keep `Vec2` opaque enough (constructor + accessors in hot paths) that the representation can change without breaking the API. This category sets the precedent for all gameplay math types.
4. **Rect mutation natives** (`SetRect`, `MoveRectTo`) imply identity — Go `Rect` is a value, so APIs elsewhere that held a `rect` handle (fog modifiers, weather) must take rect values at call time, re-issued to move.
