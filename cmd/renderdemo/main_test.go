package main

import (
	"testing"

	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
)

func TestResolutionFlagSetFSV(t *testing.T) {
	var r resolutionFlag
	if err := r.Set("1920x1080"); err != nil {
		t.Fatalf("valid resolution rejected: %v", err)
	}
	t.Logf("FSV resolution valid BEFORE empty AFTER %+v", r)
	if r.W != 1920 || r.H != 1080 || !r.set {
		t.Fatalf("valid resolution parsed incorrectly: %+v", r)
	}

	before := r
	invalid := []string{
		"",
		"1920",
		"1920x",
		"x1080",
		"1920x1080extra",
		"1920x1080x1",
		"0x1080",
		"1920x-1",
		"1920X1080",
	}
	for _, input := range invalid {
		if err := r.Set(input); err == nil {
			t.Fatalf("invalid resolution %q accepted: %+v", input, r)
		}
		t.Logf("FSV resolution invalid input=%q BEFORE %+v AFTER %+v", input, before, r)
		if r != before {
			t.Fatalf("invalid resolution %q mutated state: got %+v want %+v", input, r, before)
		}
	}
}

func TestCameraZoomRequestFSV(t *testing.T) {
	cfg := litrender.DefaultRTSCameraConfig(16.0 / 9.0)
	cases := []struct {
		input string
		want  float32
	}{
		{input: "", want: cfg.Zoom},
		{input: "default", want: cfg.Zoom},
		{input: "min", want: cfg.ZoomMin},
		{input: "zmin", want: cfg.ZoomMin},
		{input: "max", want: cfg.ZoomMax},
		{input: "zmax", want: cfg.ZoomMax},
		{input: "below-min", want: cfg.ZoomMin * 0.5},
		{input: "above-max", want: cfg.ZoomMax * 2},
		{input: "1700", want: 1700},
	}
	for _, tc := range cases {
		got, err := cameraZoomRequest(tc.input, cfg)
		t.Logf("FSV camera zoom request input=%q got=%.3f err=%v", tc.input, got, err)
		if err != nil || got != tc.want {
			t.Fatalf("cameraZoomRequest(%q) = %.3f, %v; want %.3f nil", tc.input, got, err, tc.want)
		}
	}
	if got, err := cameraZoomRequest("bogus", cfg); err == nil {
		t.Fatalf("invalid zoom accepted: got %.3f", got)
	} else {
		t.Logf("FSV camera invalid zoom input=%q err=%v", "bogus", err)
	}
}
