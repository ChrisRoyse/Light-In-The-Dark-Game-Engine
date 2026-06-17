package litd

// Visibility & fog of war (#243; visibility-and-fog.md). The fogmodifier
// CRUD, SetFogState*, point-visibility query triple, and global toggles
// collapse onto a small surface:
//   - D3: rect/radius/Loc constructor variants → one NewFogModifier with an
//     Area sum type (Rect | Circle); the *BJ/Simple ctors collapse via the
//     Started()/SharedVision() options (D2).
//   - D5: the visible/fogged/masked query triple was one enum all along →
//     FogStateAt, with IsVisibleTo as convenience.
//   - D1: FogEnableOn/Off etc. drop onto SetFogEnabled(bool).
// The visibility grid is sim gameplay state, so every query here is
// answerable headless (R-SIM-4). Fog *display* is render-only and reads the
// local player's grid — not part of this surface.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// FogState is the visibility of a point to a player. It collapses the JASS
// IsVisible/IsFogged/IsMasked boolean triple into one enum.
type FogState uint8

const (
	FogMasked  FogState = iota // never seen — black mask
	FogFogged                  // explored but not currently visible — dimmed
	FogVisible                 // currently in sight
)

// fog state <-> sim grid encoding (sim: FogHidden=0, FogExplored=1, FogVisible=2,
// which matches the FogState ordering above).
func (s FogState) toSim() uint8 { return uint8(s) }
func fogStateFromSim(v uint8) FogState {
	if v > uint8(FogVisible) {
		return FogVisible
	}
	return FogState(v)
}

// Area is the shape of a fog override: Rect or Circle (the D3 sum type that
// collapses the rect/radius/Loc constructor variants).
type Area interface {
	// fogBounds returns the shape kind (0=rect, 1=circle) and its parameters:
	// rect → (minx, miny, maxx, maxy); circle → (cx, cy, radius, _).
	fogBounds() (kind uint8, a, b, c, d float64)
}

func (r Rect) fogBounds() (uint8, float64, float64, float64, float64) {
	return 0, r.MinX, r.MinY, r.MaxX, r.MaxY
}

// Circle is a world-space disc, the radius form of a fog Area.
type Circle struct {
	Center Vec2
	Radius float64
}

func (c Circle) fogBounds() (uint8, float64, float64, float64, float64) {
	return 1, c.Center.X, c.Center.Y, c.Radius, 0
}

// fogOpts collects the optional modifier flags.
type fogOpts struct {
	shared     bool
	afterUnits bool
	started    bool
}

// FogOption configures NewFogModifier.
type FogOption func(*fogOpts)

// SharedVision makes the modifier also apply to players the owner shares
// vision with (JASS useSharedVision). Default off.
func SharedVision(on bool) FogOption { return func(o *fogOpts) { o.shared = on } }

// AfterUnits requests the modifier render after units (JASS afterUnits). This
// is a render-ordering hint only; it does not affect the sim grid. Default off.
func AfterUnits(on bool) FogOption { return func(o *fogOpts) { o.afterUnits = on } }

// Started creates the modifier already running (collapses the enabled-by-default
// CreateFogModifier*BJ constructors). Default: created stopped, call Start().
func Started() FogOption { return func(o *fogOpts) { o.started = true } }

// FogModifier is a handle to a persistent per-player area visibility override.
type FogModifier struct {
	g  *Game
	id sim.FogModifierID
}

// Valid reports whether this modifier handle refers to a live modifier
// (R-API-5). The zero value and a destroyed modifier are invalid.
func (f FogModifier) Valid() bool {
	return f.g != nil && f.g.w != nil && f.g.w.FogModifierValid(f.id)
}

// NewFogModifier creates a fog-state modifier over an area for a player and
// returns its handle. Created stopped unless Started() is passed. Returns the
// zero (invalid) handle on a bad player/area or a full modifier pool.
// JASS: CreateFogModifierRadius, CreateFogModifierRadiusLoc, CreateFogModifierRadiusLocBJ, CreateFogModifierRadiusLocSimple, CreateFogModifierRect, CreateFogModifierRectBJ, CreateFogModifierRectSimple
func (g *Game) NewFogModifier(p Player, state FogState, area Area, opts ...FogOption) FogModifier {
	if g == nil || g.w == nil || !p.Valid() || area == nil {
		return FogModifier{}
	}
	var o fogOpts
	for _, opt := range opts {
		opt(&o)
	}
	kind, a, b, c, d := area.fogBounds()
	var id sim.FogModifierID
	var ok bool
	if kind == 1 { // circle
		id, ok = g.w.CreateFogModifierRadius(uint8(p.idx), state.toSim(), fromFloat(a), fromFloat(b), fromFloat(c), o.shared, o.started)
	} else { // rect
		id, ok = g.w.CreateFogModifierRect(uint8(p.idx), state.toSim(), fromFloat(a), fromFloat(b), fromFloat(c), fromFloat(d), o.shared, o.started)
	}
	if !ok {
		return FogModifier{}
	}
	return FogModifier{g: g, id: id}
}

