package statehash

import (
	"encoding/binary"
	"math/bits"
	"testing"
)

// --- Independent one-shot reference --------------------------------------
//
// refSum64 is a deliberately separate, non-streaming xxHash64 written
// straight from the published algorithm description. It shares no
// buffering code with Hasher, so it cross-checks the streaming path
// (stripe buffering, tail handling) rather than just repeating it.

func refRound(acc, in uint64) uint64 {
	return bits.RotateLeft64(acc+in*0xC2B2AE3D27D4EB4F, 31) * 0x9E3779B185EBCA87
}

func refSum64(b []byte) uint64 {
	const (
		p1 uint64 = 0x9E3779B185EBCA87
		p2 uint64 = 0xC2B2AE3D27D4EB4F
		p3 uint64 = 0x165667B19E3779F9
		p4 uint64 = 0x85EBCA77C2B2AE63
		p5 uint64 = 0x27D4EB2F165667C5
	)
	var acc uint64
	n := uint64(len(b))
	if len(b) >= 32 {
		v1, v2, v3, v4 := p1, p2, uint64(0), uint64(0)
		v1 += p2 // wraps at runtime; overflowing const expr is rejected
		v4 -= p1
		for len(b) >= 32 {
			v1 = refRound(v1, binary.LittleEndian.Uint64(b[0:8]))
			v2 = refRound(v2, binary.LittleEndian.Uint64(b[8:16]))
			v3 = refRound(v3, binary.LittleEndian.Uint64(b[16:24]))
			v4 = refRound(v4, binary.LittleEndian.Uint64(b[24:32]))
			b = b[32:]
		}
		acc = bits.RotateLeft64(v1, 1) + bits.RotateLeft64(v2, 7) +
			bits.RotateLeft64(v3, 12) + bits.RotateLeft64(v4, 18)
		for _, v := range []uint64{v1, v2, v3, v4} {
			acc = (acc^refRound(0, v))*p1 + p4
		}
	} else {
		acc = p5
	}
	acc += n
	for len(b) >= 8 {
		acc = bits.RotateLeft64(acc^refRound(0, binary.LittleEndian.Uint64(b[:8])), 27)*p1 + p4
		b = b[8:]
	}
	if len(b) >= 4 {
		acc = bits.RotateLeft64(acc^uint64(binary.LittleEndian.Uint32(b[:4]))*p1, 23)*p2 + p3
		b = b[4:]
	}
	for _, c := range b {
		acc = bits.RotateLeft64(acc^uint64(c)*p5, 11) * p1
	}
	acc ^= acc >> 33
	acc *= p2
	acc ^= acc >> 29
	acc *= p3
	acc ^= acc >> 32
	return acc
}

// --- KATs vs published vectors -------------------------------------------

// Published xxHash64 seed-0 vectors (xxHash project / RFC draft
// draft-doering-xxhash; "Nobody inspects..." crosses the 32-byte stripe).
var kats = []struct {
	in   string
	want uint64
}{
	{"", 0xEF46DB3751D8E999},
	{"a", 0xD24EC4F1A98C6E5B},
	{"abc", 0x44BC2CF5AD770999},
	{"Nobody inspects the spammish repetition", 0xFBCEA83C8A378BF1},
}

func TestKnownAnswerVectors(t *testing.T) {
	for _, k := range kats {
		h := New()
		h.WriteBytes([]byte(k.in))
		got := h.Sum64()
		t.Logf("KAT %-42q len=%2d  got 0x%016X  published 0x%016X", k.in, len(k.in), got, k.want)
		if got != k.want {
			t.Fatalf("KAT %q: got 0x%016X want 0x%016X", k.in, got, k.want)
		}
		if ref := refSum64([]byte(k.in)); ref != k.want {
			t.Fatalf("reference impl broken on %q: 0x%016X", k.in, ref)
		}
	}
}

// deterministic synthetic bytes (no global RNG — known input, known output)
func synth(n int) []byte {
	b := make([]byte, n)
	x := uint64(0x9E3779B97F4A7C15)
	for i := range b {
		x = x*6364136223846793005 + 1442695040888963407
		b[i] = byte(x >> 56)
	}
	return b
}

func TestBoundaryLengthsVsReference(t *testing.T) {
	// stripe boundary 32, block boundary 64, and neighbors either side
	for _, n := range []int{0, 1, 3, 4, 7, 8, 31, 32, 33, 39, 63, 64, 65, 100, 1024, 1025} {
		in := synth(n)
		h := New()
		h.WriteBytes(in)
		got, want := h.Sum64(), refSum64(in)
		t.Logf("len=%4d  streaming 0x%016X  one-shot ref 0x%016X", n, got, want)
		if got != want {
			t.Fatalf("len %d: streaming 0x%016X != reference 0x%016X", n, got, want)
		}
	}
}

func TestChunkedEqualsOneShot(t *testing.T) {
	in := synth(257) // crosses several stripes, odd tail
	one := New()
	one.WriteBytes(in)
	chunked := New()
	for i := range in {
		chunked.WriteBytes(in[i : i+1])
	}
	a, b := one.Sum64(), chunked.Sum64()
	t.Logf("257 bytes: one-shot 0x%016X  1-byte-chunked 0x%016X", a, b)
	if a != b {
		t.Fatalf("chunked %x != one-shot %x", b, a)
	}
}

// --- Padding-layout immunity ----------------------------------------------

// Two structs with identical logical fields but different memory layout:
// posA packs to 24 bytes with no padding; posB's field order inserts
// 7+3 padding bytes (size 32 on amd64). Hashing via the writer API must
// make them indistinguishable — that is the whole point of forbidding
// raw-memory hashing.
type posA struct {
	X, Y int64
	HP   uint32
	Team uint8
	pad  [11]byte // explicit junk standing in for padding garbage
}
type posB struct {
	Team uint8
	junk [7]byte // different junk in different places
	X    int64
	HP   uint32
	pad  [4]byte
	Y    int64
}

