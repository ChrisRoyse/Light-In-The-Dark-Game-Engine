package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// inRangeOfPoint reports whether the unit's center is within rng of point
// (inclusive boundary), matching the engine's acquisition convention
// (center-to-center; not WC3's edge-to-edge — collision radii are not
// subtracted). False on a unit with no transform row or a negative range.
func (w *World) inRangeOfPoint(id EntityID, point fixed.Vec2, rng fixed.F64) bool {
	r := w.Transforms.Row(id)
	if r == -1 || rng < 0 {
		return false
	}
	dHi, dLo := fixed.DistSq(w.Transforms.Pos[r], point)
	rHi, rLo := fixed.RadiusSq(rng)
	if dHi != rHi {
		return dHi < rHi
	}
	return dLo <= rLo // inclusive: exactly at range counts as in range
}

// UnitsInRange reports whether two units' centers are within rng of each other
// (IsUnitInRange). False if either has no transform row or rng is negative.
func (w *World) UnitsInRange(a, b EntityID, rng fixed.F64) bool {
	br := w.Transforms.Row(b)
	if br == -1 {
		return false
	}
	return w.inRangeOfPoint(a, w.Transforms.Pos[br], rng)
}

// UnitInRangeOfPoint reports whether the unit is within rng of a world point
// (IsUnitInRangeXY / IsUnitInRangeLoc).
func (w *World) UnitInRangeOfPoint(id EntityID, point fixed.Vec2, rng fixed.F64) bool {
	return w.inRangeOfPoint(id, point, rng)
}
