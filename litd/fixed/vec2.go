package fixed

import "math/bits"

// Vec2 is a value-type 2D vector of 32.32 fixed-point components
// (R-API value-type math; no pointer returns anywhere).
type Vec2 struct {
	X, Y F64
}

// Add returns a+b component-wise (wraps like F64.Add).
func (a Vec2) Add(b Vec2) Vec2 { return Vec2{a.X + b.X, a.Y + b.Y} }

// Sub returns a-b component-wise (wraps like F64.Sub).
func (a Vec2) Sub(b Vec2) Vec2 { return Vec2{a.X - b.X, a.Y - b.Y} }

// Scale returns a*s component-wise with F64.Mul semantics.
func (a Vec2) Scale(s F64) Vec2 { return Vec2{a.X.Mul(s), a.Y.Mul(s)} }

// Dot returns a·b with F64.Mul semantics (truncate toward zero, wrap on
// overflow). For range checks use DistSqLess — it never truncates.
func (a Vec2) Dot(b Vec2) F64 { return a.X.Mul(b.X).Add(a.Y.Mul(b.Y)) }

// LenSq returns a·a with F64.Mul semantics. Same caveat as Dot.
func (a Vec2) LenSq() F64 { return a.Dot(a) }

// deltaMag returns |x - y| exactly as uint64. The true difference of two
// int64 values fits in 65 bits signed; its magnitude fits uint64
// (max 2^64-1 for MaxF64 - MinF64). Two's-complement subtraction in
// uint64 yields the exact magnitude once operands are ordered.
func deltaMag(x, y F64) uint64 {
	if x >= y {
		return uint64(x) - uint64(y)
	}
	return uint64(y) - uint64(x)
}

// DistSqLess reports whether the squared distance between a and b is
// strictly less than r squared — exactly, over the full coordinate
// range. Squares are 128-bit via bits.Mul64; the sum dx²+dy² may need
// 129 bits, tracked with the carry from bits.Add64; nothing is ever
// truncated to F64 (determinism.md §2.4).
//
// r is a distance in F64 units; negative r has an empty interior
// (returns false).
func DistSqLess(a, b Vec2, r F64) bool {
	if r <= 0 {
		return false // strict-less: nothing is closer than a non-positive radius
	}
	dxHi, dxLo := bits.Mul64(deltaMag(a.X, b.X), deltaMag(a.X, b.X))
	dyHi, dyLo := bits.Mul64(deltaMag(a.Y, b.Y), deltaMag(a.Y, b.Y))

	sumLo, carry := bits.Add64(dxLo, dyLo, 0)
	sumHi, carry := bits.Add64(dxHi, dyHi, carry)
	if carry != 0 {
		return false // sum ≥ 2^128 > any representable r²
	}

	rMag := magnitude(r)
	rHi, rLo := bits.Mul64(rMag, rMag)

	if sumHi != rHi {
		return sumHi < rHi
	}
	return sumLo < rLo
}
