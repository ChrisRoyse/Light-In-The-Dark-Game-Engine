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
