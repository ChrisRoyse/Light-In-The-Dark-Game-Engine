package fixed

import (
	"math"
	"math/big"
	"math/rand"
	"testing"
)

// ratOf returns the exact rational value of an F64.
func ratOf(a F64) *big.Rat {
	return new(big.Rat).SetFrac(big.NewInt(int64(a)), new(big.Int).Lsh(big.NewInt(1), 32))
}

// refMul computes the reference product truncated toward zero at 32.32,
// with wrap to 64 bits — the documented contract.
func refMul(a, b F64) F64 {
	prod := new(big.Int).Mul(big.NewInt(int64(a)), big.NewInt(int64(b)))
	neg := prod.Sign() < 0
	prod.Abs(prod)
	prod.Rsh(prod, 32) // truncate toward zero on the magnitude
	// wrap magnitude to low 64 bits
	prod.And(prod, new(big.Int).SetUint64(math.MaxUint64))
	r := F64(prod.Uint64())
	if neg {
		r = -r
	}
	return r
}

func refDivFits(a, b F64) (F64, bool) {
	if b == 0 {
		return 0, false
	}
	num := new(big.Int).Lsh(new(big.Int).Abs(big.NewInt(int64(a))), 32)
	den := new(big.Int).Abs(big.NewInt(int64(b)))
	q := new(big.Int).Quo(num, den)
	if q.BitLen() > 64 {
		return 0, false // bits.Div64 would panic (quotient overflow)
	}
	r := F64(q.Uint64())
	if (a < 0) != (b < 0) {
		r = -r
	}
	return r, true
}

var boundary = []F64{
	0, 1, -1, One, -One, One / 2, -One / 2, One - 1, -(One - 1),
	FromInt(1), FromInt(-1), FromInt(2), FromInt(-2),
	FromInt(math.MaxInt32), FromInt(math.MinInt32),
	MaxF64, MinF64, MaxF64 - 1, MinF64 + 1,
	F64(0x123456789ABCDEF0), F64(-0x123456789ABCDEF0),
}

func TestMulCrossCheckBoundary(t *testing.T) {
	n := 0
	for _, a := range boundary {
		for _, b := range boundary {
			got := a.Mul(b)
			want := refMul(a, b)
			if got != want {
				t.Fatalf("Mul(%d, %d): got %d want %d", a, b, got, want)
			}
			n++
		}
	}
	t.Logf("boundary cross-check: %d Mul cases vs math/big — all exact", n)
}

func TestMulCrossCheckRandom(t *testing.T) {
	rng := rand.New(rand.NewSource(42))
	const N = 200000
	for i := 0; i < N; i++ {
		a, b := F64(rng.Uint64()), F64(rng.Uint64())
		if got, want := a.Mul(b), refMul(a, b); got != want {
			t.Fatalf("Mul(%d, %d): got %d want %d", a, b, got, want)
		}
	}
	t.Logf("randomized cross-check: %d Mul cases vs math/big — all exact", N)
}

func TestDivCrossCheckRandom(t *testing.T) {
	rng := rand.New(rand.NewSource(43))
	const N = 200000
	checked := 0
	for i := 0; i < N; i++ {
		a, b := F64(rng.Uint64()), F64(rng.Uint64())
		want, fits := refDivFits(a, b)
		if !fits {
			continue
		}
		if got := a.Div(b); got != want {
			t.Fatalf("Div(%d, %d): got %d want %d", a, b, got, want)
		}
		checked++
	}
	t.Logf("randomized cross-check: %d Div cases vs math/big — all exact", checked)
}

func TestEdgeMulMinInt64ByNegOne(t *testing.T) {
	// documented wrap path: |MinF64| = 2^63; product 2^63 * 2^32 >> 32 = 2^63
	// → low 64 bits = 2^63; sign positive→ wraps to MinF64 when negated... walk it:
	a, b := MinF64, -One
	got := a.Mul(b)
	want := refMul(a, b)
	t.Logf("edge Mul(MinF64=%d, -One=%d) = %d (reference %d, documented wrap policy)", a, b, got, want)
	if got != want {
		t.Fatalf("got %d want %d", got, want)
	}
}

func TestEdgeDivByZeroPanics(t *testing.T) {
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("Div by zero did not panic; documented contract is fail-fast panic")
		}
		t.Logf("edge Div(One, 0) panicked as documented: %v", r)
	}()
	_ = One.Div(0)
}

