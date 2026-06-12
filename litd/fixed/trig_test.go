package fixed

import (
	"math"
	"math/rand"
	"testing"
)

func TestSqrtEdgeValues(t *testing.T) {
	cases := []struct {
		in   uint64
		want uint32
	}{
		{0, 0}, {1, 1}, {2, 1}, {3, 1}, {4, 2},
		{math.MaxUint64, 4294967295},
	}
	for _, c := range cases {
		got := SqrtU64(c.in)
		t.Logf("edge SqrtU64(%d) = %d", c.in, got)
		if got != c.want {
			t.Fatalf("SqrtU64(%d): got %d want %d", c.in, got, c.want)
		}
	}
}

func sqrtPropertyHolds(v uint64) bool {
	r := uint64(SqrtU64(v))
	if r*r > v {
		return false
	}
	// (r+1)^2 > v, guarding overflow at r = 2^32-1
	if r == math.MaxUint32 {
		return true // (2^32)^2 = 2^64 > any uint64
	}
	return (r+1)*(r+1) > v
}

func TestSqrtPerfectSquareBoundaries(t *testing.T) {
	rng := rand.New(rand.NewSource(7))
	const N = 100000
	for i := 0; i < N; i++ {
		k := uint64(rng.Uint32())
		sq := k * k
		for _, v := range []uint64{sq, sq - 1, sq + 1} {
			if sq == 0 && v == math.MaxUint64 {
				continue // sq-1 underflow for k=0
			}
			if !sqrtPropertyHolds(v) {
				t.Fatalf("floor-sqrt property fails at v=%d (k=%d)", v, k)
			}
		}
		if SqrtU64(sq) != uint32(k) {
			t.Fatalf("SqrtU64(%d) != %d", sq, k)
		}
	}
	k := uint64(123456789)
	t.Logf("sample: k=%d k²=%d → SqrtU64(k²)=%d, SqrtU64(k²-1)=%d, SqrtU64(k²+1)=%d",
		k, k*k, SqrtU64(k*k), SqrtU64(k*k-1), SqrtU64(k*k+1))
	t.Logf("property r*r<=v<(r+1)*(r+1) held for %d randomized k (k², k²−1, k²+1 each)", N)
}

func TestSqrtRandomProperty(t *testing.T) {
	rng := rand.New(rand.NewSource(8))
	const N = 200000
	for i := 0; i < N; i++ {
		v := rng.Uint64()
		if !sqrtPropertyHolds(v) {
			t.Fatalf("floor-sqrt property fails at v=%d", v)
		}
	}
	t.Logf("property held for %d random uint64", N)
}

func TestAngleSymmetryExhaustive(t *testing.T) {
	for i := 0; i < 65536; i++ {
		a := Angle(i)
		if a.Sin() != -(a + halfTurn).Sin() {
			t.Fatalf("Sin(%#04x) != -Sin(+half turn): %d vs %d", i, a.Sin(), -(a + halfTurn).Sin())
		}
		if a.Cos() != (a + quarterTurn).Sin() {
			t.Fatalf("Cos(%#04x) != Sin(+quarter turn)", i)
		}
	}
	t.Log("exhaustive 65536 angles: Sin(a) == -Sin(a+0x8000) and Cos(a) == Sin(a+0x4000)")
}

func TestAngleWraparound(t *testing.T) {
	a := Angle(0xFFFF)
	a++ // wraps to 0
	if a != 0 || a.Sin() != Angle(0).Sin() {
		t.Fatalf("wraparound: Angle(0xFFFF)+1 = %v, Sin = %d, want Sin(0) = %d", a, a.Sin(), Angle(0).Sin())
	}
	t.Logf("wraparound: Angle(0xFFFF)+1 == Angle(0), Sin == %d", a.Sin())
}

func TestAngleKnownValues(t *testing.T) {
	if got := Angle(0).Sin(); got != 0 {
		t.Fatalf("Sin(0) = %d, want 0", got)
	}
	if got := Angle(quarterTurn).Sin(); got != One {
		t.Fatalf("Sin(quarter) = %d, want One", got)
	}
	if got := Angle(halfTurn).Sin(); got != 0 {
		t.Fatalf("Sin(half) = %d, want 0", got)
	}
	if got := Angle(3 * 0x4000).Sin(); got != -One {
		t.Fatalf("Sin(3/4) = %d, want -One", got)
	}
	if got := Angle(0).Cos(); got != One {
		t.Fatalf("Cos(0) = %d, want One", got)
	}
	// 1/8 turn: sin = cos = √2/2 ≈ 0.7071067811865476 → raw ≈ 3037000500
	eighth := Angle(0x2000)
	t.Logf("known values: Sin(0)=0 Sin(¼)=One Sin(½)=0 Sin(¾)=-One Cos(0)=One; Sin(⅛ turn)=%d (√2/2*2^32≈3037000500)", eighth.Sin())
	if d := int64(eighth.Sin()) - 3037000500; d < -1 || d > 1 {
		t.Fatalf("Sin(1/8 turn) = %d, want ≈3037000500 ±1", eighth.Sin())
	}
}

func TestTrigZeroAllocs(t *testing.T) {
	a := Angle(12345)
	v := uint64(987654321987654321)
	if n := testing.AllocsPerRun(1000, func() { _ = a.Sin() }); n != 0 {
		t.Fatalf("Sin allocates %v/op", n)
	}
	if n := testing.AllocsPerRun(1000, func() { _ = a.Cos() }); n != 0 {
		t.Fatalf("Cos allocates %v/op", n)
	}
	if n := testing.AllocsPerRun(1000, func() { _ = SqrtU64(v) }); n != 0 {
		t.Fatalf("SqrtU64 allocates %v/op", n)
	}
	t.Log("AllocsPerRun = 0 for Sin/Cos/SqrtU64")
}
