package litd

// Doodad surface (#229, D-2026-06-11-13 "promotion on first touch"). Doodads
// are render-side decoration by default — they cost zero sim memory until a
// script first addresses one. The first mutating touch (Show/Hide/SetAnimation)
// promotes the doodad: it is assigned a sim EntityID and a hashed/saved row.
// Promotion is deterministic (script execution order) and one-way for the match.
//
// A Doodad handle is addressed by its map placement index, so it is a stable,
// copyable value that names the same doodad whether or not it has been promoted.

// Doodad names a map doodad by its placement index. The zero value (and any
// negative index) is invalid.
type Doodad struct {
	placement int32
	g         *Game
}

// Doodad returns a handle to the map doodad at placement index i. The doodad is
// not promoted until first touched; an out-of-range index yields an invalid
// handle (promotion will no-op).
func (g *Game) Doodad(i int) Doodad {
	if g == nil || i < 0 {
		return Doodad{placement: -1}
	}
	return Doodad{placement: int32(i), g: g}
}

// Valid reports whether the handle names a real placement slot.
func (d Doodad) Valid() bool { return d.g != nil && d.placement >= 0 }

// IsZero reports the zero-value handle.
func (d Doodad) IsZero() bool { return d.g == nil && d.placement == 0 }

// Promoted reports whether this doodad has been promoted to a sim entity (i.e.
// touched at least once). Unpromoted doodads carry no sim state.
func (d Doodad) Promoted() bool {
	return d.Valid() && d.g.w.Doodads.PromotedRow(d.placement) >= 0
}

// Show promotes the doodad (first touch) and sets its visibility. No-op on an
// invalid handle or when the promotion pool is exhausted.
func (d Doodad) Show(visible bool) {
	if !d.Valid() {
		if d.g != nil {
			d.g.reportInvalid("Doodad.Show")
		}
		return
	}
	if _, ok := d.g.w.ShowDoodad(d.placement, visible); !ok {
		d.g.reportInvalid("Doodad.Show (promotion pool exhausted)")
	}
}

// Hide is Show(false).
func (d Doodad) Hide() { d.Show(false) }

// SetAnimation promotes the doodad and installs an animation override. No-op on
// an invalid handle or pool exhaustion.
// JASS: SetDoodadAnimation, SetDoodadAnimationBJ, SetDoodadAnimationRect, SetDoodadAnimationRectBJ
func (d Doodad) SetAnimation(anim int) {
	if !d.Valid() {
		if d.g != nil {
			d.g.reportInvalid("Doodad.SetAnimation")
		}
		return
	}
	if anim < 0 {
		anim = 0
	}
	if anim > 0xFFFF {
		anim = 0xFFFF
	}
	if _, ok := d.g.w.SetDoodadAnim(d.placement, uint16(anim)); !ok {
		d.g.reportInvalid("Doodad.SetAnimation (promotion pool exhausted)")
	}
}
