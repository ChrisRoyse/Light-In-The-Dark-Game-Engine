package render

// #309 missile billboard builder FSV. SoT = the resolved billboard draw list
// (Active) the renderer would draw. X+X=Y: a missile with Arc=64 at Progress=0.5
// resolves to Y=64 (parabola peak); at launch/impact Y=0. Plus the arc symmetry,
// ground-position passthrough, over-cap drop accounting, and zero steady-state
// alloc (R-GC-2). Headless — no GL, no sim.

import (
	"math"
	"testing"
)

func TestArcHeightFSV(t *testing.T) {
	const arc = 64
	// Endpoints flat, midpoint at the peak.
	cases := []struct {
		p, want float32
	}{
		{0, 0}, {1, 0}, {0.5, 64}, {0.25, 48}, {0.75, 48},
	}
	for _, c := range cases {
		got := ArcHeight(arc, c.p)
		t.Logf("FSV ArcHeight(64, %.2f) = %.3f", c.p, got)
		if math.Abs(float64(got-c.want)) > 1e-3 {
			t.Fatalf("ArcHeight(64,%.2f) = %.3f, want %.3f", c.p, got, c.want)
		}
	}
	// Symmetry: equal heights either side of the peak.
	if ArcHeight(arc, 0.3) != ArcHeight(arc, 0.7) {
		t.Fatalf("arc not symmetric: %.4f vs %.4f", ArcHeight(arc, 0.3), ArcHeight(arc, 0.7))
	}
	// Clamp + degenerate arc.
	if ArcHeight(arc, -1) != 0 || ArcHeight(arc, 2) != 0 {
		t.Fatal("progress not clamped to [0,1]")
	}
	if ArcHeight(0, 0.5) != 0 || ArcHeight(-5, 0.5) != 0 {
		t.Fatal("non-positive arc should be flat")
	}
}

func TestProjectileBuildFSV(t *testing.T) {
	p := NewProjectileBillboards()
	inputs := []MissileBillboardInput{
		{Key: 11, GroundX: 100, GroundZ: 200, Arc: 64, Progress: 0.5, Facing: 1.5, Guidance: 1},
		{Key: 12, GroundX: 300, GroundZ: 400, Arc: 40, Progress: 0, Facing: 0, Guidance: 2},
		{Key: 13, GroundX: -50, GroundZ: 0, Arc: 20, Progress: 1, Facing: 3, Guidance: 3},
	}
	n := p.BuildInto(inputs)
	t.Logf("FSV build: built=%d dropped=%d", n, p.Dropped())
	if n != 3 || p.Count() != 3 || p.Dropped() != 0 {
		t.Fatalf("built=%d count=%d dropped=%d, want 3/3/0", n, p.Count(), p.Dropped())
	}
	a := p.Active()
	// SoT: ground XZ passthrough, Y from the arc, facing/guidance/key carried.
	if a[0].Key != 11 || a[0].X != 100 || a[0].Z != 200 || a[0].Y != 64 || a[0].Facing != 1.5 || a[0].Guidance != 1 {
		t.Fatalf("billboard 0 = %+v, want key11 (100,64,200) face1.5 guid1", a[0])
	}
	t.Logf("FSV billboard0: (%.0f,%.0f,%.0f) — peak height at progress 0.5", a[0].X, a[0].Y, a[0].Z)
	// Progress 0 and 1 are on the ground (Y=0).
	if a[1].Y != 0 || a[2].Y != 0 {
		t.Fatalf("launch/impact billboards not grounded: Y=%.3f, %.3f", a[1].Y, a[2].Y)
	}
}

func TestProjectileOverCapFSV(t *testing.T) {
	p := NewProjectileBillboards()
	// One past the cap: built is capped, the surplus is reported (no silent drop).
	inputs := make([]MissileBillboardInput, MaxProjectiles+5)
	for i := range inputs {
		inputs[i] = MissileBillboardInput{Key: uint32(i + 1), Arc: 10, Progress: 0.5}
	}
	n := p.BuildInto(inputs)
	t.Logf("FSV over-cap: inputs=%d built=%d dropped=%d", len(inputs), n, p.Dropped())
	if n != MaxProjectiles || p.Count() != MaxProjectiles {
		t.Fatalf("built=%d, want capped at %d", n, MaxProjectiles)
	}
	if p.Dropped() != 5 {
		t.Fatalf("dropped=%d, want 5 (no silent truncation)", p.Dropped())
	}
}

func TestProjectileZeroAllocFSV(t *testing.T) {
	p := NewProjectileBillboards()
	inputs := make([]MissileBillboardInput, 32)
	for i := range inputs {
		inputs[i] = MissileBillboardInput{Key: uint32(i + 1), GroundX: float32(i), Arc: 50, Progress: 0.5}
	}
	allocs := testing.AllocsPerRun(200, func() {
		p.BuildInto(inputs)
		_ = p.Active()
	})
	t.Logf("FSV zero-alloc: allocs/op=%.2f over BuildInto", allocs)
	if allocs != 0 {
		t.Fatalf("projectile build allocated %.2f/op, want 0 (R-GC-2)", allocs)
	}
}
