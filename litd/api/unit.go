package litd

// unit.go is the canonical units-category surface (jass-mapping/units.md):
// the ~235 common.j unit natives + ~125 blizzard.j BJs collapse onto methods
// of Unit (and Game.CreateUnit). Life/MaxLife/SetLife/Owner live in
// handle_validity.go; the value math and Valid()/IsZero() skeleton in
// handles.go. This file adds the lifecycle and transform verbs.
//
// Every verb keeps WC3's forgiving semantics (R-API-5): a call on an invalid
// or destroyed handle is a silent no-op (debug mode reports the call site),
// getters return the zero value. Reads/writes go straight to the sim component
// stores — the deterministic Source of Truth — never to render (R-API-6).

// UnitType names a bound unit type — the WC3 unit code ('hfoo') resolved to a
// stable ref. Obtain one from Game.UnitType; the zero value is the null type
// (rejected by CreateUnit). Opaque value type (D-2026-06-13 / #361), mirroring
// AbilityRef.
type UnitType struct {
	ref uint16 // typeID + 1; 0 = null
}

// IsZero reports whether this is the null unit type.
func (t UnitType) IsZero() bool { return t.ref == 0 }

// UnitType resolves a unit code (e.g. "hfoo") to its bound type, or the null
// UnitType if the code is unknown or no unit table is bound. JASS: the 'xxxx'
// rawcodes passed to CreateUnit.
func (g *Game) UnitType(code string) UnitType {
	if g == nil || g.w == nil {
		return UnitType{}
	}
	if id, ok := g.w.UnitTypeID(code); ok {
		return UnitType{ref: id + 1}
	}
	return UnitType{}
}

// CreateUnit spawns a unit of type typ for owner at pos facing the given angle,
// returning its handle (the zero Unit on failure — null/unknown type, foreign
// owner, or the unit cap reached). JASS: CreateUnit, CreateUnitAtLoc and the
// CreateUnitAtLocSaveLast family collapse here (D2); the returned handle
// replaces the bj_lastCreatedUnit side channel (GetLastCreatedUnit tombstoned).
//
// Team currently defaults to the owner's player slot (FFA); alliance/team
// assignment lands with players-and-forces (#218), per the #361 decision.
func (g *Game) CreateUnit(owner Player, typ UnitType, pos Vec2, facing Angle) Unit {
	if g == nil || g.w == nil {
		return Unit{}
	}
	if typ.IsZero() {
		g.reportInvalid("Game.CreateUnit (null UnitType)")
		return Unit{}
	}
	if owner.g != g {
		g.reportInvalid("Game.CreateUnit (owner not from this game)")
		return Unit{}
	}
	slot := uint8(owner.idx)
	id, ok := g.w.SpawnFromTable(typ.ref-1, slot, slot, vec(pos))
	if !ok {
		g.reportInvalid("Game.CreateUnit (spawn failed: unit cap or unbound type)")
		return Unit{}
	}
	if r := g.w.Transforms.Row(id); r >= 0 {
		g.w.Transforms.Facing[r] = angleToBrad(facing)
		g.w.MarkSnap(id)
	}
	return Unit{id: id, g: g}
}

// Position returns the unit's current world position, or the zero Vec2 on an
// invalid handle. JASS: GetUnitX/GetUnitY, GetUnitLoc (D3 → one Vec2).
func (u Unit) Position() Vec2 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Position")
		return Vec2{}
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return Vec2{}
	}
	p := u.g.w.Transforms.Pos[r]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
}

// PositionOption configures SetPosition (R-API-3 functional option).
type PositionOption func(*positionConfig)

type positionConfig struct {
	skipPathing bool
}

// Teleport places the unit at the exact target, ignoring pathing and
// collision — the raw SetUnitX/SetUnitY capability. Without it, SetPosition
// respects static pathing and nudges an unpathable target to the nearest
// walkable cell (units.md hazard 3: the capability is preserved, not averaged
// away).
func Teleport() PositionOption {
	return func(c *positionConfig) { c.skipPathing = true }
}

// SetPosition relocates the unit. By default it respects static pathing
// (the SetUnitPosition semantics): a target on an unpathable cell is nudged to
// the nearest walkable cell; an already-pathable target keeps its exact
// coordinates. With Teleport() it places raw (the SetUnitX/SetUnitY teleport
// semantics). No-op on an invalid handle. JASS: SetUnitX, SetUnitY,
// SetUnitPosition, SetUnitPositionLoc all collapse here (D3).
func (u Unit) SetPosition(pos Vec2, opts ...PositionOption) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetPosition")
		return
	}
	var c positionConfig
	for _, o := range opts {
		o(&c)
	}
	u.g.w.PlaceUnit(u.id, vec(pos), c.skipPathing)
}

// Facing returns the unit's facing angle, or the zero Angle on an invalid
// handle. JASS: GetUnitFacing.
func (u Unit) Facing() Angle {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Facing")
		return Angle{}
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return Angle{}
	}
	return angleFromBrad(u.g.w.Transforms.Facing[r])
}

// SetFacing instantly orients the unit to a. This is the snap form
// (SetUnitFacing); the gradual turn-to-face is an order, issued separately.
// No-op on an invalid handle. JASS: SetUnitFacing, SetUnitFacingTimed (the
// timed variant's instant endpoint).
func (u Unit) SetFacing(a Angle) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetFacing")
		return
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return
	}
	u.g.w.Transforms.Facing[r] = angleToBrad(a)
	u.g.w.MarkSnap(u.id) // discontinuity: render must not lerp across the snap
}

// Kill kills the unit (marked this tick; resolved in the sim step, firing the
// death event). A unit already dead or invalid is a no-op. JASS: KillUnit,
// KillUnitBJ (D1 passthrough collapses here).
func (u Unit) Kill() {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Kill")
		return
	}
	u.g.w.KillUnit(u.id)
}

// Remove deletes the unit and all its component rows immediately, with no
// death event — the unit simply ceases to exist (corpse, selection, and
// orders all released). No-op on an invalid handle. JASS: RemoveUnit,
// RemoveUnitBJ.
func (u Unit) Remove() {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Remove")
		return
	}
	u.g.w.DestroyUnit(u.id)
}
