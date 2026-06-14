package sim

// Public draw primitives over the single seeded PRNG (R-SIM-2). Every
// gameplay roll — Go or Lua, internal or script-facing — funnels through
// w.rng so the draw sequence is part of the deterministic, replayable,
// hashed state. These are the canonical forms the public RandomInt/
// RandomFloat/RandomAngle verbs translate one-to-one.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// RandU32 returns the next raw draw from the sim PRNG.
func (w *World) RandU32() uint32 { return w.rng.Uint32() }

// RandRange returns a uniformly-distributed integer in [min, max]
// inclusive. A degenerate range (max <= min) returns min without
// drawing — fail-closed, deterministic.
func (w *World) RandRange(min, max int32) int32 {
	if max <= min {
		return min
	}
	span := uint32(int64(max) - int64(min) + 1)
	return min + int32(w.rng.Uint32()%span)
}

// RandUnit returns a fixed-point value in [0, 1): the raw 32-bit draw
// read as a 32.32 fraction.
func (w *World) RandUnit() fixed.F64 { return fixed.F64(w.rng.Uint32()) }

// RandAngle returns a uniformly-distributed binary angle.
func (w *World) RandAngle() fixed.Angle { return fixed.Angle(uint16(w.rng.Uint32())) }

// RandPointInRect returns a uniformly-distributed point inside the
// rectangle [minx,maxx]×[miny,maxy], drawing X then Y from the sim PRNG.
// A degenerate axis (max <= min) returns that axis's min without drawing
// on it — fail-closed and deterministic, mirroring RandRange. JASS:
// GetRandomLocInRect.
func (w *World) RandPointInRect(minx, miny, maxx, maxy fixed.F64) (x, y fixed.F64) {
	if maxx > minx {
		x = minx + w.RandUnit().Mul(maxx-minx)
	} else {
		x = minx
	}
	if maxy > miny {
		y = miny + w.RandUnit().Mul(maxy-miny)
	} else {
		y = miny
	}
	return x, y
}