func TestEdgeDivQuotientOverflowPanics(t *testing.T) {
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("Div quotient overflow did not panic; documented contract is fail-fast panic")
		}
		t.Logf("edge Div(MaxF64, 1) (quotient ≈ 2^95) panicked as documented: %v", r)
	}()
	_ = MaxF64.Div(1) // |MaxF64|<<32 / 1 ≈ 2^95 — cannot fit
}

func TestEdgeNegativeMulRoundsTowardZero(t *testing.T) {
	// -1.5 * 0.25 = -0.375 → toward zero at 2^-32 granularity
	a := -(One + One/2) // -1.5
	b := One / 4        // 0.25
	got := a.Mul(b)
	want := refMul(a, b)
	t.Logf("edge negative rounding: Mul(-1.5, 0.25) raw got=%d want=%d (truncate toward zero)", got, want)
	if got != want {
		t.Fatalf("got %d want %d", got, want)
	}
	// and an inexact case: -(1/3)*3 — verify truncation direction
	third := One.Div(FromInt(3)) // raw 0x55555555 ≈ 0.333…
	gotN := third.Neg().Mul(FromInt(3))
	wantN := refMul(third.Neg(), FromInt(3))
	t.Logf("edge: -(1/3)*3 raw got=%d want=%d (-(One-1) = -0.99999…, truncated toward zero, expected ≠ -One)", gotN, wantN)
	if gotN != wantN {
		t.Fatalf("got %d want %d", gotN, wantN)
	}
	if int64(gotN) != -(int64(One) - 1) {
		t.Fatalf("-(1/3)*3 = %d, want -(One-1) = %d", gotN, -(int64(One) - 1))
	}
}

func TestEdge128BitPathExact(t *testing.T) {
	// operands whose naive int64 product overflows but whose true result fits
	a := FromInt(1 << 20)     // 2^20
	b := FromInt(1 << 10)     // 2^10
	got := a.Mul(b)           // 2^30 — fits comfortably
	want := FromInt(1 << 30)
	t.Logf("edge 128-bit path: Mul(2^20, 2^10) got=%d want=%d (naive int64 product would overflow: raw operands 2^52, 2^42)", got, want)
	if got != want {
		t.Fatalf("got %d want %d", got, want)
	}
}

func TestFloorTowardNegInf(t *testing.T) {
	cases := []struct {
		in   F64
		want int64
	}{
		{One / 2, 0}, {-One / 2, -1}, {FromInt(3), 3}, {FromInt(-3), -3},
		{-One - One/2, -2}, {0, 0},
	}
	for _, c := range cases {
		if got := c.in.Floor(); got != c.want {
			t.Fatalf("Floor(%d): got %d want %d", c.in, got, c.want)
		}
	}
}

func TestAddSubNegAbsWrap(t *testing.T) {
	if MaxF64.Add(1) != MinF64 {
		t.Fatal("Add overflow must wrap")
	}
	if MinF64.Sub(1) != MaxF64 {
		t.Fatal("Sub overflow must wrap")
	}
	if MinF64.Neg() != MinF64 {
		t.Fatal("Neg(MinF64) must wrap to MinF64")
	}
	if MinF64.Abs() != MinF64 {
		t.Fatal("Abs(MinF64) must wrap to MinF64")
	}
	if FromInt(-7).Abs() != FromInt(7) {
		t.Fatal("Abs(-7) != 7")
	}
}

func TestZeroAllocs(t *testing.T) {
	a, b := FromInt(12345), FromInt(-678)
	ops := map[string]func(){
		"Add": func() { _ = a.Add(b) }, "Sub": func() { _ = a.Sub(b) },
		"Neg": func() { _ = a.Neg() }, "Abs": func() { _ = a.Abs() },
		"Mul": func() { _ = a.Mul(b) }, "Div": func() { _ = a.Div(b) },
		"Floor": func() { _ = a.Floor() },
	}
	for name, fn := range ops {
		if n := testing.AllocsPerRun(1000, fn); n != 0 {
			t.Fatalf("%s allocates %v/op; R-GC-1 requires 0", name, n)
		}
	}
	t.Log("AllocsPerRun = 0 for Add/Sub/Neg/Abs/Mul/Div/Floor (R-GC-1)")
}

func BenchmarkMul(b *testing.B) {
	x, y := FromInt(31415), One/7
	var r F64
	for i := 0; i < b.N; i++ {
		r = x.Mul(y)
	}
	_ = r
}

func BenchmarkDiv(b *testing.B) {
	x, y := FromInt(31415), One*7
	var r F64
	for i := 0; i < b.N; i++ {
		r = x.Div(y)
	}
	_ = r
}
