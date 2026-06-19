package fixed

import (
	"math"
	"testing"
)

func f2f(x F64) float64 { return float64(int64(x)) / 4294967296.0 }
func toF(x float64) F64  { return F64(int64(math.RoundToEven(x * 4294967296.0))) }

// TestLog2Accuracy — Log2 tracks math.Log2 within table+fixed tolerance, and is
// bit-identical run-to-run (integer-deterministic by construction).
func TestLog2Accuracy(t *testing.T) {
	for _, x := range []float64{0.5, 1, 1.5, 2, 2.718281828, 3, 7, 10, 16, 100, 1000, 1e6, 0.001, 2e9} {
		got := f2f(Log2(toF(x)))
		want := math.Log2(x)
		if d := math.Abs(got - want); d > 1e-6 {
			t.Errorf("Log2(%g) = %.10f, math = %.10f, |Δ| = %.2e > 1e-6", x, got, want, d)
		}
		if Log2(toF(x)) != Log2(toF(x)) {
			t.Fatalf("Log2(%g) not deterministic", x)
		}
	}
}

func TestLnLog10Accuracy(t *testing.T) {
	for _, x := range []float64{0.5, 1, 2, 2.718281828, 10, 100, 1e6} {
		if d := math.Abs(f2f(Log(toF(x))) - math.Log(x)); d > 1e-6 {
			t.Errorf("Log(%g) |Δ|=%.2e > 1e-6", x, d)
		}
		if d := math.Abs(f2f(Log10(toF(x))) - math.Log10(x)); d > 1e-6 {
			t.Errorf("Log10(%g) |Δ|=%.2e > 1e-6", x, d)
		}
	}
}

func TestExp2Accuracy(t *testing.T) {
	for _, x := range []float64{-20, -10, -3.5, -1, 0, 0.5, 1, 3.3, 10, 20, 30} {
		got, ok := Exp2(toF(x))
		if !ok {
			t.Errorf("Exp2(%g) unexpected overflow", x)
			continue
		}
		want := math.Exp2(x)
		if rel := math.Abs(f2f(got)-want) / math.Max(want, 1e-9); rel > 1e-5 {
			t.Errorf("Exp2(%g) = %.8g, math = %.8g, rel = %.2e > 1e-5", x, f2f(got), want, rel)
		}
	}
	// Overflow: 2^31 leaves the 32.32 range.
	if _, ok := Exp2(toF(31.5)); ok {
		t.Error("Exp2(31.5) should overflow (ok=false)")
	}
}

func TestExpPowAccuracy(t *testing.T) {
	for _, x := range []float64{-10, -1, 0, 1, 2, 5, 10, 15, 20} {
		got, ok := Exp(toF(x))
		if !ok {
			t.Errorf("Exp(%g) unexpected overflow", x)
			continue
		}
		want := math.Exp(x)
		if rel := math.Abs(f2f(got)-want) / math.Max(want, 1e-9); rel > 1e-5 {
			t.Errorf("Exp(%g) rel = %.2e > 1e-5", x, rel)
		}
	}
	// Exp overflow above ln(2^31) ≈ 21.49.
	if _, ok := Exp(toF(25)); ok {
		t.Error("Exp(25) should overflow")
	}
	for _, c := range []struct{ x, y float64 }{{2, 10}, {3, 5}, {10, 3}, {1.5, 8}, {2, 0.5}, {9, 0.5}, {5, -2}} {
		got, ok := Pow(toF(c.x), toF(c.y))
		if !ok {
			t.Errorf("Pow(%g,%g) unexpected overflow", c.x, c.y)
			continue
		}
		want := math.Pow(c.x, c.y)
		if rel := math.Abs(f2f(got)-want) / math.Max(want, 1e-9); rel > 1e-4 {
			t.Errorf("Pow(%g,%g) = %.8g, math = %.8g, rel = %.2e > 1e-4", c.x, c.y, f2f(got), want, rel)
		}
	}
}
