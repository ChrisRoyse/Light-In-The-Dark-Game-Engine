package render

import (
	"math"
	"testing"

	"github.com/g3n/engine/math32"
)

func TestRTSCameraDefaultContractFSV(t *testing.T) {
	cfg := DefaultRTSCameraConfig(16.0 / 9.0)
	rig := NewRTSCamera(cfg)
	snap := rig.Snapshot()
	t.Logf("FSV RTS camera default snapshot=%+v", snap)

	if snap.Projection != "perspective" || snap.FOVDeg != 45 || snap.PitchFromVerticalDeg != 34 || snap.YawDeg != 0 || snap.RollDeg != 0 {
		t.Fatalf("locked camera contract mismatch: %+v", snap)
	}
	if !close32(snap.ZoomMin, 1155) || !close32(snap.Zoom, 1650) || !close32(snap.ZoomMax, 2310) {
		t.Fatalf("zoom defaults wrong: %+v", snap)
	}
	if !close32(snap.Near, 288.75) || !close32(snap.Far, 3696) {
		t.Fatalf("near/far wrong: %+v", snap)
	}

	pitch := float32(math.Pi) * RTSCameraPitchFromVerticalDeg / 180
	wantY := RTSCameraDefaultZoom * math32.Cos(pitch)
	wantZ := RTSCameraDefaultZoom * math32.Sin(pitch)
	if !close32(snap.Eye.Y, wantY) || !close32(snap.Eye.Z, wantZ) || !close32(snap.Eye.X, 0) {
		t.Fatalf("eye not anchor + zoom*unitOffset: got=%+v wantY=%.3f wantZ=%.3f", snap.Eye, wantY, wantZ)
	}
}

func TestRTSCameraZoomClampFSV(t *testing.T) {
	rig := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))

	rig.SetZoomRequested(1)
	below := rig.Snapshot()
	rig.SetZoomRequested(99999)
	above := rig.Snapshot()
	rig.SetZoomRequested(RTSCameraDefaultZoom)
	def := rig.Snapshot()

	t.Logf("FSV RTS camera clamp below=%+v", below)
	t.Logf("FSV RTS camera clamp above=%+v", above)
	t.Logf("FSV RTS camera clamp default=%+v", def)

	if below.ZoomRequested != 1 || below.Zoom != below.ZoomMin {
		t.Fatalf("below-min request did not clamp to min: %+v", below)
	}
	if above.ZoomRequested != 99999 || above.Zoom != above.ZoomMax {
		t.Fatalf("above-max request did not clamp to max: %+v", above)
	}
	if def.Zoom != RTSCameraDefaultZoom {
		t.Fatalf("default zoom wrong after clamp cycle: %+v", def)
	}
}

func TestRTSCameraScriptAnglesLockedFSV(t *testing.T) {
	rig := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))
	dump := rig.DumpWithLockProbe(91, 12, 45)
	t.Logf("FSV RTS camera lock probe=%+v", dump)

	if !dump.OK || !dump.LockProbe.Unchanged {
		t.Fatalf("lock probe failed: %+v", dump)
	}
	if dump.Snapshot.YawDeg != 0 || dump.Snapshot.PitchFromVerticalDeg != 34 || dump.Snapshot.RollDeg != 0 {
		t.Fatalf("script angle attempt mutated camera: %+v", dump.Snapshot)
	}
}

func TestRTSCameraUpdateZeroAllocFSV(t *testing.T) {
	cfg := DefaultRTSCameraConfig(16.0 / 9.0)
	rig := NewRTSCamera(cfg)
	zoom := RTSCameraDefaultZoom
	allocs := testing.AllocsPerRun(1000, func() {
		zoom += 3
		if zoom > cfg.ZoomMax {
			zoom = cfg.ZoomMin
		}
		rig.SetZoomRequested(zoom)
	})
	t.Logf("FSV RTS camera update allocs/op=%v final=%+v", allocs, rig.Snapshot())
	if allocs != 0 {
		t.Fatalf("RTS camera update allocated: %v", allocs)
	}
}

func close32(got, want float32) bool {
	d := got - want
	if d < 0 {
		d = -d
	}
	return d <= 0.001
}
