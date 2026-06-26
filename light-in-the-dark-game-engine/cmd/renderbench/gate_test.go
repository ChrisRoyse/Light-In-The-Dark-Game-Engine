package main

import (
	"math"
	"testing"
)

// #236 — the M4 budget gate evaluation. SoT = the pure gate scans over a synthetic
// set of per-frame dumps with known inputs and known worst-case outputs.
func TestGateScansFSV(t *testing.T) {
	mk := func(draws, allocs []int) *benchDump {
		d := &benchDump{}
		for i := range draws {
			d.PerFrame = append(d.PerFrame, frameStat{
				Frame:     i,
				DrawCalls: intPtr(draws[i]),
				Allocs:    int64(allocs[i]),
			})
		}
		return d
	}
	dumps := map[string]*benchDump{
		// frame 0 has the high alloc (compile) that must be excluded from the gate.
		"a/unlit/persp": mk([]int{73, 73, 73}, []int{1350, 437, 437}),
		"b/unlit/ortho": mk([]int{133, 130, 130}, []int{1350, 500, 480}),
	}

	// Draw-call worst: 133 at b frame 0 (<=300 → would pass).
	worst, where := worstDrawCalls(dumps)
	t.Logf("FSV draw worst AFTER %d (%s)", worst, where)
	if worst != 133 || where != "b/unlit/ortho frame 0" {
		t.Fatalf("worst draws = %d (%s), want 133 (b/unlit/ortho frame 0)", worst, where)
	}

	// Steady allocs worst: 500 at b frame 1 — the 1350 compile frames (frame 0) are
	// excluded, proving the gate measures steady state.
	wa, wAllocWhere := worstSteadyAllocs(dumps)
	t.Logf("FSV alloc worst AFTER %d (%s)", wa, wAllocWhere)
	if wa != 500 || wAllocWhere != "b/unlit/ortho frame 1" {
		t.Fatalf("worst steady allocs = %d (%s), want 500 (b/unlit/ortho frame 1)", wa, wAllocWhere)
	}
}

// A run whose draws exceed 300 (no-instancing baseline) must be caught.
func TestGateDrawCeilingCatchesRegressionFSV(t *testing.T) {
	baseline := map[string]*benchDump{
		"stress/unlit/persp": {PerFrame: []frameStat{{Frame: 0, DrawCalls: intPtr(1001)}}},
	}
	worst, where := worstDrawCalls(baseline)
	t.Logf("FSV regression AFTER worst=%d where=%s pass=%v", worst, where, worst <= gateMaxDrawCalls)
	if worst <= gateMaxDrawCalls {
		t.Fatalf("baseline 1001 draws not flagged over the %d ceiling", gateMaxDrawCalls)
	}
	if where != "stress/unlit/persp frame 0" {
		t.Fatalf("offender label = %q, want stress/unlit/persp frame 0", where)
	}
}

// steadyFPS drops the first-frame compile spike and reports the median FPS.
func TestSteadyFPSFSV(t *testing.T) {
	// X+X=Y: ms {99(compile), 2,4,2,4} → drop frame0 → {2,4,2,4} sorted {2,2,4,4},
	// median index 2 = 4 ms → 250 fps.
	d := &benchDump{PerFrame: []frameStat{
		{Frame: 0, FrameMS: 99}, {Frame: 1, FrameMS: 2}, {Frame: 2, FrameMS: 4},
		{Frame: 3, FrameMS: 2}, {Frame: 4, FrameMS: 4},
	}}
	fps := steadyFPS(d)
	t.Logf("FSV steadyFPS AFTER %.1f", fps)
	if math.Abs(fps-250) > 0.01 {
		t.Fatalf("steady fps = %.3f, want 250 (median 4 ms, compile frame excluded)", fps)
	}
	// Degenerate: a single frame (only the compile frame) → 0, not a divide blow-up.
	if got := steadyFPS(&benchDump{PerFrame: []frameStat{{Frame: 0, FrameMS: 5}}}); got != 0 {
		t.Fatalf("single-frame steady fps = %v, want 0", got)
	}
}
