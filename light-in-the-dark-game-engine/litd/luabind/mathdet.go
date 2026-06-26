package luabind

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// Deterministic transcendental backend for Lua math.* (#391, D-2026-06-19-1).
//
// Go's math.Sin/Cos/Exp/Log/Pow/... are not guaranteed bit-identical across
// OS/arch (platform assembly + permitted FMA contraction, golang/go#20319), which
// would desync a lockstep match and break the G5.7 cross-arch hash gate (#271).
// This backend computes the transcendental CORE through litd/fixed's committed
// integer tables and assembles the exponent/range with deterministic float64
// bit-ops only — math.Frexp/Ldexp/Floor/Mod/Trunc (pure bit manipulation, no
// transcendental asm) and the IEEE-754-mandated correctly-rounded math.Sqrt.
// Basic float64 +,-,*,/ are deterministic under gc (no automatic FMA fusion), so
// every operation here is reproducible on every architecture.
//
// Results are COARSER than stock float64 (trig carries the 1/65536-turn BAM
// resolution; exp/pow the fixed table accuracy) — the accepted determinism trade.

// Math constants as exact float64 literals (compile-time, deterministic).
const (
	twoPi    = 6.283185307179586
	log2e    = 1.4426950408889634
	ln2      = 0.6931471805599453
	log10of2 = 0.30102999566398114
	// expHi/expLo bound the argument to e^x before it overflows/underflows
	// float64 — matching math.Exp's thresholds so range behavior is identical.
	expHi = 709.782712893384
	expLo = -745.1332191019442
)

// detMathBackend is the stateless deterministic backend bound to every litd
// LState (see Register / sandbox setup).
var detMathBackend = &lua.LitdMathFns{
	Sin: detSin, Cos: detCos, Tan: detTan,
	Asin: detAsin, Acos: detAcos, Atan: detAtan, Atan2: detAtan2,
	Sinh: detSinh, Cosh: detCosh, Tanh: detTanh,
	Exp: detExp, Log: detLog, Log10: detLog10, Sqrt: detSqrt, Pow: detPow,
}

// floatToFixed converts x to 32.32 fixed point; out-of-range magnitudes saturate
// to ±MaxF64 (so callers that pass already-reduced values stay exact while
// extreme inputs degrade gracefully rather than wrapping).
func floatToFixed(x float64) fixed.F64 {
	if x >= 2147483648.0 {
		return fixed.MaxF64
	}
	if x <= -2147483648.0 {
		return fixed.MinF64
	}
	return fixed.F64(int64(math.RoundToEven(x * 4294967296.0)))
}

func fixedToFloat(x fixed.F64) float64 { return float64(int64(x)) / 4294967296.0 }

// radToAngle reduces x radians to a BAM Angle deterministically (float64 floor +
// basic ops only). Large |x| reduces with degrading precision but identically on
// every machine.
func radToAngle(x float64) fixed.Angle {
	q := x / twoPi
	r := x - math.Floor(q)*twoPi // [0, 2π)
	scaled := r * (65536.0 / twoPi)
	return fixed.Angle(uint16(int64(scaled)))
}

func detSin(x float64) float64 { return fixedToFloat(radToAngle(x).Sin()) }
func detCos(x float64) float64 { return fixedToFloat(radToAngle(x).Cos()) }

func detTan(x float64) float64 {
	a := radToAngle(x)
	c := fixedToFloat(a.Cos())
	return fixedToFloat(a.Sin()) / c // ±Inf at the table-exact poles (deterministic)
}

// detExp returns e^x via 2^(x*log2e): the fractional power through litd/fixed,
// the integer power through math.Ldexp (exponent-bit op). Range matches math.Exp.
func detExp(x float64) float64 {
	if math.IsNaN(x) {
		return x
	}
	if x > expHi {
		return math.Inf(1)
	}
	if x < expLo {
		return 0
	}
	p := x * log2e
	ip := math.Floor(p)
	fp := p - ip
	base, _ := fixed.Exp2(floatToFixed(fp)) // fp in [0,1) -> [1,2)
	return math.Ldexp(fixedToFloat(base), int(ip))
}

