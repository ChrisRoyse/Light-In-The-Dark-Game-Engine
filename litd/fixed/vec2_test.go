package fixed

import (
	"math/big"
	"math/bits"
	"math/rand"
	"testing"
)

// refDistSqLess is the math/big reference: (ax-bx)² + (ay-by)² < r²,
// false for r <= 0 (strict-less, empty interior).
func refDistSqLess(a, b Vec2, r F64) bool {
	if r <= 0 {
		return false
	}
	dx := new(big.Int).Sub(big.NewInt(int64(a.X)), big.NewInt(int64(b.X)))
	dy := new(big.Int).Sub(big.NewInt(int64(a.Y)), big.NewInt(int64(b.Y)))
	sum := new(big.Int).Add(new(big.Int).Mul(dx, dx), new(big.Int).Mul(dy, dy))
	rr := new(big.Int).Mul(big.NewInt(int64(r)), big.NewInt(int64(r)))
	return sum.Cmp(rr) < 0
}

func TestDistSqLessCrossCheckRandom(t *testing.T) {
	rng := rand.New(rand.NewSource(99))
	const N = 300000
	for i := 0; i < N; i++ {
		a := Vec2{F64(rng.Uint64()), F64(rng.Uint64())}
		b := Vec2{F64(rng.Uint64()), F64(rng.Uint64())}
		r := F64(rng.Uint64())
		if got, want := DistSqLess(a, b, r), refDistSqLess(a, b, r); got != want {
			t.Fatalf("DistSqLess(%+v, %+v, %d): got %v want %v", a, b, r, got, want)
		}
	}
	t.Logf("randomized cross-check: %d DistSqLess cases vs math/big — all agree", N)
}

func TestDistSqLessEdgeOverflowingSquare(t *testing.T) {
	// dx alone: MaxF64 - MinF64 → magnitude 2^64-1; dx² ≈ 2^128 — far past int64
	a := Vec2{MaxF64, 0}
	b := Vec2{MinF64, 0}
	dx := deltaMag(a.X, b.X)
	hi, lo := bits.Mul64(dx, dx)
	got := DistSqLess(a, b, MaxF64)
	want := refDistSqLess(a, b, MaxF64)
	t.Logf("edge dx²>int64: dx=|MaxF64-MinF64|=%d, dx² = (hi=%d, lo=%d); big ref=%s; DistSqLess(...,MaxF64) got=%v want=%v",
		dx, hi, lo, new(big.Int).Mul(new(big.Int).SetUint64(dx), new(big.Int).SetUint64(dx)).String(), got, want)
	if got != want {
		t.Fatalf("got %v want %v", got, want)
	}
}

func TestDistSqLessEdgeZeroRadiusSamePoint(t *testing.T) {
	p := Vec2{FromInt(42), FromInt(-7)}
	got := DistSqLess(p, p, 0)
	t.Logf("edge r==0, a==b: DistSqLess = %v (strict-less semantics → false)", got)
	if got {
		t.Fatal("DistSqLess(p, p, 0) must be false (0 < 0 is false)")
	}
}

func TestDistSqLessEdgeNegativeDeltasBoundary(t *testing.T) {
	// a 3-4-5 triangle with negative deltas: dist = 5 exactly
	a := Vec2{FromInt(-3), FromInt(-4)}
	b := Vec2{0, 0}
	exactly := DistSqLess(a, b, FromInt(5))     // 25 < 25 → false
	above := DistSqLess(a, b, FromInt(5)+1)     // 25 < (5+ε)² → true
	t.Logf("edge negative deltas (3-4-5): r=5 → %v (big ref %v); r=5+ε → %v (big ref %v)",
		exactly, refDistSqLess(a, b, FromInt(5)), above, refDistSqLess(a, b, FromInt(5)+1))
	if exactly || !above {
		t.Fatalf("boundary pair wrong: exactly=%v above=%v", exactly, above)
	}
}

func TestDistSqLessEdgeMaxRangeCoords(t *testing.T) {
	corners := []Vec2{
		{MaxF64, MaxF64}, {MinF64, MinF64}, {MaxF64, MinF64}, {MinF64, MaxF64},
	}
	n := 0
	for _, a := range corners {
		for _, b := range corners {
			for _, r := range []F64{0, 1, One, MaxF64} {
				if got, want := DistSqLess(a, b, r), refDistSqLess(a, b, r); got != want {
					t.Fatalf("corners %+v %+v r=%d: got %v want %v", a, b, r, got, want)
				}
				n++
			}
		}
	}
	t.Logf("edge max-range coords: %d corner-pair cases vs math/big — all agree (sum dx²+dy² needs 129 bits at the extreme)", n)
}

func TestVec2OpsValueSemantics(t *testing.T) {
	a := Vec2{FromInt(3), FromInt(4)}
	b := Vec2{FromInt(-1), FromInt(2)}
	if got := a.Add(b); got != (Vec2{FromInt(2), FromInt(6)}) {
		t.Fatalf("Add: %+v", got)
	}
	if got := a.Sub(b); got != (Vec2{FromInt(4), FromInt(2)}) {
		t.Fatalf("Sub: %+v", got)
	}
	if got := a.Scale(FromInt(2)); got != (Vec2{FromInt(6), FromInt(8)}) {
		t.Fatalf("Scale: %+v", got)
	}
	if got := a.Dot(b); got != FromInt(5) { // -3 + 8
		t.Fatalf("Dot: %d", got)
	}
	if got := a.LenSq(); got != FromInt(25) {
		t.Fatalf("LenSq: %d", got)
	}
}

func TestVec2ZeroAllocs(t *testing.T) {
	a := Vec2{FromInt(100), FromInt(200)}
	b := Vec2{FromInt(-50), FromInt(75)}
	ops := map[string]func(){
		"Add": func() { _ = a.Add(b) }, "Sub": func() { _ = a.Sub(b) },
		"Scale": func() { _ = a.Scale(One / 2) }, "Dot": func() { _ = a.Dot(b) },
		"LenSq": func() { _ = a.LenSq() }, "DistSqLess": func() { _ = DistSqLess(a, b, FromInt(1000)) },
	}
	for name, fn := range ops {
		if n := testing.AllocsPerRun(1000, fn); n != 0 {
			t.Fatalf("%s allocates %v/op; R-GC-1 requires 0", name, n)
		}
	}
	t.Log("AllocsPerRun = 0 for Vec2 Add/Sub/Scale/Dot/LenSq/DistSqLess")
}
