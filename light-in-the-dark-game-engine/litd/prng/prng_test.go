package prng

import (
	"math"
	"testing"
)

// Published reference output: pcg32-demo.c (pcg-random.org),
// pcg32_srandom_r(&rng, 42u, 54u), first six pcg32_random_r outputs.
var katExpected = [6]uint32{0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e}

func TestKnownAnswerVectors(t *testing.T) {
	s := New(42, 54)
	for i, want := range katExpected {
		got := s.Uint32()
		t.Logf("KAT[%d]: got 0x%08x  published 0x%08x", i, got, want)
		if got != want {
			t.Fatalf("KAT[%d]: got 0x%08x want 0x%08x", i, got, want)
		}
	}
}

func TestCursorRoundTripMidStream(t *testing.T) {
	const k = 1000
	ref := New(0xDEADBEEF, 7)
	for i := 0; i < k; i++ {
		ref.Uint32()
	}
	var refNext [100]uint32
	for i := range refNext {
		refNext[i] = ref.Uint32()
	}

	s := New(0xDEADBEEF, 7)
	for i := 0; i < k; i++ {
		s.Uint32()
	}
	cur := s.Cursor()
	restored := Restore(cur)
	for i := range refNext {
		if got := restored.Uint32(); got != refNext[i] {
			t.Fatalf("restored draw %d: got 0x%08x want 0x%08x", i, got, refNext[i])
		}
	}
	t.Logf("cursor after %d draws: %+v; restored stream's next 100 draws identical (prefix: 0x%08x 0x%08x 0x%08x ...)",
		k, cur, refNext[0], refNext[1], refNext[2])
}

func TestSubStreamSplit(t *testing.T) {
	const seed = 0x123456789ABCDEF
	master := New(seed, 0)
	masterPrefix := [3]uint32{master.Uint32(), master.Uint32(), master.Uint32()}

	s0a, s1a := Split(seed, 0), Split(seed, 1)
	p0 := [3]uint32{s0a.Uint32(), s0a.Uint32(), s0a.Uint32()}
	p1 := [3]uint32{s1a.Uint32(), s1a.Uint32(), s1a.Uint32()}
	t.Logf("master prefix: %08x %08x %08x", masterPrefix[0], masterPrefix[1], masterPrefix[2])
	t.Logf("substream0:    %08x %08x %08x", p0[0], p0[1], p0[2])
	t.Logf("substream1:    %08x %08x %08x", p1[0], p1[1], p1[2])
	if p0 == p1 {
		t.Fatal("substreams 0 and 1 must differ")
	}
	if p0 == masterPrefix || p1 == masterPrefix {
		t.Fatal("substreams must differ from master")
	}

	// stable across runs (same derivation), and master unaffected by sub draws
	s0b := Split(seed, 0)
	if got := [3]uint32{s0b.Uint32(), s0b.Uint32(), s0b.Uint32()}; got != p0 {
		t.Fatalf("substream derivation not stable: %v vs %v", got, p0)
	}
	master2 := New(seed, 0)
	_ = Split(seed, 0).Uint32() // draw on a substream
	if got := [3]uint32{master2.Uint32(), master2.Uint32(), master2.Uint32()}; got != masterPrefix {
		t.Fatal("master stream affected by substream draws")
	}
}

func TestSeedExtremes(t *testing.T) {
	a, b := New(0, 0), New(math.MaxUint64, 0)
	pa := [3]uint32{a.Uint32(), a.Uint32(), a.Uint32()}
	pb := [3]uint32{b.Uint32(), b.Uint32(), b.Uint32()}
	t.Logf("seed 0 prefix:        %08x %08x %08x", pa[0], pa[1], pa[2])
	t.Logf("seed MaxUint64 prefix: %08x %08x %08x", pb[0], pb[1], pb[2])
	if pa == pb {
		t.Fatal("seed 0 and MaxUint64 must produce distinct streams")
	}
}

func TestZeroAllocsPerDraw(t *testing.T) {
	s := New(1, 1)
	if n := testing.AllocsPerRun(10000, func() { _ = s.Uint32() }); n != 0 {
		t.Fatalf("Uint32 allocates %v/op; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for Uint32")
}
