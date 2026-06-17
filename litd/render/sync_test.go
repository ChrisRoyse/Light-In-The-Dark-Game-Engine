package render

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// snap1 builds a single-slot snapshot at the given integer sim coordinates.
func snap1(tick uint64, x, y, z, facing int32, present, snapFlag bool) *Snapshot {
	return &Snapshot{
		Tick:    tick,
		X:       []fixed.F64{fixed.FromInt(x)},
		Y:       []fixed.F64{fixed.FromInt(y)},
		Z:       []fixed.F64{fixed.FromInt(z)},
		Facing:  []fixed.F64{fixed.FromInt(facing)},
		Present: []bool{present},
		Snap:    []bool{snapFlag},
	}
}

func TestInterpolateMidpointFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 100, 200, 40, 8, true, false)
	var buf InterpBuffer
	for _, tc := range []struct {
		alpha            float32
		wx, wy, wz, wf   float32
	}{
		{0, 0, 0, 0, 0},
		{0.5, 50, 100, 20, 4},
		{1, 100, 200, 40, 8},
	} {
		Interpolate(&buf, prev, curr, tc.alpha)
		t.Logf("FSV lerp alpha=%.1f -> (%.1f,%.1f,%.1f) facing=%.1f", tc.alpha, buf.X[0], buf.Y[0], buf.Z[0], buf.Facing[0])
		if buf.X[0] != tc.wx || buf.Y[0] != tc.wy || buf.Z[0] != tc.wz || buf.Facing[0] != tc.wf {
			t.Fatalf("alpha %.1f got (%v,%v,%v,%v) want (%v,%v,%v,%v)", tc.alpha, buf.X[0], buf.Y[0], buf.Z[0], buf.Facing[0], tc.wx, tc.wy, tc.wz, tc.wf)
		}
	}
}

func TestInterpolateSnapTeleportFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 5000, 6000, 0, 0, true, true) // Snap=true (teleport)
	var buf InterpBuffer
	for _, alpha := range []float32{0, 0.25, 0.5, 0.99, 1} {
		Interpolate(&buf, prev, curr, alpha)
		t.Logf("FSV teleport alpha=%.2f -> (%.0f,%.0f)", alpha, buf.X[0], buf.Y[0])
		if buf.X[0] != 5000 || buf.Y[0] != 6000 {
			t.Fatalf("snap should jump to curr at alpha %.2f, got (%v,%v)", alpha, buf.X[0], buf.Y[0])
		}
	}
}

func TestInterpolateSpawnFSV(t *testing.T) {
	// prev has the slot absent (a spawn this tick): no lerp from origin.
	prev := snap1(10, 0, 0, 0, 0, false, false)
	curr := snap1(11, 300, 400, 0, 0, true, true)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5)
	t.Logf("FSV spawn alpha=0.5 -> (%.0f,%.0f) active=%v", buf.X[0], buf.Y[0], buf.Active[0])
	if !buf.Active[0] || buf.X[0] != 300 || buf.Y[0] != 400 {
		t.Fatalf("spawn must render at spawn point, got (%v,%v) active=%v", buf.X[0], buf.Y[0], buf.Active[0])
	}
}

func TestInterpolatePausedFSV(t *testing.T) {
	// Same snapshot twice (sim paused): position frozen, alpha irrelevant.
	s := snap1(10, 123, 456, 0, 0, true, false)
	var buf InterpBuffer
	for _, alpha := range []float32{0, 0.5, 1} {
		Interpolate(&buf, s, s, alpha)
		if buf.X[0] != 123 || buf.Y[0] != 456 {
			t.Fatalf("paused frozen failed at alpha %.1f: (%v,%v)", alpha, buf.X[0], buf.Y[0])
		}
	}
	t.Logf("FSV paused frozen at (123,456) across all alpha")
}

func TestInterpolateDeathFSV(t *testing.T) {
	prev := snap1(10, 10, 10, 0, 0, true, false)
	curr := snap1(11, 10, 10, 0, 0, false, true) // absent in curr => dead
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5)
	t.Logf("FSV death -> active=%v", buf.Active[0])
	if buf.Active[0] {
		t.Fatalf("dead entity must be inactive in render buffer")
	}
}

func TestInterpolateClampFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 100, 0, 0, 0, true, false)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, -0.5) // clamps to 0 => prev
	got0 := buf.X[0]
	Interpolate(&buf, prev, curr, 2.0) // clamps to 1 => curr
	got1 := buf.X[0]
	t.Logf("FSV clamp alpha=-0.5->%.0f alpha=2.0->%.0f", got0, got1)
	if got0 != 0 || got1 != 100 {
		t.Fatalf("alpha clamp wrong: -0.5->%v (want 0), 2.0->%v (want 100)", got0, got1)
	}
}

func TestInterpolateZeroAllocFSV(t *testing.T) {
	const n = 500
	mk := func(base int32) *Snapshot {
		s := &Snapshot{
			X: make([]fixed.F64, n), Y: make([]fixed.F64, n), Z: make([]fixed.F64, n),
			Facing: make([]fixed.F64, n), Present: make([]bool, n), Snap: make([]bool, n),
		}
		for i := 0; i < n; i++ {
			s.X[i] = fixed.FromInt(base + int32(i))
			s.Y[i] = fixed.FromInt(base + int32(i*2))
			s.Present[i] = true
		}
		return s
	}
	prev, curr := mk(0), mk(100)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5) // warm the buffers
	allocs := testing.AllocsPerRun(500, func() {
		Interpolate(&buf, prev, curr, 0.5)
	})
	t.Logf("FSV 500-mover interpolate allocs/op = %v (lastX0=%.1f)", allocs, buf.X[0])
	if allocs != 0 {
		t.Fatalf("sync path allocates %v/op for 500 movers, want 0", allocs)
	}
	// Spot-check a couple of interpolated slots against the fixed-point truth.
	if buf.X[0] != 50 || buf.X[10] != 60 {
		t.Fatalf("interpolated positions wrong: X0=%v X10=%v", buf.X[0], buf.X[10])
	}
}
