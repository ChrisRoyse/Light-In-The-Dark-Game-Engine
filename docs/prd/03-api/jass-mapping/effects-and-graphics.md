# Effects, Terrain & World Graphics — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.2 render budgets](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~97** | special effects (+ Blz effect property setters), lightning, weather, text tags, images/ubersplats, terrain type/height/pathability, blight, water color, terrain deformations, sky/atmospheric fog |
| `blizzard.j` BJs | **~78** | `AddSpecialEffectLocBJ`, `GetLastCreatedEffectBJ`, lightning/texttag percent wrappers, terrain BJs |

## Representative JASS signatures

```jass
native AddSpecialEffect        takes string modelName, real x, real y returns effect
native AddSpecialEffectTarget  takes string modelName, widget targetWidget, string attachPointName returns effect
native DestroyEffect           takes effect whichEffect returns nothing
native AddLightning            takes string codeName, boolean checkVisibility, real x1, real y1, real x2, real y2 returns lightning
native CreateTextTag           takes nothing returns texttag
native AddWeatherEffect        takes rect where, integer effectID returns weathereffect
native SetTerrainType          takes real x, real y, integer terrainType, integer variation, integer area, integer shape returns nothing
native TerrainDeformCrater     takes real x, real y, real radius, real depth, integer duration, boolean permanent returns terraindeformation

function AddSpecialEffectLocBJ takes location where, string modelName returns effect
function CreateTextTagLocBJ takes string s, location loc, real zOffset, real size, real red, real green, real blue, real transparency returns texttag
function SetTerrainTypeBJ takes location where, integer terrainType, integer variation, integer area, integer shape returns nothing
```

## Canonical Go surface

```go
// Special effects:
type Effect struct{ /* render-layer handle */ }
func (g *Game) SpawnEffect(model EffectType, at Vec2, opts ...EffectOption) Effect
func (g *Game) AttachEffect(model EffectType, target Widget, attach AttachPoint) Effect
func (e Effect) Destroy()                       // plays death anim then frees (WC3 semantics)
func (e Effect) DestroyNow()                    // BlzSetSpecialEffectPosition off-map trick, made explicit
func (e Effect) SetScale(s float64)             // D5 over the ~20 BlzSetSpecialEffect* setters
func (e Effect) SetColor(c Color)
func (e Effect) SetOrientation(yaw, pitch, roll Angle)
func (e Effect) PlayAnimation(name string)

// Lightning beams:
type Beam struct{ /* ... */ }
func (g *Game) SpawnBeam(typ BeamType, from, to Vec3, opts ...BeamOption) Beam
func (b Beam) Move(from, to Vec3)               // MoveLightningEx
func (b Beam) SetColor(c Color)                 // SetLightningColor

// Floating world text:
func (g *Game) SpawnTextTag(text string, at Vec2, opts ...TextTagOption) TextTag
// opts: Size, Color, Velocity, Lifespan, FadePoint, Permanent, VisibleTo(p)

// Weather, terrain, water, sky:
func (g *Game) SetWeather(area Rect, typ WeatherType) Weather   // + Weather.Stop()
func (g *Game) SetTerrainType(at Vec2, t TerrainTile, variation, area int, shape Shape)
func (g *Game) TerrainType(at Vec2) TerrainTile
func (g *Game) SetTerrainPathable(at Vec2, ptype PathingType, allow bool)
func (g *Game) DeformTerrain(d Deformation) DeformationHandle   // crater/ripple/wave/random
func (g *Game) SetBlight(p Player, area Area, blighted bool)    // rect/circle/point collapse
func (g *Game) SetWaterColor(c Color)
func (g *Game) SetSkyModel(name string)
func (g *Game) SetDistanceFog(f FogSettings)    // SetTerrainFog/ResetTerrainFog/SetTerrainFogEx
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough/`GetLastCreatedEffectBJ` patterns dropped | constructor returns the handle |
| **D2** | preset wrappers collapse onto options | `CreateTextTagLocBJ`'s 8 positional args → `SpawnTextTag(text, at, Size(s), Color(c))` (R-API-3) |
| **D3** | XY/Loc/unit variants → `Vec2`/widget overloads | `AddSpecialEffectLoc` → `SpawnEffect(model, at)` |
| **D4** | composite VFX recipes kept once | `TerrainDeformationRandomBJ`, blight-circle rasterizing → helper/options |
| **D5** | `BlzSetSpecialEffect{Scale,Color,Alpha,Roll,Pitch,Yaw,X,Y,Z,Position,…}` family (~20) → ~6 typed setters | `Effect.SetScale/SetColor/SetOrientation/SetPosition` |

Tombstoned: `image`/`ubersplat` natives (superseded by `SpawnDecal` — one decal
primitive replaces both legacy systems), `Preload` of model paths (asset pipeline
handles preloading), checkVisibility flags folded into a `VisibleTo` option.

## Subsystem dependencies

- **render** (primary): effects/beams/text tags/weather/water/sky are pure presentation — pooled scene-graph nodes, hard caps for the draw-call budget (R-RND-3) and VFX light cap (R-RND-4).
- **sim**: **terrain type, pathability, blight, and permanent deformation are gameplay state** — they live in the sim grid (pathfinding R-SIM-5, blight feeds undead mechanics) and must be deterministic. Cosmetic deformation (temporary craters) is render-only. The category straddles the §4.1 line — the manifest must tag each native sim/render explicitly.
- **asset**: `EffectType` → GLB/particle definitions in `data/` (replaces Blizzard model path strings — no `.mdl` paths in the API, §2.2); weather/beam type tables; decal textures.

## Porting hazards

1. **Model-path strings are an IP trap**: JASS takes `"Abilities\\Spells\\..."` Blizzard paths. LitD takes typed keys into its own CC0 asset tables — `jassgen` must tombstone every hardcoded path constant.
2. **`DestroyEffect` plays the death animation** (WC3 quirk maps rely on: spawn+destroy = one-shot VFX). Keep `Destroy()` (death anim) vs `DestroyNow()` (immediate) explicit.
3. **Sim/render split inside one native family**: `TerrainDeformCrater(..., permanent=true)` changes walkable height (sim) while `permanent=false` is cosmetic (render). Two code paths behind one signature — test both against the determinism hash.
4. **Text tag limit** (WC3: 100 visible, per-player visibility): pool with cap; `VisibleTo` is per-player presentation like camera calls.
5. **G3N has no particle system** — "effects" are animated GLB meshes/billboards in v1; a particle MVP is an M4 line item. Don't promise particle-grade VFX in API docs.
6. **Atmospheric fog vs fog of war** naming collision — distance fog is here, vision fog in [visibility-and-fog](visibility-and-fog.md); manifest tags prevent double-count.
