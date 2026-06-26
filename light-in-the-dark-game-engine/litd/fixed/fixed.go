package fixed

import "math/bits"

// F64 is a 32.32 signed fixed-point number. See doc.go for the frozen
// semantics contract.
type F64 int64

const (
	// One is the fixed-point representation of 1.
	One F64 = 1 << 32
	// MaxF64 is the largest representable value (≈ 2^31).
	MaxF64 F64 = 1<<63 - 1
	// MinF64 is the smallest (most negative) representable value.
	MinF64 F64 = -1 << 63
)

// FromInt converts an int32 to fixed-point exactly.
func FromInt(i int32) F64 { return F64(i) << 32 }

// Floor returns the largest integer ≤ a (floors toward -inf).
func (a F64) Floor() int64 { return int64(a >> 32) }

// Add returns a+b with two's-complement wrap on overflow.
func (a F64) Add(b F64) F64 { return a + b }

// Sub returns a-b with two's-complement wrap on overflow.
func (a F64) Sub(b F64) F64 { return a - b }

// Neg returns -a. Neg(MinF64) wraps to MinF64.
func (a F64) Neg() F64 { return -a }

// Abs returns |a|. Abs(MinF64) wraps to MinF64.
func (a F64) Abs() F64 {
	if a < 0 {
		return -a
	}
	return a
}

// magnitude returns |v| as uint64; correct for MinInt64 because the
// two's-complement wrap of -MinInt64 reinterpreted as uint64 is 2^63.
func magnitude(v F64) uint64 {
	if v < 0 {
		return uint64(-v)
	}
	return uint64(v)
}

// Mul returns a*b: exact 128-bit magnitude product, >>32, truncated
// toward zero, sign reapplied. Overflow keeps the low 64 bits (wrap).
func (a F64) Mul(b F64) F64 {
	hi, lo := bits.Mul64(magnitude(a), magnitude(b))
	r := hi<<32 | lo>>32
	if (a < 0) != (b < 0) {
		return F64(-int64(r))
	}
	return F64(r)
}

// Div returns a/b truncated toward zero. Panics if b == 0 or if the
// scaled quotient magnitude does not fit (bits.Div64 fail-fast).
func (a F64) Div(b F64) F64 {
	na, nb := magnitude(a), magnitude(b)
	q, _ := bits.Div64(na>>32, na<<32, nb)
	if (a < 0) != (b < 0) {
		return F64(-int64(q))
	}
	return F64(q)
}