// detLog returns ln(x): exponent from math.Frexp, mantissa log2 from litd/fixed.
func detLog(x float64) float64 {
	switch {
	case math.IsNaN(x) || x < 0:
		return math.NaN()
	case x == 0:
		return math.Inf(-1)
	case math.IsInf(x, 1):
		return math.Inf(1)
	}
	return log2of(x) * ln2
}

func detLog10(x float64) float64 {
	switch {
	case math.IsNaN(x) || x < 0:
		return math.NaN()
	case x == 0:
		return math.Inf(-1)
	case math.IsInf(x, 1):
		return math.Inf(1)
	}
	return log2of(x) * log10of2
}

// log2of returns log2(x) for x > 0 as a float64, deterministically: x = m·2^e
// (Frexp), log2(x) = (e-1) + log2(2m) with the mantissa log2 from litd/fixed.
func log2of(x float64) float64 {
	m, e := math.Frexp(x) // m in [0.5,1), x = m*2^e
	frac := fixedToFloat(fixed.Log2(floatToFixed(m * 2)))
	return float64(e-1) + frac
}

func detSqrt(x float64) float64 {
	if x < 0 {
		return math.NaN()
	}
	return math.Sqrt(x) // IEEE-754 correctly rounded -> already bit-identical cross-arch
}

// signedAngleToRad maps a BAM angle to radians in (-π, π] (signed interpretation).
func signedAngleToRad(a fixed.Angle) float64 { return float64(int16(a)) * (twoPi / 65536.0) }

func detAtan2(y, x float64) float64 {
	return signedAngleToRad(fixed.Atan2(floatToFixed(y), floatToFixed(x)))
}

func detAtan(x float64) float64 { return detAtan2(x, 1) }

func detAsin(v float64) float64 {
	if v < -1 || v > 1 {
		return math.NaN()
	}
	return detAtan2(v, math.Sqrt(1-v*v)) // [-π/2, π/2]
}

func detAcos(v float64) float64 {
	if v < -1 || v > 1 {
		return math.NaN()
	}
	// atan2(sqrt(1-v²), v) is in [0, π]; map unsigned so v=-1 -> π (not -π).
	a := fixed.Atan2(floatToFixed(math.Sqrt(1-v*v)), floatToFixed(v))
	return float64(uint16(a)) * (twoPi / 65536.0)
}

func detSinh(x float64) float64 {
	if x > expHi {
		return math.Inf(1)
	}
	if x < -expHi {
		return math.Inf(-1)
	}
	return (detExp(x) - detExp(-x)) / 2
}

func detCosh(x float64) float64 {
	if x > expHi || x < -expHi {
		return math.Inf(1)
	}
	return (detExp(x) + detExp(-x)) / 2
}

func detTanh(x float64) float64 {
	if x > 20 {
		return 1
	}
	if x < -20 {
		return -1
	}
	ex, enx := detExp(x), detExp(-x)
	return (ex - enx) / (ex + enx)
}

// detPow returns x^y, matching math.Pow's IEEE special cases for the common
// domain. Small integer exponents use exact float64 exponentiation-by-squaring
// (so pow(2,10) is exactly 1024); other cases use e^(y·ln x).
func detPow(x, y float64) float64 {
	switch {
	case y == 0 || x == 1:
		return 1
	case math.IsNaN(x) || math.IsNaN(y):
		return math.NaN()
	case x == 0:
		if y > 0 {
			return 0
		}
		return math.Inf(1) // 0^negative
	}
	if y == math.Trunc(y) && math.Abs(y) <= 64 {
		return ipow(x, int(y)) // exact for small integer powers
	}
	if x > 0 {
		return detExp(y * detLog(x))
	}
	// x < 0: real only for integer y (handled above for |y|<=64); else NaN.
	return math.NaN()
}

// ipow computes x^n for integer n by squaring (float64 mul is deterministic).
func ipow(x float64, n int) float64 {
	if n < 0 {
		return 1 / ipow(x, -n)
	}
	r := 1.0
	b := x
	for n > 0 {
		if n&1 == 1 {
			r *= b
		}
		b *= b
		n >>= 1
	}
	return r
}
