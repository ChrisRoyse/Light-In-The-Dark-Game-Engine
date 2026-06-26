package fixed

import "math/bits"

// Deterministic base-2 exponential and logarithm in 32.32 fixed point, plus the
// natural/base-10/power functions derived from them (D-2026-06-19-1). All compute
// via committed integer tables (exptable.go) and integer arithmetic — bit
// identical on every OS/arch, no runtime math.*. Accuracy is ~1e-7 relative
// (coarser than float64): the deliberate determinism-over-precision trade.

// expFracBits is log2(expSteps): a [0, 2^32) fraction indexes its top expFracBits
// bits into a table; the low (32-expFracBits) bits drive linear interpolation
// between adjacent entries (integer-exact).
const expFracBits = 12 // 1<<12 == expSteps == 4096

// interpFrac linearly interpolates a generated [expSteps+1]F64 table at fraction
// frac in [0, 2^32).
func interpFrac(t *[expSteps + 1]F64, frac uint64) F64 {
	idx := frac >> (32 - expFracBits)             // [0, expSteps)
	rem := frac & ((1 << (32 - expFracBits)) - 1) // [0, 2^(32-expFracBits))
	a, b := int64(t[idx]), int64(t[idx+1])
	return F64(a + (b-a)*int64(rem)>>(32-expFracBits))
}

// Log2 returns log2(x) as 32.32 fixed point for x > 0. x <= 0 has no real
// logarithm; Log2 returns MinF64 as a sentinel and callers (the Lua bridge) must
// guard the domain and emit the correct IEEE bit pattern.
func Log2(x F64) F64 {
	if x <= 0 {
		return MinF64
	}
	raw := uint64(x)
	msb := bits.Len64(raw) - 1 // highest set bit, 0..62 (x>0 => bit 63 clear)
	e := int64(msb) - 32       // floor(log2(value)), value = raw / 2^32
	// Normalize the mantissa to [2^32, 2^33), i.e. value's mantissa in [1, 2).
	var m uint64
	if msb >= 32 {
		m = raw >> uint(msb-32)
	} else {
		m = raw << uint(32-msb)
	}
	frac := m - (1 << 32) // [0, 2^32): mantissa - 1
	return F64(e<<32) + interpFrac(&log2Mant, frac)
}

// Exp2 returns 2^x as 32.32 fixed point. ok is false when the result overflows
// the 32.32 range (the caller maps !ok to +Inf); values below the resolution
// underflow to (0, true).
func Exp2(x F64) (F64, bool) {
	e := int64(x) >> 32               // floor(x) toward -inf
	frac := uint64(x) - uint64(e)<<32 // [0, 2^32)
	base := uint64(interpFrac(&exp2Frac, frac))
	if e >= 0 {
		sh := uint(e)
		if sh > uint(bits.LeadingZeros64(base))-1 { // would set the sign bit
			return 0, false // overflow -> +Inf
		}
		return F64(base << sh), true
	}
	sh := uint(-e)
	if sh >= 64 {
		return 0, true // underflow to 0
	}
	return F64(base >> sh), true
}

// Exp returns e^x as 32.32 fixed point. ok is false on overflow (+Inf).
func Exp(x F64) (F64, bool) { return Exp2(x.Mul(log2eF64)) }

// Log returns ln(x) for x > 0 (see Log2 for the domain contract).
func Log(x F64) F64 { return Log2(x).Mul(ln2F64) }

// Log10 returns log10(x) for x > 0 (see Log2 for the domain contract).
func Log10(x F64) F64 { return Log2(x).Mul(log10_2F64) }

// Pow returns x^y for x > 0 via 2^(y*log2(x)). ok is false on overflow (+Inf).
// x <= 0 is the bridge's responsibility (integer-exponent and domain rules); Pow
// returns (0, true) defensively.
func Pow(x, y F64) (F64, bool) {
	if x <= 0 {
		return 0, true
	}
	return Exp2(y.Mul(Log2(x)))
}
