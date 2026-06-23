package main

import (
	"testing"

	"github.com/g3n/engine/core"
)

func TestRenderBenchScenePoliciesFSV(t *testing.T) {
	cases := []struct {
		name          string
		scene         string
		variant       string
		wantRigid     int
		wantSkinned   int
		wantWorldDraw int
		wantRecovered int
	}{
		{name: "battle500-baseline", scene: "battle500", variant: variantBaseline, wantRigid: 440, wantSkinned: 60, wantWorldDraw: 501, wantRecovered: 428},
		{name: "battle500-floor", scene: "battle500", variant: variantFloor, wantRigid: 440, wantSkinned: 60, wantWorldDraw: 73, wantRecovered: 428},
		{name: "battle1000-baseline", scene: "battle1000", variant: variantBaseline, wantRigid: 880, wantSkinned: 120, wantWorldDraw: 1001, wantRecovered: 868},
		{name: "battle1000-floor", scene: "battle1000", variant: variantFloor, wantRigid: 880, wantSkinned: 120, wantWorldDraw: 133, wantRecovered: 868},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			sc, err := scenarioFor(tc.scene)
			if err != nil {
				t.Fatal(err)
			}
			dump, err := buildScene(core.NewNode(), sc, tc.variant)
			if err != nil {
				t.Fatal(err)
			}
			t.Logf("FSV renderbench synthetic %s AFTER dump=%+v", tc.name, dump)
			if dump.RigidInstances != tc.wantRigid || dump.SkinnedUnits != tc.wantSkinned {
				t.Fatalf("scenario counts wrong: %+v", dump)
			}
			if dump.ExpectedWorldDraws != tc.wantWorldDraw {
				t.Fatalf("world draw target = %d, want %d", dump.ExpectedWorldDraws, tc.wantWorldDraw)
			}
			if dump.Policy.RecoveredDraws != tc.wantRecovered {
				t.Fatalf("recovered draws = %d, want %d", dump.Policy.RecoveredDraws, tc.wantRecovered)
			}
			if dump.Policy.SkinnedInstanced {
				t.Fatalf("floor should not mark skinned units as instanced: %+v", dump.Policy)
			}
			if got := len(dump.AnimationSamples); got != 4 {
				t.Fatalf("animation samples = %d, want 4", got)
			}
			if dump.AnimationSamples[0].Clip != "Idle" || dump.AnimationSamples[1].Clip != "Walk" ||
				dump.AnimationSamples[2].Clip != "Attack" || dump.AnimationSamples[3].Clip != "Death" {
				t.Fatalf("animation samples did not cover idle/walk/attack/death: %+v", dump.AnimationSamples)
			}
			if len(dump.DeathTrace) != 3 || dump.DeathTrace[1].Time != 0.8 || dump.DeathTrace[2].Fade >= 1 {
				t.Fatalf("death trace did not hold clamped death clip with fade-out: %+v", dump.DeathTrace)
			}
		})
	}
}

// #233 — the three M4 acceptance segments. SoT = the synthetic scenario counts
// the bench will render (unit totals per segment) + the stress segment's
// spell-storm light count. CPU-only (buildScene on a bare node); the GL keyframe
// FSV is added with the headless render path in a later slice.
func TestRenderBenchSegmentsFSV(t *testing.T) {
	cases := []struct {
		scene      string
		segment    string
		wantUnits  int // rigid + skinned
		wantLights int
	}{
		{"typical", "typical", 200, 0},
		{"max-battle", "max-battle", 500, 0},
		{"stress", "stress", 1000, 8},
	}
	for _, tc := range cases {
		t.Run(tc.scene, func(t *testing.T) {
			sc, err := scenarioFor(tc.scene)
			if err != nil {
				t.Fatal(err)
			}
			dump, err := buildScene(core.NewNode(), sc, variantFloor)
			if err != nil {
				t.Fatal(err)
			}
			units := dump.RigidInstances + dump.SkinnedUnits
			t.Logf("FSV segment %s AFTER units=%d (rigid=%d skinned=%d) lights=%d segment=%q",
				tc.scene, units, dump.RigidInstances, dump.SkinnedUnits, dump.Lights, dump.Segment)
			if units != tc.wantUnits {
				t.Fatalf("segment %s units = %d, want %d", tc.scene, units, tc.wantUnits)
			}
			if dump.Lights != tc.wantLights {
				t.Fatalf("segment %s lights = %d, want %d", tc.scene, dump.Lights, tc.wantLights)
			}
			if dump.Segment != tc.segment {
				t.Fatalf("segment label = %q, want %q", dump.Segment, tc.segment)
			}
			// Overlays (fog + health bars + minimap) are on for max-battle only.
			wantOverlay := tc.scene == "max-battle"
			if dump.OverlayFog != wantOverlay || dump.OverlayMinimap != wantOverlay {
				t.Fatalf("segment %s overlays fog=%v minimap=%v, want %v", tc.scene, dump.OverlayFog, dump.OverlayMinimap, wantOverlay)
			}
			if wantOverlay {
				if dump.OverlayBars != dump.SkinnedUnits {
					t.Fatalf("max-battle bars = %d, want %d (one per skinned unit)", dump.OverlayBars, dump.SkinnedUnits)
				}
				if dump.MinimapBlips != dump.RigidInstances+dump.SkinnedUnits {
					t.Fatalf("max-battle minimap blips = %d, want %d", dump.MinimapBlips, dump.RigidInstances+dump.SkinnedUnits)
				}
			} else if dump.OverlayBars != 0 || dump.MinimapBlips != 0 {
				t.Fatalf("segment %s has unexpected overlays bars=%d blips=%d", tc.scene, dump.OverlayBars, dump.MinimapBlips)
			}
		})
	}
}

func TestRenderBenchEdgesFSV(t *testing.T) {
	if _, err := scenarioFor("missing"); err == nil {
		t.Fatal("unknown scene accepted")
	} else {
		t.Logf("FSV renderbench unknown scene AFTER err=%v", err)
	}
	sc, err := scenarioFor("battle500")
	if err != nil {
		t.Fatal(err)
	}
	if got, err := buildScene(core.NewNode(), sc, "vat"); err == nil {
		t.Fatalf("unknown variant accepted: %+v", got)
	} else {
		t.Logf("FSV renderbench unknown variant AFTER err=%v", err)
	}
	sc.Columns = 0
	if got, err := buildScene(core.NewNode(), sc, variantFloor); err == nil {
		t.Fatalf("zero-column scene accepted: %+v", got)
	} else {
		t.Logf("FSV renderbench zero columns AFTER err=%v", err)
	}

	x0, z0 := gridPos(0, 4, 8, 10, 100)
	x7, z7 := gridPos(7, 4, 8, 10, 100)
	t.Logf("FSV renderbench grid BEFORE total=8 AFTER first=(%.1f,%.1f) last=(%.1f,%.1f)", x0, z0, x7, z7)
	if x0 != -15 || z0 != 90 || x7 != 15 || z7 != 100 {
		t.Fatalf("grid centering wrong: first=(%.1f,%.1f) last=(%.1f,%.1f)", x0, z0, x7, z7)
	}
}