func writeFields(h *Hasher, x, y int64, hp uint32, team uint8) {
	h.WriteI64(x)
	h.WriteI64(y)
	h.WriteU32(hp)
	h.WriteU8(team)
}

func TestPaddingLayoutImmunity(t *testing.T) {
	a := posA{X: -42, Y: 1 << 40, HP: 100, Team: 3, pad: [11]byte{0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4, 5, 6, 7}}
	b := posB{Team: 3, junk: [7]byte{0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF}, X: -42, HP: 100, Y: 1 << 40}

	ha, hb := New(), New()
	writeFields(ha, a.X, a.Y, a.HP, a.Team)
	writeFields(hb, b.X, b.Y, b.HP, b.Team)
	sa, sb := ha.Sum64(), hb.Sum64()
	t.Logf("layout A (packed, junk pad %x): 0x%016X", a.pad, sa)
	t.Logf("layout B (reordered, junk %x):  0x%016X", b.junk, sb)
	if sa != sb {
		t.Fatalf("identical logical fields hashed differently: 0x%016X vs 0x%016X", sa, sb)
	}
}

// --- Registry: bisect, ordering, divergence -------------------------------

func tickInto(r *Registry, units, proj *Hasher, hp uint32) {
	r.Reset()
	units.WriteI64(1001) // unit id
	units.WriteU32(hp)
	units.WriteI64(-77) // x
	proj.WriteI64(555)  // projectile id
	proj.WriteU32(9)
}

func TestBitFlipBisect(t *testing.T) {
	reg := NewRegistry()
	units := reg.Register("units")
	proj := reg.Register("projectiles")
	movement := reg.Register("movement") // contributes nothing this tick

	var before, after Snapshot
	tickInto(reg, units, proj, 100)
	reg.Sum(&before)
	tickInto(reg, units, proj, 100^(1<<4)) // single bit flipped in units' hp
	reg.Sum(&after)

	t.Logf("%-12s %-18s %-18s %s", "system", "before", "after", "changed")
	changed := 0
	for i, name := range reg.Names() {
		mark := ""
		if before.Subs[i] != after.Subs[i] {
			mark = "  <-- DIVERGED"
			changed++
		}
		t.Logf("%-12s 0x%016X 0x%016X%s", name, before.Subs[i], after.Subs[i], mark)
	}
	t.Logf("%-12s 0x%016X 0x%016X", "TOP", before.Top, after.Top)

	if before.Top == after.Top {
		t.Fatal("single bit flip did not change top hash")
	}
	if changed != 1 {
		t.Fatalf("expected exactly 1 sub-hash to change, got %d", changed)
	}
	name, ok := reg.FirstDivergence(&before, &after)
	if !ok || name != "units" {
		t.Fatalf("FirstDivergence = (%q, %v), want (\"units\", true)", name, ok)
	}
	t.Logf("FirstDivergence -> %q", name)
	_ = movement
}

func TestEmptyContributionOrdering(t *testing.T) {
	ab := NewRegistry()
	ab.Register("alpha")
	ab.Register("beta")
	ba := NewRegistry()
	ba.Register("beta")
	ba.Register("alpha")

	var sab, sba Snapshot
	ab.Sum(&sab)
	ba.Sum(&sba)
	t.Logf("order alpha,beta (both empty): top 0x%016X", sab.Top)
	t.Logf("order beta,alpha (both empty): top 0x%016X", sba.Top)
	if sab.Top == sba.Top {
		t.Fatal("registration order of empty contributions must change top hash")
	}
	if sab.Subs[0] != sba.Subs[1] || sab.Subs[1] != sba.Subs[0] {
		t.Fatal("empty sub-hashes themselves should be equal regardless of order")
	}
}

func TestFirstDivergenceNoDiff(t *testing.T) {
	reg := NewRegistry()
	u := reg.Register("units")
	var s1, s2 Snapshot
	u.WriteU64(7)
	reg.Sum(&s1)
	reg.Reset()
	u.WriteU64(7)
	reg.Sum(&s2)
	if name, ok := reg.FirstDivergence(&s1, &s2); ok {
		t.Fatalf("identical streams reported divergent at %q", name)
	}
	if s1.Top != s2.Top {
		t.Fatal("identical streams produced different top hashes")
	}
	t.Logf("identical ticks: top 0x%016X == 0x%016X, no divergence", s1.Top, s2.Top)
}

func TestDuplicateRegisterPanics(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("duplicate Register did not panic")
		}
	}()
	r := NewRegistry()
	r.Register("units")
	r.Register("units")
}

// --- R-GC-1: zero allocations at steady state ------------------------------

func TestZeroAllocsSteadyState(t *testing.T) {
	h := New()
	if n := testing.AllocsPerRun(1000, func() {
		h.WriteU64(0xABCDEF)
		h.WriteU32(7)
		h.WriteU8(1)
		h.WriteBool(true)
		_ = h.Sum64()
	}); n != 0 {
		t.Fatalf("Hasher write+sum allocates %v/op; R-GC-1 requires 0", n)
	}

	reg := NewRegistry()
	u := reg.Register("units")
	p := reg.Register("projectiles")
	var snap Snapshot
	reg.Sum(&snap) // warm: allocate Subs once
	if n := testing.AllocsPerRun(1000, func() {
		reg.Reset()
		u.WriteI64(123)
		p.WriteI64(456)
		reg.Sum(&snap)
	}); n != 0 {
		t.Fatalf("Registry tick allocates %v/op; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for Hasher writes+Sum64 and full Registry tick")
}
