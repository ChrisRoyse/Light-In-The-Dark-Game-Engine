package render

import (
	"math"
	"testing"
)

func TestComputeNearFarFSV(t *testing.T) {
	// Default zoom clamps: 1650*0.7=1155, 1650*1.4=2310.
	near, far := ComputeNearFar(1155, 2310)
	t.Logf("FSV ComputeNearFar(1155,2310) near=%g far=%g ratio=1:%.2f", near, far, far/near)
	if math.Abs(float64(near)-288.75) > 1e-3 {
		t.Fatalf("near=%g want 288.75", near)
	}
	if math.Abs(float64(far)-3696) > 1e-3 {
		t.Fatalf("far=%g want 3696", far)
	}
	// near≈290, far≈3700 (issue #40 default-map targets).
	if near < 285 || near > 295 || far < 3650 || far > 3750 {
		t.Fatalf("near/far outside #40 targets: %g/%g", near, far)
	}
	// near:far ratio must stay under ~1:13.
	if far/near >= 13 {
		t.Fatalf("near:far ratio 1:%.2f exceeds 1:13", far/near)
	}

	// Edge: zMin==zMax — far still strictly > near (1.6z > 0.25z).
	n2, f2 := ComputeNearFar(1000, 1000)
	t.Logf("FSV degenerate zMin==zMax: near=%g far=%g", n2, f2)
	if !(f2 > n2) || n2 != 250 || f2 != 1600 {
		t.Fatalf("degenerate planes wrong: near=%g far=%g", n2, f2)
	}
	// Edge: tiny range stays positive and ordered.
	n3, f3 := ComputeNearFar(4, 8)
	if !(n3 > 0 && f3 > n3) {
		t.Fatalf("tiny range planes wrong: near=%g far=%g", n3, f3)
	}
}

func TestRTSCameraDefaultPlanesFSV(t *testing.T) {
	cam := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))
	snap := cam.Snapshot()
	wantNear, wantFar := ComputeNearFar(snap.ZoomMin, snap.ZoomMax)
	t.Logf("FSV default camera near=%g far=%g (computed %g/%g) zoomMin=%g zoomMax=%g liveNear=%g liveFar=%g",
		snap.Near, snap.Far, wantNear, wantFar, snap.ZoomMin, snap.ZoomMax, cam.Camera.Near(), cam.Camera.Far())
	if snap.Near != wantNear || snap.Far != wantFar {
		t.Fatalf("camera planes not from ComputeNearFar: snap=%g/%g want=%g/%g", snap.Near, snap.Far, wantNear, wantFar)
	}
	// The live g3n projection must carry the same planes (no lazy 10k far).
	if cam.Camera.Near() != wantNear || cam.Camera.Far() != wantFar {
		t.Fatalf("g3n projection planes %g/%g != %g/%g", cam.Camera.Near(), cam.Camera.Far(), wantNear, wantFar)
	}
}

func TestSetZoomClampsRecomputesPlanesFSV(t *testing.T) {
	cam := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))
	before := cam.Snapshot()
	cam.SetZoomClamps(2000, 4000)
	after := cam.Snapshot()
	wantNear, wantFar := ComputeNearFar(2000, 4000) // 500, 6400
	t.Logf("FSV SetZoomClamps before near/far=%g/%g after=%g/%g (live %g/%g)",
		before.Near, before.Far, after.Near, after.Far, cam.Camera.Near(), cam.Camera.Far())
	if after.Near != wantNear || after.Far != wantFar || wantNear != 500 || wantFar != 6400 {
		t.Fatalf("recomputed planes wrong: %g/%g want %g/%g", after.Near, after.Far, wantNear, wantFar)
	}
	if cam.Camera.Near() != wantNear || cam.Camera.Far() != wantFar {
		t.Fatalf("live projection not updated on Z_max change: %g/%g", cam.Camera.Near(), cam.Camera.Far())
	}
	if before.Near == after.Near || before.Far == after.Far {
		t.Fatalf("planes did not change on Z_max change")
	}
}
