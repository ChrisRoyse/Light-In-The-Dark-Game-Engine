package litd

// Camera surface (#248, camera.md; public-api-design.md §2 row 19). The camera
// is per-player render state with ZERO sim coupling — no sim query can read it,
// and a camera verb never mutates the sim. The per-player receiver g.Camera(p)
// structurally eliminates the GetLocalPlayer idiom: a call for a non-local
// player is a recorded no-op, so a script can never branch the simulation on
// the local view (the classic WC3 desync vector). Gameplay-mode field writes
// are clamped per R-RND-1; BeginCinematic gates the unclamped range explicitly.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// CameraField indexes the camera's continuous parameters (the D5 collapse of
// the Get/SetCameraField constant family).
type CameraField uint8

// The CameraField values index the camera's continuous parameters in the order
// the field accessors and clamp table expect.
const (
	CameraTargetDistance CameraField = iota // "zoom"
	CameraFarZ
	CameraAngleOfAttack
	CameraFieldOfView
	CameraRoll
	CameraRotation
	CameraNearZ
	cameraFieldCount
)

// camClamp is the R-RND-1 gameplay clamp per field {min,max}. Cinematic mode
// bypasses it.
var camClamp = [cameraFieldCount][2]float64{
	CameraTargetDistance: {200, 5000},
	CameraFarZ:           {100, 10000},
	CameraAngleOfAttack:  {0, 90},
	CameraFieldOfView:    {30, 120},
	CameraRoll:           {-180, 180},
	CameraRotation:       {0, 360},
	CameraNearZ:          {1, 1000},
}

type cameraState struct {
	cinematic bool
	follow    Unit
	field     [cameraFieldCount]float64
	fieldSet  [cameraFieldCount]bool
}

// CameraEventKind tags a CameraEvent.
type CameraEventKind uint8

// The CameraEventKind values tag the camera operations emitted as CameraEvents.
const (
	CameraPan CameraEventKind = iota
	CameraSetField
	CameraSetBounds
	CameraFollow
	CameraStopFollow
	CameraBeginCinematic
	CameraEndCinematic
	CameraShake
)

// CameraEvent is one resolved view request handed to the render sink. Field
// values are already clamped (in gameplay mode). It carries no sim state.
type CameraEvent struct {
	// Kind selects which camera action this event describes.
	Kind CameraEventKind
	// Player is the recipient player slot (camera is per-player presentation).
	Player int
	// Pos is the target world position (pan/jump destinations).
	Pos Vec2
	// Z is the target world height, where applicable.
	Z float64
	// Duration is the move/transition time in seconds (0 = instant).
	Duration float64
	// Field is the camera parameter addressed by a field-set event.
	Field CameraField
	// Value is the new value for the addressed Field.
	Value float64
	// Target is the unit the camera locks to, if any.
	Target Unit
	// Bounds is the camera movement bounding rectangle for a bounds event.
	Bounds Rect
	// Magnitude is the shake/noise amplitude for an effect event.
	Magnitude float64
}

// SetLocalPlayer records which player's view this client renders. Camera verbs
// for other players become recorded no-ops. -1 (any out-of-range slot) means
// no local viewer (the headless default). This is render config, not sim state.
func (g *Game) SetLocalPlayer(p Player) {
	if g == nil {
		return
	}
	if !p.Valid() {
		g.localPlayer = -1
		return
	}
	g.localPlayer = p.idx
}

// OnCamera installs the render/test sink. nil restores headless no-op behavior.
// Camera is sim-inert, so installing a sink cannot change the state hash.
func (g *Game) OnCamera(f func(CameraEvent)) {
	if g != nil {
		g.onCamera = f
	}
}

// Camera returns the per-player camera control handle. The zero Camera for an
// invalid player.
func (g *Game) Camera(p Player) Camera {
	if g == nil || !p.Valid() {
		return Camera{}
	}
	return Camera{id: uint32(p.idx) + 1, g: g}
}

func (c Camera) slot() int32 { return int32(c.id) - 1 }

// isLocal reports whether this camera drives the local view. Non-local verbs
// are recorded no-ops (they never reach the sink or mutate local state).
func (c Camera) isLocal() bool { return c.g != nil && c.g.localPlayer == c.slot() }

func (c Camera) state() *cameraState { return &c.g.cam[c.slot()] }

func (c Camera) emit(ev CameraEvent) {
	ev.Player = int(c.slot())
	if c.g.onCamera != nil {
		c.g.onCamera(ev)
	}
}

// Pan moves the camera target to pos. Options set a transition duration and a
// height. A non-local-player camera records the request without moving the
// local view. JASS: PanCameraTo/Timed/WithZ and SmartCameraPan collapse here.
// JASS: PanCameraTo, PanCameraToForPlayer, PanCameraToLocForPlayer, PanCameraToTimed, PanCameraToTimedForPlayer, PanCameraToTimedLocForPlayer, PanCameraToTimedLocWithZForPlayer, PanCameraToTimedWithZ, PanCameraToWithZ, SetCameraPosition, SetCameraPositionForPlayer, SetCameraPositionLocForPlayer, SetCameraQuickPosition, SetCameraQuickPositionForPlayer, SetCameraQuickPositionLoc, SetCameraQuickPositionLocForPlayer, SmartCameraPanBJ
func (c Camera) Pan(pos Vec2, opts ...PanOption) {
	if !c.valid("Camera.Pan") || !c.isLocal() {
		return
	}
	cfg := panConfig{}
	for _, o := range opts {
		o(&cfg)
	}
	c.emit(CameraEvent{Kind: CameraPan, Pos: pos, Z: cfg.z, Duration: cfg.duration})
}

// PanOption configures Pan (R-API-3 functional option).
type PanOption func(*panConfig)

