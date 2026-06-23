package main

import (
	"math"
	"testing"

	"github.com/g3n/engine/camera"
)

// #233 slice 4 — the recorded command stream. SoT = the generated stream struct
// (frame count, camera triples) and the validator's accept/reject verdict.
func TestBenchStreamGenerateValidateFSV(t *testing.T) {
	s := generateStream("max-battle", projPersp, benchStreamFrames)
	t.Logf("FSV stream AFTER frames=%d frame0=%+v frameLast=%+v", s.Frames, s.Camera[0], s.Camera[len(s.Camera)-1])
	if s.Frames != benchStreamFrames || len(s.Camera) != benchStreamFrames {
		t.Fatalf("frames=%d cameraKeys=%d, want %d", s.Frames, len(s.Camera), benchStreamFrames)
	}
	if err := s.validate(); err != nil {
		t.Fatalf("generated stream failed validation: %v", err)
	}
	// Zoom-in: first key farther than last; pan: anchor X increases.
	if s.Camera[0].Zoom <= s.Camera[len(s.Camera)-1].Zoom {
		t.Fatalf("not a zoom-in: first zoom %v <= last %v", s.Camera[0].Zoom, s.Camera[len(s.Camera)-1].Zoom)
	}
	if s.Camera[0].Anchor[0] >= s.Camera[len(s.Camera)-1].Anchor[0] {
		t.Fatalf("not a pan: first anchorX %v >= last %v", s.Camera[0].Anchor[0], s.Camera[len(s.Camera)-1].Anchor[0])
	}

	// Edge cases: each tamper must reject cleanly.
	for name, mut := range map[string]func(*benchStream){
		"zero frames":     func(b *benchStream) { b.Frames = 0 },
		"key count drift": func(b *benchStream) { b.Camera = b.Camera[:len(b.Camera)-1] },
		"bad projection":  func(b *benchStream) { b.Camera[0].Projection = "fisheye" },
		"non-positive dt": func(b *benchStream) { b.DtPerFrame = 0 },
		"zero zoom":       func(b *benchStream) { b.Camera[1].Zoom = 0 },
	} {
		bad := generateStream("max-battle", projPersp, benchStreamFrames)
		mut(&bad)
		if err := bad.validate(); err == nil {
			t.Fatalf("edge %q: validate accepted a malformed stream", name)
		} else {
			t.Logf("FSV edge %q AFTER rejected: %v", name, err)
		}
	}
}

// computeStreamHash is the GL↔-nogl parity contract: identical inputs → identical
// hash; any scene-definition change → different hash. This is what proves a
// headless replay executed the same command stream.
func TestBenchStreamHashParityFSV(t *testing.T) {
	s := generateStream("max-battle", projPersp, benchStreamFrames)
	gl := computeStreamHash("max-battle", variantFloor, matUnlit, 73, 60, 500, s)
	nogl := computeStreamHash("max-battle", variantFloor, matUnlit, 73, 60, 500, s)
	t.Logf("FSV hash parity AFTER gl=%s nogl=%s", gl, nogl)
	if gl != nogl {
		t.Fatalf("hash not deterministic: %s vs %s", gl, nogl)
	}
	// Any change to the scene definition must change the hash (no collisions on
	// the fields that define the workload).
	for name, h := range map[string]string{
		"draw target": computeStreamHash("max-battle", variantFloor, matUnlit, 74, 60, 500, s),
		"bars":        computeStreamHash("max-battle", variantFloor, matUnlit, 73, 61, 500, s),
		"blips":       computeStreamHash("max-battle", variantFloor, matUnlit, 73, 60, 501, s),
		"material":    computeStreamHash("max-battle", variantFloor, matPBR, 73, 60, 500, s),
		"segment":     computeStreamHash("typical", variantFloor, matUnlit, 73, 60, 500, s),
	} {
		if h == gl {
			t.Fatalf("hash collision: %q produced the same hash %s", name, gl)
		}
	}
}

// summarize must report honest aggregates and, crucially, mark the draw peak n/a
// (nil) for a -nogl series rather than fabricating a zero.
func TestStreamSummaryFSV(t *testing.T) {
	// X+X=Y: ms {4,1,3,2} → min 1, avg 2.5, p99 idx = ceil(0.99*4)-1 = 3 → 4.
	stats := []frameStat{
		{Frame: 0, FrameMS: 4, OpaqueDraws: intPtr(73), Allocs: 10},
		{Frame: 1, FrameMS: 1, OpaqueDraws: intPtr(70), Allocs: 50},
		{Frame: 2, FrameMS: 3, OpaqueDraws: intPtr(99), Allocs: 5},
		{Frame: 3, FrameMS: 2, OpaqueDraws: intPtr(80), Allocs: 5},
	}
	gl := summarize(stats, true)
	t.Logf("FSV summary GL AFTER %+v", gl)
	if gl.MinFrameMS != 1 || math.Abs(gl.AvgFrameMS-2.5) > 1e-9 || gl.P99FrameMS != 4 {
		t.Fatalf("frame-ms aggregates wrong: %+v", gl)
	}
	if gl.MaxOpaqueDraws == nil || *gl.MaxOpaqueDraws != 99 {
		t.Fatalf("max opaque draws = %v, want 99", gl.MaxOpaqueDraws)
	}
	if gl.MaxAllocs != 50 {
		t.Fatalf("max allocs = %d, want 50", gl.MaxAllocs)
	}

	// -nogl series: GL columns nil → draw peak must be n/a, not a fabricated 0.
	nogl := summarize([]frameStat{{Frame: 0, FrameMS: 0}, {Frame: 1, FrameMS: 0}}, false)
	t.Logf("FSV summary noGL AFTER %+v", nogl)
	if nogl.MaxOpaqueDraws != nil {
		t.Fatalf("-nogl max opaque draws = %v, want nil (n/a)", *nogl.MaxOpaqueDraws)
	}
	if nogl.GL {
		t.Fatal("-nogl summary marked gl=true")
	}
}

// applyCamKey must place the eye on the fixed view ray at the keyed distance and
// look at the anchor — the property that makes the path reproducible.
func TestApplyCamKeyFSV(t *testing.T) {
	cam := camera.New(16.0 / 9.0)
	// zoom≈1633.7 (|(0,1350,920)|) reproduces the canonical static eye (0,1350,920).
	const z = 1633.7
	applyCamKey(cam, camKey{Anchor: [3]float32{10, 0, 0}, Zoom: z, Projection: projPersp})
	pos := cam.Position()
	wantX := 10 + benchViewRay.X*z
	wantY := benchViewRay.Y * z
	wantZ := benchViewRay.Z * z
	t.Logf("FSV camKey AFTER eye=(%.1f,%.1f,%.1f) want=(%.1f,%.1f,%.1f)", pos.X, pos.Y, pos.Z, wantX, wantY, wantZ)
	if math.Abs(float64(pos.X-wantX)) > 0.5 || math.Abs(float64(pos.Y-wantY)) > 0.5 || math.Abs(float64(pos.Z-wantZ)) > 0.5 {
		t.Fatalf("eye misplaced: got (%.2f,%.2f,%.2f) want (%.2f,%.2f,%.2f)", pos.X, pos.Y, pos.Z, wantX, wantY, wantZ)
	}
}
