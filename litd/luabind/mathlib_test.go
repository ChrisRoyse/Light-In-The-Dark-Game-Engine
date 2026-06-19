package luabind_test

// #391 FSV — Lua math.* transcendentals route through deterministic litd/fixed
// (D-2026-06-19-1), NOT Go's math.* (not bit-identical across arch). SoT = the
// IEEE-754 bit pattern (math.Float64bits) of each result, committed as a golden so
// the cross-OS/arch CI matrix (#271/#284) checks bit-identity, plus the IEEE
// special-value bits for domain/range edges, plus fail-closed behavior when no
// backend is bound.

import (
	"fmt"
	"math"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// evalNum runs `return <expr>` on a backend-bound state and returns the result.
func evalNum(t *testing.T, expr string) float64 {
	t.Helper()
	L := luabind.NewState()
	defer L.Close()
	if err := L.DoString("__r = " + expr); err != nil {
		t.Fatalf("eval %q: %v", expr, err)
	}
	n, ok := L.GetGlobal("__r").(lua.LNumber)
	if !ok {
		t.Fatalf("eval %q: result not a number", expr)
	}
	return float64(n)
}

// mathGolden is the committed cross-arch reference: math.Float64bits of each
// deterministic result on this engine. Regenerate via TestPrintMathGoldenBits;
// any change requires an explicit, justified rebump (the CI matrix #284 checks
// these are identical on linux/windows/macos × amd64/arm64).
var mathGolden = map[string]uint64{
	"math.sin(0.5)":    0x3fdeaea5b1c00000, // 0.479409621796
	"math.cos(0.5)":    0x3fec153a42800000, // 0.877591256984
	"math.tan(0.5)":    0x3fe17b1df8d52c5b, // 0.546278940202
	"math.asin(0.5)":   0x3fe0c10f32e492b0, // 0.523566817665
	"math.acos(0.5)":   0x3ff0c10f32e492b0, // 1.04713363533
	"math.atan(0.5)":   0x3fddac5efc3d7e3e, // 0.463645693138
	"math.atan2(1,2)":  0x3fddac5efc3d7e3e, // 0.463645693138
	"math.sinh(1)":     0x3ff2cd9fc5380000, // 1.17520119704
	"math.cosh(1)":     0x3ff8b07553080000, // 1.54308063921
	"math.tanh(1)":     0x3fe85efab5192754, // 0.761594155986
	"math.exp(2)":      0x401d8e64ba800000, // 7.38905612379
	"math.log(10)":     0x40026bb1bbb43259, // 2.30258509296
	"math.log10(1000)": 0x400800000000edb6, // 3.00000000003
	"math.sqrt(2)":     0x3ff6a09e667f3bcd, // 1.41421356237
	"math.pow(2,10)":   0x4090000000000000, // 1024
	"math.pow(2,0.5)":  0x3ff6a09e66800000, // 1.41421356238
}

var mathGoldenExprs = []string{
	"math.sin(0.5)", "math.cos(0.5)", "math.tan(0.5)",
	"math.asin(0.5)", "math.acos(0.5)", "math.atan(0.5)", "math.atan2(1,2)",
	"math.sinh(1)", "math.cosh(1)", "math.tanh(1)",
	"math.exp(2)", "math.log(10)", "math.log10(1000)",
	"math.sqrt(2)", "math.pow(2,10)", "math.pow(2,0.5)",
}

// TestMathGoldenBitsAndDeterminism logs each result's bit pattern and asserts the
// value re-computes bit-identically (determinism on this arch; the committed
// goldens below are the cross-arch reference the CI matrix verifies).
func TestMathGoldenBitsAndDeterminism(t *testing.T) {
	for _, expr := range mathGoldenExprs {
		v1 := evalNum(t, expr)
		v2 := evalNum(t, expr)
		b1, b2 := math.Float64bits(v1), math.Float64bits(v2)
		if b1 != b2 {
			t.Errorf("%s nondeterministic: %#016x != %#016x", expr, b1, b2)
		}
		want, ok := mathGolden[expr]
		if !ok {
			t.Errorf("%s no golden committed (got %#016x = %.12g)", expr, b1, v1)
			continue
		}
		if b1 != want {
			t.Errorf("%s = %#016x (%.12g), golden %#016x — bit drift (intentional? rebump)", expr, b1, v1, want)
		}
	}
}

// TestMathAccuracy — sanity: the deterministic results stay close to stock math.*
// (coarser, not exact). Generous bounds: this only catches gross errors.
func TestMathAccuracy(t *testing.T) {
	cases := []struct {
		expr string
		want float64
		tol  float64
	}{
		{"math.sin(0.5)", math.Sin(0.5), 1e-4},
		{"math.cos(0.5)", math.Cos(0.5), 1e-4},
		{"math.exp(2)", math.Exp(2), 1e-3},
		{"math.log(10)", math.Log(10), 1e-5},
		{"math.log10(1000)", 3, 1e-5},
		{"math.sqrt(2)", math.Sqrt2, 1e-12}, // sqrt is IEEE-exact
		{"math.pow(2,10)", 1024, 1e-9},      // integer power exact
		{"math.atan2(1,2)", math.Atan2(1, 2), 1e-4},
		{"math.asin(0.5)", math.Asin(0.5), 1e-4},
		{"math.acos(0.5)", math.Acos(0.5), 1e-4},
		{"math.tanh(1)", math.Tanh(1), 1e-3},
	}
	for _, c := range cases {
		got := evalNum(t, c.expr)
		if d := math.Abs(got - c.want); d > c.tol {
			t.Errorf("%s = %.12g, want ≈ %.12g, |Δ| = %.2e > %.0e", c.expr, got, c.want, d, c.tol)
		}
	}
}

// TestMathEdgeCases — domain/range edges reproduce IEEE special-value semantics.
func TestMathEdgeCases(t *testing.T) {
	bits := func(expr string) (float64, uint64) {
		v := evalNum(t, expr)
		return v, math.Float64bits(v)
	}
	// sqrt(-1) = NaN
	if v, _ := bits("math.sqrt(-1)"); !math.IsNaN(v) {
		t.Errorf("sqrt(-1) = %v, want NaN", v)
	}
	// log(0) = -Inf
	if v, _ := bits("math.log(0)"); !math.IsInf(v, -1) {
		t.Errorf("log(0) = %v, want -Inf", v)
	}
	// log(-1) = NaN
	if v, _ := bits("math.log(-1)"); !math.IsNaN(v) {
		t.Errorf("log(-1) = %v, want NaN", v)
	}
	// exp(1000) overflows to +Inf
	if v, _ := bits("math.exp(1000)"); !math.IsInf(v, 1) {
		t.Errorf("exp(1000) = %v, want +Inf", v)
	}
	// exp(-1000) underflows to 0
	if v := evalNum(t, "math.exp(-1000)"); v != 0 {
		t.Errorf("exp(-1000) = %v, want 0", v)
	}
	// pow(0,0) = 1; pow(2,10) = 1024 exact; pow(-2,0.5) = NaN; pow(-2,3) = -8
	if v := evalNum(t, "math.pow(0,0)"); v != 1 {
		t.Errorf("pow(0,0) = %v, want 1", v)
	}
	if v := evalNum(t, "math.pow(2,10)"); v != 1024 {
		t.Errorf("pow(2,10) = %v, want exactly 1024", v)
	}
	if v := evalNum(t, "math.pow(-2,3)"); v != -8 {
		t.Errorf("pow(-2,3) = %v, want -8", v)
	}
	if v := evalNum(t, "math.pow(-2,0.5)"); !math.IsNaN(v) {
		t.Errorf("pow(-2,0.5) = %v, want NaN", v)
	}
	// sin(1e10) is deterministic (huge-arg reduction); just assert reproducible.
	if evalNum(t, "math.sin(1e10)") != evalNum(t, "math.sin(1e10)") {
		t.Error("sin(1e10) not deterministic")
	}
}

// TestMathFailsClosedWhenUnbound — a raw fork state with NO backend bound must
// raise a loud error on a transcendental, never silently fall back to Go math.
func TestMathFailsClosedWhenUnbound(t *testing.T) {
	L := lua.NewState() // raw: SkipOpenLibs false, but NO SetMathBackend
	defer L.Close()
	err := L.DoString("return math.sin(0.5)")
	if err == nil {
		t.Fatal("math.sin with no backend bound must fail closed, got nil error")
	}
	if !strings.Contains(err.Error(), "deterministic backend") {
		t.Fatalf("expected a fail-closed backend error, got: %v", err)
	}
	t.Logf("FSV #391 fail-closed: unbound math.sin → %v", err)
}

// printMathBits is a helper to (re)derive the golden table; run with -v.
func TestPrintMathGoldenBits(t *testing.T) {
	var b strings.Builder
	for _, expr := range mathGoldenExprs {
		v := evalNum(t, expr)
		fmt.Fprintf(&b, "\t%q: %#016x, // %.12g\n", expr, math.Float64bits(v), v)
	}
	t.Logf("golden table:\n%s", b.String())
}