// Start activates the modifier (FogModifierStart).
// JASS: FogModifierStart
func (f FogModifier) Start() {
	if f.g != nil && f.g.w != nil {
		f.g.w.StartFogModifier(f.id)
	}
}

// Stop deactivates the modifier (FogModifierStop). The grid reverts to
// computed vision at the next update.
// JASS: FogModifierStop
func (f FogModifier) Stop() {
	if f.g != nil && f.g.w != nil {
		f.g.w.StopFogModifier(f.id)
	}
}

// Destroy frees the modifier (DestroyFogModifier). The handle becomes invalid.
// JASS: DestroyFogModifier
func (f FogModifier) Destroy() {
	if f.g != nil && f.g.w != nil {
		f.g.w.DestroyFogModifier(f.id)
	}
}

// SetFogState stamps a fog state over an area immediately, with no modifier
// lifetime (SetFogStateRect/Radius). It is overwritten at the next vision
// update. No-op on a bad player/area.
// JASS: SetFogStateRadius, SetFogStateRadiusLoc, SetFogStateRect
func (g *Game) SetFogState(p Player, state FogState, area Area, sharedVision bool) {
	if g == nil || g.w == nil || !p.Valid() || area == nil {
		return
	}
	kind, a, b, c, d := area.fogBounds()
	if kind == 1 {
		g.w.SetFogStateRadius(uint8(p.idx), state.toSim(), fromFloat(a), fromFloat(b), fromFloat(c), sharedVision)
	} else {
		g.w.SetFogStateRect(uint8(p.idx), state.toSim(), fromFloat(a), fromFloat(b), fromFloat(c), fromFloat(d), sharedVision)
	}
}

// FogStateAt returns a point's visibility to a player (IsVisible/IsFogged/
// IsMasked collapsed). Reads FogMasked for an invalid player.
// JASS: IsFoggedToPlayer, IsLocationFoggedToPlayer, IsLocationMaskedToPlayer, IsMaskedToPlayer
func (g *Game) FogStateAt(p Player, pos Vec2) FogState {
	if g == nil || g.w == nil || !p.Valid() {
		return FogMasked
	}
	return fogStateFromSim(g.w.FogStateAtWorld(uint8(p.idx), vec(pos)))
}

// IsVisibleTo reports whether a point is currently visible to a player
// (IsVisibleToPlayer / IsLocationVisibleToPlayer).
// JASS: IsLocationVisibleToPlayer, IsVisibleToPlayer
func (g *Game) IsVisibleTo(p Player, pos Vec2) bool {
	return g.FogStateAt(p, pos) == FogVisible
}

// SetFogEnabled turns fog of war on or off globally (FogEnable). Off reveals
// the whole map to every player.
// JASS: FogEnable, FogEnableOff, FogEnableOn
func (g *Game) SetFogEnabled(on bool) {
	if g != nil && g.w != nil {
		g.w.SetFogEnabled(on)
	}
}

// FogEnabled reports whether fog of war is on (IsFogEnabled).
// JASS: IsFogEnabled
func (g *Game) FogEnabled() bool { return g != nil && g.w != nil && g.w.FogEnabled() }

// SetFogMaskEnabled turns the black mask on or off (FogMaskEnable). Off makes
// never-seen terrain read as explored instead of masked.
// JASS: FogMaskEnable, FogMaskEnableOff, FogMaskEnableOn
func (g *Game) SetFogMaskEnabled(on bool) {
	if g != nil && g.w != nil {
		g.w.SetFogMaskEnabled(on)
	}
}

// FogMaskEnabled reports whether the black mask is on (IsFogMaskEnabled).
// JASS: IsFogMaskEnabled
func (g *Game) FogMaskEnabled() bool { return g != nil && g.w != nil && g.w.FogMaskEnabled() }

// ShareVision grants or revokes sharing of this unit's sight with a player
// (UnitShareVision). No-op on an invalid unit.
// JASS: UnitShareVision, UnitShareVisionBJ
func (u Unit) ShareVision(p Player, share bool) {
	if !u.Valid() || !p.Valid() {
		return
	}
	u.g.w.SetShareVision(u.id, uint8(p.idx), share)
}
