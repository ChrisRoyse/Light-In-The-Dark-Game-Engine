package render

import (
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/core"
)

func TestSunAmbientLightingFromMapFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	cfg, err := LightingConfigFromMap(m)
	if err != nil {
		t.Fatal(err)
	}
	if cfg != DefaultLightingConfig() {
		t.Fatalf("test64 should use default lighting config: %+v want %+v", cfg, DefaultLightingConfig())
	}
	scene := core.NewNode()
	t.Logf("FSV lighting scene BEFORE children=%d config=%+v", len(scene.Children()), cfg)
	lights, err := AddSunAmbient(scene, cfg)
	if err != nil {
		t.Fatal(err)
	}
	snap := SnapshotSceneLighting(scene, cfg)
	t.Logf("FSV lighting scene AFTER children=%d sunPos=%+v snapshot=%+v", len(scene.Children()), lights.Sun.Position(), snap)
	if !snap.OK || len(snap.Lights) != 2 || snap.Lights[0].Kind != "Directional" || snap.Lights[1].Kind != "Ambient" {
		t.Fatalf("scene light snapshot wrong: %+v", snap)
	}
	if snap.Lights[0].Intensity != cfg.SunIntensity || snap.Lights[1].Intensity != cfg.AmbientIntensity {
		t.Fatalf("light intensity mismatch: %+v cfg=%+v", snap.Lights, cfg)
	}
}

func TestSunPositionFSV(t *testing.T) {
	pos := SunPosition(90, 0)
	t.Logf("FSV sun position az=90 el=0 AFTER pos=%+v", pos)
	if !close32(pos.X, 100) || !close32(pos.Y, 0) || !close32(pos.Z, 0) {
		t.Fatalf("sun position for az=90 el=0 = %+v, want x=100 y=0 z=0", pos)
	}
}

func TestLightingEdgesFSV(t *testing.T) {
	if _, err := LightingConfigFromMap(nil); err == nil {
		t.Fatal("nil map accepted")
	} else {
		t.Logf("FSV lighting nil map AFTER err=%v", err)
	}

	scene := core.NewNode()
	beforeChildren := len(scene.Children())
	badIntensity := DefaultLightingConfig()
	badIntensity.AmbientIntensity = -0.1
	if _, err := AddSunAmbient(scene, badIntensity); err == nil {
		t.Fatal("negative ambient intensity accepted")
	} else {
		t.Logf("FSV lighting bad intensity BEFORE children=%d AFTER children=%d err=%v", beforeChildren, len(scene.Children()), err)
	}
	if len(scene.Children()) != beforeChildren {
		t.Fatalf("invalid lighting mutated scene: before=%d after=%d", beforeChildren, len(scene.Children()))
	}

	badAzimuth := DefaultLightingConfig()
	badAzimuth.SunAzimuth = 360
	if _, err := NewSunAmbientLights(badAzimuth); err == nil {
		t.Fatal("sun azimuth 360 accepted")
	} else {
		t.Logf("FSV lighting bad azimuth BEFORE cfg=%+v AFTER err=%v", badAzimuth, err)
	}

	if _, err := AddSunAmbient(scene, DefaultLightingConfig()); err != nil {
		t.Fatal(err)
	}
	afterFirst := SnapshotSceneLighting(scene, DefaultLightingConfig())
	if _, err := AddSunAmbient(scene, DefaultLightingConfig()); err == nil {
		t.Fatal("duplicate persistent lights accepted")
	} else {
		t.Logf("FSV lighting duplicate BEFORE snapshot=%+v AFTER children=%d err=%v", afterFirst, len(scene.Children()), err)
	}
	if len(scene.Children()) != 2 {
		t.Fatalf("duplicate add mutated scene children=%d", len(scene.Children()))
	}
}
