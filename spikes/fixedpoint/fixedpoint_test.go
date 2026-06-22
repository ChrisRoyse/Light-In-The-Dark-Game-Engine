// Spike S1 (decision D-2026-06-11-1 validation): fixed-point int64 32.32
// performance and reproducibility. Validates the M1 questions:
//   - worst-case tick math cost at 1,000 units + 1,000 projectiles vs the 10 ms budget
//   - bit-identical state hash across repeated 10k-tick runs
//   - range/precision sanity for map coords, DPS accumulators, long timers
package fixedpoint

import (
	"hash/fnv"
	"math/bits"
	"testing"
)

// F is a 32.32 fixed-point number.
type F int64

const One F = 1 << 32

func FromInt(i int) F      { return F(i) << 32 }
func FromF64(f float64) F  { return F(f * float64(One)) }
func (a F) Float() float64 { return float64(a) / float64(One) }

func (a F) Mul(b F) F {
	hi, lo := bits.Mul64(uint64(abs64(int64(a))), uint64(abs64(int64(b))))
	r := int64(hi<<32 | lo>>32)
	if (a < 0) != (b < 0) {
		return F(-r)
	}
	return F(r)
}

func (a F) Div(b F) F {
	na, nb := abs64(int64(a)), abs64(int64(b))
	hi, lo := na>>32, na<<32
	q, _ := bits.Div64(uint64(hi), uint64(lo), uint64(nb))
	if (a < 0) != (b < 0) {
		return F(-int64(q))
	}
	return F(q)
}

// Sqrt returns the fixed-point square root (integer Newton iteration).
func (a F) Sqrt() F {
	if a <= 0 {
		return 0
	}
	x := uint64(a)
	// initial guess from bit length
	g := uint64(1) << ((bits.Len64(x) + 32) / 2)
	for i := 0; i < 32; i++ {
		ng := (g + (x<<16)/(g>>16)) / 2 // scale-corrected for 32.32
		if ng == g {
			break
		}
		g = ng
	}
	return F(g)
}

func abs64(v int64) int64 {
	if v < 0 {
		return -v
	}
	return v
}

// pcg32 is the sim's seeded PRNG candidate (deterministic, allocation-free).
type pcg32 struct{ state, inc uint64 }

func (p *pcg32) next() uint32 {
	old := p.state
	p.state = old*6364136223846793005 + (p.inc | 1)
	xorshifted := uint32(((old >> 18) ^ old) >> 27)
	rot := uint32(old >> 59)
	return (xorshifted >> rot) | (xorshifted << ((-rot) & 31))
}

type entity struct {
	px, py, vx, vy, hp F
}

const (
	nUnits       = 1000
	nProjectiles = 1000
	nEntities    = nUnits + nProjectiles
)

// stepWorld is one tick of representative sim math: movement integration,
// distance checks against a target, damage accumulation, normalization.
func stepWorld(ents []entity, rng *pcg32, dt F) uint64 {
	h := fnv.New64a()
	var hashAcc uint64
	for i := range ents {
		e := &ents[i]
		// pick a pseudo-target deterministically
		ti := int(rng.next()) % len(ents)
		if ti < 0 {
			ti = -ti
		}
		t := &ents[ti]
		dx, dy := t.px-e.px, t.py-e.py
		distSq := dx.Mul(dx) + dy.Mul(dy)
		dist := distSq.Sqrt()
		if dist > FromInt(2) {
			// normalize direction and accelerate toward target
			inv := One.Div(dist + 1)
			e.vx += dx.Mul(inv).Mul(dt)
			e.vy += dy.Mul(inv).Mul(dt)
		} else {
			// in range: apply damage
			t.hp -= FromF64(12.5).Mul(dt)
		}
		e.px += e.vx.Mul(dt)
		e.py += e.vy.Mul(dt)
		hashAcc ^= uint64(e.px) * 0x9E3779B97F4A7C15
		hashAcc ^= uint64(e.py)
	}
	_ = h
	return hashAcc
}

