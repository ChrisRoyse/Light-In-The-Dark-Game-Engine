package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func mathGame() *Game { return newGame(sim.NewWorld(sim.Caps{Units: 4})) }

// foldU32 is an order-sensitive rolling hash of a draw sequence — the
// sequence fingerprint compared across runs.
func foldU32(vals []int) uint64 {
	var h uint64 = 1469598103934665603
	for _, v := range vals {
		h ^= uint64(uint32(v))
		h *= 1099511628211
	}
	return h
}

// TestPRNGDeterminism — edge (1): same seed, 10,000 draws, two
// independent games produce the identical sequence (R-SIM-2). SoT: the
// sequence fold hash + first/last 5 values.
func TestPRNGDeterminism(t *testing.T) {
	draw := func(seed int64, n int) []int {
		g := mathGame()
		g.SetRandomSeed(seed)
		out := make([]int, n)
		for i := range out {
			out[i] = g.RandomInt(0, 1<<30)
		}
		return out
	}
	a := draw(20260612, 10000)
	b := draw(20260612, 10000)
	ha, hb := foldU32(a), foldU32(b)
	t.Logf("first5=%v last5=%v", a[:5], a[len(a)-5:])
	t.Logf("seqHash run1=%#x run2=%#x", ha, hb)
	if ha != hb {
		t.Fatalf("same-seed sequences diverged: %#x vs %#x", ha, hb)
	}
	// sanity: the draws are not all identical (the PRNG actually moves)
	if a[0] == a[1] && a[1] == a[2] {
		t.Fatalf("PRNG appears stuck: %v", a[:3])
	}
}

// TestPRNGSeedChange — edge (2): the seed controls the sequence —
// reseeding to the same value reproduces it, to a different value
// diverges.
func TestPRNGSeedChange(t *testing.T) {
	seq := func(seed int64) []int {
		g := mathGame()
		g.SetRandomSeed(seed)
		return []int{g.RandomInt(0, 1000), g.RandomInt(0, 1000), g.RandomInt(0, 1000)}
	}
	s1a, s1b, s2 := seq(1), seq(1), seq(2)
	t.Logf("seed1=%v  seed1again=%v  seed2=%v", s1a, s1b, s2)
	if foldU32(s1a) != foldU32(s1b) {
		t.Fatalf("same seed gave different sequences: %v vs %v", s1a, s1b)
	}
	if foldU32(s1a) == foldU32(s2) {
		t.Fatalf("different seeds gave the same sequence: %v", s1a)
	}
}

// TestRandomRanges checks bounds: RandomFloat in [0,1), RandomInt
// inclusive, degenerate range returns min.
func TestRandomRanges(t *testing.T) {
	g := mathGame()
	g.SetRandomSeed(99)
	for i := 0; i < 1000; i++ {
		f := g.RandomFloat()
		if f < 0 || f >= 1 {
			t.Fatalf("RandomFloat out of [0,1): %v", f)
		}
		n := g.RandomInt(5, 7)
		if n < 5 || n > 7 {
			t.Fatalf("RandomInt(5,7) out of range: %d", n)
		}
	}
	if d := g.RandomInt(42, 42); d != 42 {
		t.Fatalf("degenerate RandomInt(42,42) = %d, want 42", d)
	}
	t.Logf("RandomFloat∈[0,1), RandomInt(5,7) inclusive, RandomInt(42,42)=42 — all in bounds")
}

// TestAngleNormalize — edge (3): Deg(360) normalizes to 0; negative
// angles wrap into [0, 2π).
func TestAngleNormalize(t *testing.T) {
	cases := []struct {
		in   Angle
		want float64 // radians
	}{
		{Deg(360), 0},
		{Deg(-90), 3 * math.Pi / 2},
		{Rad(2.5 * math.Pi), 0.5 * math.Pi},
		{Deg(450), 0.5 * math.Pi},
	}
	for _, c := range cases {
		got := c.in.Normalized().Radians()
		t.Logf("Normalize(%.4f rad) -> %.6f (want %.6f)", c.in.Radians(), got, c.want)
		if !approx(got, c.want) {
			t.Errorf("Normalized(%v) = %v rad, want %v", c.in.Radians(), got, c.want)
		}
	}
}

// TestStringHash — edge (4): empty string and known FNV-1a/32 vectors.
func TestStringHash(t *testing.T) {
	cases := []struct {
		s    string
		want uint32 // canonical FNV-1a 32-bit vectors
	}{
		{"", 0x811c9dc5},
		{"a", 0xe40c292c},
		{"foobar", 0xbf9cf968},
	}
	for _, c := range cases {
		got := uint32(StringHash(c.s))
		t.Logf("StringHash(%q) = %#08x (want %#08x)", c.s, got, c.want)
		if got != c.want {
			t.Errorf("StringHash(%q) = %#x, want %#x", c.s, got, c.want)
		}
	}
}

// TestVec2Geometry checks the deterministic-fixed-point geometry.
func TestVec2Geometry(t *testing.T) {
	if d := (Vec2{X: 0, Y: 0}).DistanceTo(Vec2{X: 3, Y: 4}); !approx(d, 5) {
		t.Errorf("DistanceTo (3,4) = %v, want 5", d)
	}
	if a := (Vec2{}).AngleTo(Vec2{X: 0, Y: 1}).Normalized().Radians(); !near(a, math.Pi/2, 0.01) {
		t.Errorf("AngleTo (0,1) = %v rad, want ~π/2", a)
	}
	p := (Vec2{}).Polar(Deg(90), 10)
	t.Logf("Distance=5 | AngleTo(0,1)≈π/2 | Polar(90°,10)=%v", p)
	if !near(p.X, 0, 0.01) || !near(p.Y, 10, 0.01) {
		t.Errorf("Polar(90°,10) = %v, want ~(0,10)", p)
	}
}

func near(a, b, tol float64) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d <= tol
}
