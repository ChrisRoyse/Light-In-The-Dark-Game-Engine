# Camera — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.2 R-RND-1](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~51** | `SetCameraField`/`GetCameraField`, pan/position, `camerasetup` objects, target/orient controllers, bounds, shake/noise, cine filters |
| `blizzard.j` BJs | **~34** | per-player guards (`CameraSetupApplyForPlayer`, `PanCameraToTimedLocBJ`, `SmartCameraPanBJ`), shake presets |

## Representative JASS signatures

```jass
native SetCameraField            takes camerafield whichField, real value, real duration returns nothing
native PanCameraToTimed          takes real x, real y, real duration returns nothing
native SetCameraBounds           takes real x1, real y1, real x2, real y2, real x3, real y3, real x4, real y4 returns nothing
native SetCameraTargetController takes unit whichUnit, real xoffset, real yoffset, boolean inheritOrientation returns nothing
native CreateCameraSetup         takes nothing returns camerasetup
native CameraSetupApplyForceDuration takes camerasetup whichSetup, boolean doPan, real forceDuration returns nothing
native CameraSetSmoothingFactor  takes real factor returns nothing

function CameraSetupApplyForPlayer takes boolean doPan, camerasetup whichSetup, player whichPlayer, real duration returns nothing
function SmartCameraPanBJ takes player whichPlayer, location loc, real duration returns nothing
function PanCameraToTimedLocBJ takes location loc, real duration returns nothing
```

## Canonical Go surface

```go
// Per-player camera object — kills the GetLocalPlayer()-guard idiom (hazard 1):
type Camera struct{ /* render-layer, per player */ }
func (g *Game) Camera(p Player) Camera

func (c Camera) Pan(to Vec2, opts ...CamOption)        // PanCameraTo + Timed + WithZ + Smart
// opts: Over(d), Z(height), OnlyIfOffscreen() (SmartCameraPan)

// D5 collapse of Get/SetCameraField × CAMERA_FIELD_* :
func (c Camera) Field(f CameraField) float64
func (c Camera) SetField(f CameraField, v float64, over time.Duration)
// + sugar for the locked-RTS fields actually allowed in gameplay (R-RND-1):
func (c Camera) SetZoom(dist float64, over time.Duration)

func (c Camera) SetBounds(r Rect)                      // 8-real quad → Rect (axis-aligned)
func (c Camera) Follow(u Unit, offset Vec2, inheritOrientation bool) // target controller
func (c Camera) Unfollow()
func (c Camera) Shake(magnitude float64)               // CameraSetSourceNoise/TargetNoise presets
func (c Camera) StopShake()
func (c Camera) Reset(over time.Duration)              // ResetToGameCamera

// Saved configurations (cinematics):
type CameraShot struct{ Fields [NumCameraFields]float64; Target Vec2 }  // camerasetup as value type
func (c Camera) Apply(s CameraShot, pan bool, over time.Duration)
```

`camerasetup` becomes the value type `CameraShot` — it was a plain bag of field values;
no handle lifetime needed (same R-API-2 logic as `location` → `Vec2`).

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthroughs dropped | `PanCameraToTimedLocBJ` → `c.Pan(v, Over(d))` |
| **D2** | per-player-guard BJs collapse — the per-player split moves into the `Camera(p)` receiver | `CameraSetupApplyForPlayer(doPan, setup, p, dur)` → `g.Camera(p).Apply(shot, doPan, dur)` |
| **D3** | XY/Loc/timed/Z pan variants → one `Pan` with options | `PanCameraTo`, `PanCameraToTimed`, `PanCameraToTimedWithZ`, `SmartCameraPanBJ` |
| **D4** | shake envelope math (`CameraSetEQNoiseForPlayer` magnitude→noise mapping) kept once | `Camera.Shake(mag)` |
| **D5** | `GetCameraField`/`SetCameraField` × ~10 field constants → `Field`/`SetField` | plus typed sugar for zoom |

Cine filters (`SetCineFilter*` full-screen color/texture overlay, `DisplayCineFilter`)
stay here as `c.ScreenFade(color, dur, opts)` — they're per-player camera-space
post effects (D2 collapse of the `CinematicFadeBJ` preset family).

## Subsystem dependencies

- **render** (primary): pure presentation. The camera is the *one* G3N camera; per-player only matters for which player's calls the local client honors. Locked-RTS constraints (R-RND-1: fixed yaw, pitch/zoom clamps) are enforced *here* — script `SetField` calls clamp in gameplay mode, unclamped in cinematic mode.
- **sim**: none — camera state must never affect simulation (no sim query may read camera position; selection/targeting use sim picks). Input layer reads camera for screen→world ray casts, then issues commands.
- **asset**: none (cine filter masks are simple textures).

## Porting hazards

1. **The `GetLocalPlayer()` camera idiom** (`if GetLocalPlayer() == p then PanCamera...`) was WC3's only per-player mechanism and a desync trap. `g.Camera(p)` makes it structural: calls for non-local players are recorded no-ops on other clients. This is the category's main design win — get it right and document it as the pattern for all per-player presentation.
2. **Camera fields interact** (distance vs angle-of-attack vs z-offset compute eye position together); apply field changes atomically per frame, not per call, or interpolated transitions fight each other.
3. **Cinematic vs gameplay mode**: cinematics need unclamped fields (R-RND-1 clamps are gameplay-only). Mode flag with explicit `ui.BeginCinematic()`/`EndCinematic()` (also letterboxes, hides HUD — coordinate with [ui-frames-and-dialogs](ui-frames-and-dialogs.md)).
4. **`SetCameraBounds` takes a quad** (4 corners) in JASS; LitD uses axis-aligned `Rect` — non-axis-aligned camera bounds are tombstoned (no known legitimate use).
5. **Smoothing/duration timing is render-rate**, deliberately outside sim ticks — fine, but `time.Duration` here is wall-clock-ish; never let scripts read interpolated camera state back into sim logic.
