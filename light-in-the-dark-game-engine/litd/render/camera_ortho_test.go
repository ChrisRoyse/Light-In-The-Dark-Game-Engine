package render

import "testing"

func TestRTSCameraOrthographicClampSizeFSV(t *testing.T) {
	cfg := DefaultRTSCameraConfig(16.0 / 9.0)
	rig := NewRTSCamera(cfg)
	if err := rig.SetProjectionMode(RTSCameraProjectionOrthographic); err != nil {
		t.Fatalf("orthographic projection rejected: %v", err)
	}

	rig.SetZoomRequested(cfg.ZoomMin * 0.5)
	below := rig.Snapshot()
	rig.SetZoomRequested(cfg.ZoomMax * 2)
	above := rig.Snapshot()

	t.Logf("FSV ortho clamp below=%+v", below)
	t.Logf("FSV ortho clamp above=%+v", above)

	if below.Projection != "orthographic" || above.Projection != "orthographic" {
		t.Fatalf("projection did not stay orthographic: below=%+v above=%+v", below, above)
	}
	if below.Zoom != below.ZoomMin || !close32(below.OrthoSize, below.OrthoSizeMin) {
		t.Fatalf("below-min zoom did not clamp to Size_min: %+v", below)
	}
	if above.Zoom != above.ZoomMax || !close32(above.OrthoSize, above.OrthoSizeMax) {
		t.Fatalf("above-max zoom did not clamp to Size_max: %+v", above)
	}
	if below.OrthoSizeScale <= 0 || above.OrthoSizeScale <= 0 {
		t.Fatalf("orthographic footprint scale not calibrated: below=%+v above=%+v", below, above)
	}
}

func TestRTSCameraProjectionFootprintParityFSV(t *testing.T) {
	cfg := DefaultRTSCameraConfig(16.0 / 9.0)
	for _, tc := range []struct {
		name string
		zoom float32
	}{
		{name: "zmin", zoom: cfg.ZoomMin},
		{name: "zmax", zoom: cfg.ZoomMax},
	} {
		rig := NewRTSCamera(cfg)
		rig.SetZoomRequested(tc.zoom)
		parity := rig.ProjectionParityFootprints()
		t.Logf("FSV footprint parity %s perspArea=%.3f orthoArea=%.3f deltaPct=%.4f tolerancePct=%.3f",
			tc.name, parity.Perspective.Area, parity.Orthographic.Area, parity.AreaDeltaPct, parity.AreaTolerancePct)
		if !parity.OK {
			t.Fatalf("%s footprint parity failed: %+v", tc.name, parity)
		}
		if parity.Perspective.Projection != "perspective" || parity.Orthographic.Projection != "orthographic" {
			t.Fatalf("%s parity projections wrong: %+v", tc.name, parity)
		}
	}
}

func TestRTSCameraProjectionPickParityFSV(t *testing.T) {
	rig := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))
	pick := rig.PickParityForViewport(960, 540)
	t.Logf("FSV pick parity center pixel=%+v", pick)

	if !pick.SameCell {
		t.Fatalf("center-pixel pick did not resolve to the same cell: %+v", pick)
	}
	if pick.Screen.X != 480 || pick.Screen.Y != 270 || pick.NDC.X != 0 || pick.NDC.Y != 0 {
		t.Fatalf("unexpected pick parity sample point: %+v", pick)
	}
	if !close32(pick.Perspective.Projected.X, 0) || !close32(pick.Perspective.Projected.Y, 0) {
		t.Fatalf("perspective pick did not project back to center: %+v", pick.Perspective)
	}
	if !close32(pick.Orthographic.Projected.X, 0) || !close32(pick.Orthographic.Projected.Y, 0) {
		t.Fatalf("orthographic pick did not project back to center: %+v", pick.Orthographic)
	}
}

func TestRTSCameraProjectionParserFSV(t *testing.T) {
	cases := []struct {
		input string
		want  RTSCameraProjection
	}{
		{input: "", want: RTSCameraProjectionPerspective},
		{input: "persp", want: RTSCameraProjectionPerspective},
		{input: "perspective", want: RTSCameraProjectionPerspective},
		{input: "ortho", want: RTSCameraProjectionOrthographic},
		{input: "orthographic", want: RTSCameraProjectionOrthographic},
	}
	for _, tc := range cases {
		got, err := ParseRTSCameraProjection(tc.input)
		t.Logf("FSV camera projection input=%q got=%q err=%v", tc.input, got, err)
		if err != nil || got != tc.want {
			t.Fatalf("ParseRTSCameraProjection(%q) = %q, %v; want %q nil", tc.input, got, err, tc.want)
		}
	}
	if got, err := ParseRTSCameraProjection("isometric"); err == nil {
		t.Fatalf("invalid projection accepted: got %q", got)
	} else {
		t.Logf("FSV camera projection invalid input=%q err=%v", "isometric", err)
	}
}
