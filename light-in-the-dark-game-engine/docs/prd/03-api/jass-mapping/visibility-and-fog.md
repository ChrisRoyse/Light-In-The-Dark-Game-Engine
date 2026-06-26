# Visibility & Fog of War — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5; §8 fog-of-war risk](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~35** | `fogmodifier` CRUD, `SetFogState*`, point visibility queries (`IsVisibleToPlayer`, `IsLocationFoggedToPlayer`, `IsLocationMaskedToPlayer`), `FogMaskEnable`/`FogEnable`, day/night models |
| `blizzard.j` BJs | **~19** | `CreateFogModifierRectBJ`-style wrappers, `FogEnableOn/Off` toggles |

## Representative JASS signatures

```jass
native SetFogStateRect       takes player forWhichPlayer, fogstate whichState, rect where, boolean useSharedVision returns nothing
native SetFogStateRadius     takes player forWhichPlayer, fogstate whichState, real centerx, real centerY, real radius, boolean useSharedVision returns nothing
native CreateFogModifierRect takes player forWhichPlayer, fogstate whichState, rect where, boolean useSharedVision, boolean afterUnits returns fogmodifier
native FogModifierStart      takes fogmodifier whichFogModifier returns nothing
native IsVisibleToPlayer     takes real x, real y, player whichPlayer returns boolean
native IsFoggedToPlayer      takes real x, real y, player whichPlayer returns boolean
native FogMaskEnable         takes boolean enable returns nothing
native FogEnable             takes boolean enable returns nothing

function CreateFogModifierRectBJ takes boolean enabled, player whichPlayer, fogstate whichFogState, rect r returns fogmodifier
function FogEnableOn takes nothing returns nothing
```

## Canonical Go surface

```go
type FogState uint8 // FogVisible, FogFogged (explored), FogMasked (black mask)

type FogModifier struct{ /* opaque handle */ }

// One canonical constructor; rect vs radius is the shape option (D3):
func (g *Game) NewFogModifier(p Player, state FogState, area Area, opts ...FogOption) FogModifier
// Area is Rect | Circle{Center Vec2, Radius float64}; opts: SharedVision(b), AfterUnits(b), Started()

func (f FogModifier) Start()
func (f FogModifier) Stop()
func (f FogModifier) Destroy()

// Instant state writes (no modifier object lifetime):
func (g *Game) SetFogState(p Player, state FogState, area Area, sharedVision bool)

// Queries:
func (g *Game) IsVisibleTo(p Player, pos Vec2) bool
func (g *Game) FogStateAt(p Player, pos Vec2) FogState  // collapses IsFogged/IsMasked/IsVisible triple

// Global toggles:
func (g *Game) SetFogEnabled(b bool)     // FogEnable
func (g *Game) SetFogMaskEnabled(b bool) // FogMaskEnable
func (u Unit) ShareVision(p Player, b bool)   // UnitShareVision
func (g *Game) SetDayNightModels(terrainModel, unitModel string) // render-side
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | toggle/passthrough BJs dropped | `FogEnableOn()`/`FogEnableOff()` → `SetFogEnabled(true/false)` |
| **D2** | enabled-by-default BJ ctors collapse | `CreateFogModifierRectBJ(enabled, ...)` → `NewFogModifier(..., Started())` |
| **D3** | rect/radius/Loc constructor variants → one ctor with `Area` sum type | `CreateFogModifierRect`/`...Radius`/`...RadiusLoc` → `NewFogModifier(p, s, area)` |
| **D4** | n/a (no real-logic BJs here) | — |
| **D5** | the visible/fogged/masked query triple → one `FogStateAt` + convenience `IsVisibleTo` | three booleans were one enum all along |

## Subsystem dependencies

- **sim** (primary): per-player visibility grid updated from unit sight radii each tick — **gameplay state**, not cosmetic: targeting, acquisition, and `IsVisibleTo` queries read it deterministically (R-SIM-2). Modifier areas rasterize onto the same grid.
- **render**: fog *display* is a custom shader/render pass (PRD §8 explicitly flags G3N has no built-in fog of war — scheduled in M4): visibility texture sampled over terrain, dimming for explored, black for masked. Render reads the local player's sim grid, never writes.
- **asset**: day/night ambient models are light-rig presets in `data/`, not Blizzard model files.

## Porting hazards

1. **Two fogs, one word**: JASS `SetTerrainFog`/`SetFogColor` etc. are *atmospheric distance fog* (render-only — categorized under [effects-and-graphics](effects-and-graphics.md)); this category is *fog of war*. The manifest must split them or the audit double-counts.
2. **Sim/render split runs through the middle of the category**: visibility grid = sim; fog rendering + `SetDayNightModels` = render. Per §4.1 the public API must keep `IsVisibleTo` answerable headless (R-SIM-4).
3. **Grid resolution & determinism**: WC3 vision is grid-quantized with line-of-sight blocking by cliffs/trees. Cell size + LOS rules are gameplay-defining; fixed-point sight radii, deterministic raster order (R-SIM-2).
4. **Performance**: naive per-tick full-grid recompute for 16 players × 500 units busts the 10 ms tick budget — incremental updates (dirty sight circles) required; benchmark in M3.
5. **`useSharedVision` semantics** (modifier follows allied-vision changes) interacts with [players-and-forces](players-and-forces.md) alliance flags — test matrix needed.
6. **Local-player rendering only**: render shows `LocalPlayer()`'s fog; replays/observers need a viewable-player switch — design the render hook for it now, cheap later.