func newWorld() ([]entity, *pcg32) {
	ents := make([]entity, nEntities)
	rng := &pcg32{state: 0x853c49e6748fea9b, inc: 0xda3e39cb94b95bdb}
	for i := range ents {
		ents[i].px = FromInt(int(rng.next()%8192)) / 64
		ents[i].py = FromInt(int(rng.next()%8192)) / 64
		ents[i].hp = FromInt(100)
	}
	return ents, rng
}

// run10k runs 10,000 ticks and returns the final state hash.
func run10k() uint64 {
	ents, rng := newWorld()
	dt := FromF64(0.05) // 50 ms tick
	var hash uint64
	for tick := 0; tick < 10000; tick++ {
		hash = hash*31 ^ stepWorld(ents, rng, dt)
	}
	return hash
}

// TestReproducibility: 10 full 10k-tick runs must produce identical hashes.
// (The cross-OS/arch matrix runs this same test in CI later; fixed-point is
// integer-only so divergence is structurally impossible.)
func TestReproducibility(t *testing.T) {
	if testing.Short() {
		t.Skip("10x 10k-tick historical fixed-point spike skipped in -short")
	}
	want := run10k()
	for i := 0; i < 9; i++ {
		if got := run10k(); got != want {
			t.Fatalf("run %d: hash %x != %x", i, got, want)
		}
	}
	t.Logf("10k-tick state hash stable across 10 runs: %x", want)
}

// TestRangePrecision: map coords (0..8192 with 1/64 precision), long timers,
// and DPS accumulation stay exact within 32.32.
func TestRangePrecision(t *testing.T) {
	// 4 hours of 50ms ticks accumulated
	tickDur := FromF64(0.05)
	var elapsed F
	for i := 0; i < 4*3600*20; i++ {
		elapsed += tickDur
	}
	if got := elapsed.Float(); got < 14399.9 || got > 14400.1 {
		t.Fatalf("4h timer drift: %v", got)
	}
	// DPS accumulator: 12.5 dmg/s for 1 hour
	var dmg F
	per := FromF64(12.5).Mul(tickDur)
	for i := 0; i < 3600*20; i++ {
		dmg += per
	}
	if got := dmg.Float(); got < 44995 || got > 45005 {
		t.Fatalf("dps accum drift: %v (want ~45000)", got)
	}
	// coordinate edge: 8192*64 fits with huge headroom in 31 integer bits
	c := FromInt(8192).Mul(FromInt(64))
	if c.Float() != 524288 {
		t.Fatalf("coord range: %v", c.Float())
	}
}

// BenchmarkTick measures one full 2,000-entity tick of representative math.
// Budget context: whole tick ≤ 10 ms; movement/combat math should be far under.
func BenchmarkTick(b *testing.B) {
	ents, rng := newWorld()
	dt := FromF64(0.05)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		stepWorld(ents, rng, dt)
	}
}

// BenchmarkFloatTick is the float64 comparison baseline (same workload).
func BenchmarkFloatTick(b *testing.B) {
	type fe struct{ px, py, vx, vy, hp float64 }
	ents := make([]fe, nEntities)
	rng := &pcg32{state: 0x853c49e6748fea9b, inc: 0xda3e39cb94b95bdb}
	for i := range ents {
		ents[i].px = float64(rng.next()%8192) / 64
		ents[i].py = float64(rng.next()%8192) / 64
		ents[i].hp = 100
	}
	sqrt := func(x float64) float64 { // Newton, to keep comparison apples-to-apples
		if x <= 0 {
			return 0
		}
		g := x
		for i := 0; i < 8; i++ {
			g = (g + x/g) / 2
		}
		return g
	}
	b.ReportAllocs()
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		for i := range ents {
			e := &ents[i]
			ti := int(rng.next()) % len(ents)
			if ti < 0 {
				ti = -ti
			}
			t := &ents[ti]
			dx, dy := t.px-e.px, t.py-e.py
			dist := sqrt(dx*dx + dy*dy)
			if dist > 2 {
				inv := 1 / (dist + 1)
				e.vx += dx * inv * 0.05
				e.vy += dy * inv * 0.05
			} else {
				t.hp -= 12.5 * 0.05
			}
			e.px += e.vx * 0.05
			e.py += e.vy * 0.05
		}
	}
}