type panConfig struct {
	duration float64
	z        float64
}

// PanOver sets the pan transition duration in seconds (default 0 = snap).
func PanOver(seconds float64) PanOption {
	return func(c *panConfig) {
		if seconds > 0 {
			c.duration = seconds
		}
	}
}

// PanHeight sets the pan target height (z).
func PanHeight(z float64) PanOption { return func(c *panConfig) { c.z = z } }

// Field returns the camera's last applied value for f (0 if never set). This
// reads the api's per-player applied-value cache, never the sim. JASS:
// GetCameraField.
// JASS: CameraSetupGetField, CameraSetupGetFieldSwap, GetCameraField
func (c Camera) Field(f CameraField) float64 {
	if !c.valid("Camera.Field") || f >= cameraFieldCount {
		return 0
	}
	return c.state().field[f]
}

// SetField sets a camera field. In gameplay mode the value is clamped per
// R-RND-1; inside a cinematic (BeginCinematic) it is applied verbatim. A
// non-local-player camera records the request without changing the local view.
// JASS: AdjustCameraField, CameraSetupSetField, SetCameraField, SetCameraFieldForPlayer
func (c Camera) SetField(f CameraField, v float64) {
	if !c.valid("Camera.SetField") || f >= cameraFieldCount {
		return
	}
	st := c.state()
	applied := v
	if !st.cinematic {
		lo, hi := camClamp[f][0], camClamp[f][1]
		if applied < lo {
			applied = lo
		}
		if applied > hi {
			applied = hi
		}
	}
	if !c.isLocal() {
		return // recorded no-op: do not mutate local view state
	}
	st.field[f] = applied
	st.fieldSet[f] = true
	c.emit(CameraEvent{Kind: CameraSetField, Field: f, Value: applied})
}

// SetZoom is typed sugar for SetField(CameraTargetDistance, …).
func (c Camera) SetZoom(distance float64) { c.SetField(CameraTargetDistance, distance) }

// SetBounds restricts the camera to an axis-aligned rectangle. (WC3's
// non-axis-aligned quad bounds are tombstoned.) JASS: SetCameraBounds.
// JASS: AdjustCameraBoundsBJ, AdjustCameraBoundsForPlayerBJ, SetCameraBounds, SetCameraBoundsToRect, SetCameraBoundsToRectForPlayerBJ
func (c Camera) SetBounds(r Rect) {
	if !c.valid("Camera.SetBounds") || !c.isLocal() {
		return
	}
	c.emit(CameraEvent{Kind: CameraSetBounds, Bounds: r})
}

// Follow locks the camera onto a unit. Following a dead/invalid unit releases
// the lock (no panic). JASS: SetCameraTargetController, the camera-follow BJs.
// JASS: SetCameraTargetController, SetCameraTargetControllerNoZForPlayer
func (c Camera) Follow(u Unit) {
	if !c.valid("Camera.Follow") || !c.isLocal() {
		return
	}
	st := c.state()
	if !u.Valid() {
		st.follow = Unit{}
		c.emit(CameraEvent{Kind: CameraStopFollow})
		return
	}
	st.follow = u
	c.emit(CameraEvent{Kind: CameraFollow, Target: u})
}

// Following returns the unit the camera is locked onto, or the zero Unit. A
// followed unit that has since died reports as released (the lock self-clears).
func (c Camera) Following() Unit {
	if !c.valid("Camera.Following") {
		return Unit{}
	}
	st := c.state()
	if !st.follow.Valid() {
		st.follow = Unit{}
	}
	return st.follow
}

// StopFollow releases any follow lock.
// JASS: StopCamera, StopCameraForPlayerBJ
func (c Camera) StopFollow() {
	if !c.valid("Camera.StopFollow") || !c.isLocal() {
		return
	}
	c.state().follow = Unit{}
	c.emit(CameraEvent{Kind: CameraStopFollow})
}

// Shake applies a screen shake of the given magnitude (0 stops it). JASS:
// CameraSetSourceNoiseEx / the earthquake BJs collapse here.
// JASS: CameraClearNoiseForPlayer, CameraSetEQNoiseForPlayer, CameraSetSourceNoise, CameraSetSourceNoiseEx, CameraSetSourceNoiseForPlayer, CameraSetTargetNoise, CameraSetTargetNoiseEx, CameraSetTargetNoiseForPlayer
func (c Camera) Shake(magnitude float64) {
	if !c.valid("Camera.Shake") || !c.isLocal() {
		return
	}
	if magnitude < 0 {
		magnitude = 0
	}
	c.emit(CameraEvent{Kind: CameraShake, Magnitude: magnitude})
}

// BeginCinematic enters cinematic mode: subsequent SetField calls bypass the
// R-RND-1 gameplay clamp. JASS: the cinematic-mode setup BJs.
func (c Camera) BeginCinematic() {
	if !c.valid("Camera.BeginCinematic") {
		return
	}
	c.state().cinematic = true
	if c.isLocal() {
		c.emit(CameraEvent{Kind: CameraBeginCinematic})
	}
}

// EndCinematic leaves cinematic mode, restoring gameplay-mode field clamping.
func (c Camera) EndCinematic() {
	if !c.valid("Camera.EndCinematic") {
		return
	}
	c.state().cinematic = false
	if c.isLocal() {
		c.emit(CameraEvent{Kind: CameraEndCinematic})
	}
}

// InCinematic reports whether the camera is in cinematic mode.
func (c Camera) InCinematic() bool {
	return c.valid("Camera.InCinematic") && c.state().cinematic
}

func (c Camera) valid(verb string) bool {
	if !c.Valid() || c.slot() < 0 || c.slot() >= sim.MaxPlayers {
		if c.g != nil {
			c.g.reportInvalid(verb)
		}
		return false
	}
	return true
}
