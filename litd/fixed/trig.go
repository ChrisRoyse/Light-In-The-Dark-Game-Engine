package fixed

import "math/bits"

//go:generate go run ./gen

// Angle is a binary angular measurement: 1/65536 of a full turn.
// Wrap-around is free (uint16 overflow), comparison is exact, and π
// never appears in the API (determinism.md §2.4).
type Angle uint16

const (
	quarterTurn Angle = 0x4000
	halfTurn    Angle = 0x8000
)

// Sin returns sin(a) as 32.32 fixed point, via the committed
// quarter-wave table (sinQuarter, generated source — no runtime math.*).
func (a Angle) Sin() F64 {
	quadrant := a >> 14    // 0..3
	idx := int(a & 0x3FFF) // position within quarter wave
	switch quadrant {
	case 0:
		return sinQuarter[idx]
	case 1:
		return sinQuarter[quarterSteps-idx]
	case 2:
		return -sinQuarter[idx]
	default:
		return -sinQuarter[quarterSteps-idx]
	}
}

// Cos returns cos(a) as 32.32 fixed point: cos(a) = sin(a + quarter turn).
func (a Angle) Cos() F64 { return (a + quarterTurn).Sin() }

// tanLessEq reports tan(a) <= y/x for a in [0, quarterTurn], x > 0,
// y >= 0 — compared as sin(a)·x <= cos(a)·y in exact 128 bits (no
// division, no truncation; determinism.md §2.4).
func tanLessEq(a Angle, y, x uint64) bool {
	sHi, sLo := bits.Mul64(uint64(a.Sin()), x)
	cHi, cLo := bits.Mul64(uint64(a.Cos()), y)
	if sHi != cHi {
		return sHi < cHi
	}
	return sLo <= cLo
}

// Atan2 returns the BAM angle of the vector (x, y): the a for which
// (Cos(a), Sin(a)) points along it. Angle 0 = +X, quarter turn = +Y.
// Atan2(0, 0) = 0 (deterministic convention). Implementation is a
// binary search over the committed sine table inside the first
// quadrant, then quadrant mapping by sign — integer-exact, identical
// on every machine.
func Atan2(y, x F64) Angle {
	if x == 0 && y == 0 {
		return 0
	}
	ym, xm := magnitude(y), magnitude(x)
	// largest a in [0, quarterTurn] with tan(a) <= ym/xm (monotone
	// inside the quadrant); x == 0 is straight up.
	var a Angle
	if xm == 0 {
		a = quarterTurn
	} else {
		lo, hi := Angle(0), quarterTurn
		for lo < hi {
			mid := (lo + hi + 1) / 2
			if tanLessEq(mid, ym, xm) {
				lo = mid
			} else {
				hi = mid - 1
			}
		}
		a = lo
	}
	switch {
	case x >= 0 && y >= 0:
		return a
	case x < 0 && y >= 0:
		return halfTurn - a
	case x < 0 && y < 0:
		return halfTurn + a
	default:
		return -a
	}
}

// TurnToward rotates cur toward want by at most rate, always along
// the shorter arc. An exact 180° request turns counterclockwise
// (+rate) — the deterministic tie rule.
func TurnToward(cur, want, rate Angle) Angle {
	diff := want - cur // unsigned wrap: counterclockwise distance
	if diff == 0 {
		return cur
	}
	if diff <= halfTurn { // counterclockwise is shorter (or tied)
		if diff <= rate {
			return want
		}
		return cur + rate
	}
	// clockwise is shorter
	if -diff <= rate {
		return want
	}
	return cur - rate
}
