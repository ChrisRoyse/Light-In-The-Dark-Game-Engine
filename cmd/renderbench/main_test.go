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
			dump, err := buildScene(core.NewNode(), sc, tc.variant, matUnlit)
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
			dump, err := buildScene(core.NewNode(), sc, variantFloor, matUnlit)
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

// #233 slice 3 — the {PBR,unlit} material axis. SoT = the dump material path +
// the world-draw target, which must be identical across material paths (same
// geometry, only the shader differs). The projection axis is camera-only and
// exercised by the headless render path. Bad paths must reject.
func TestRenderBenchMaterialMatrixFSV(t *testing.T) {
	sc, err := scenarioFor("max-battle")
	if err != nil {
		t.Fatal(err)
	}
	var target int
	for i, mp := range []string{matUnlit, matPBR} {
		dump, err := buildScene(core.NewNode(), sc, variantFloor, mp)
		if err != nil {
			t.Fatalf("material %q: %v", mp, err)
		}
		t.Logf("FSV material %q AFTER matPath=%q expectedWorldDraws=%d", mp, dump.MaterialPath, dump.ExpectedWorldDraws)
		if dump.MaterialPath != mp {
			t.Fatalf("material path = %q, want %q", dump.MaterialPath, mp)
		}
		if i == 0 {
			target = dump.ExpectedWorldDraws
		} else if dump.ExpectedWorldDraws != target {
			t.Fatalf("material %q world-draw target = %d, want %d (geometry must be path-invariant)", mp, dump.ExpectedWorldDraws, target)
		}
	}
	if got, err := buildScene(core.NewNode(), sc, variantFloor, "ray-trace"); err == nil {
		t.Fatalf("unknown material path accepted: %+v", got)
	} else {
		t.Logf("FSV unknown material path AFTER err=%v", err)
	}
}

// #233 slice 4c regression — validateDump must use the unculled reference frame,
// not the last frame. Bug: asserting last-frame opaqueDrawCalls == ExpectedWorldDraws
// flagged a false failure when an ortho zoom-in legitimately culled whole instanced
// batches on later frames (stress/ortho: draws 133 -> 130). Correct invariant:
// frame 0 == floor, and no frame exceeds the floor.
func TestValidateDumpCullingInvariantFSV(t *testing.T) {
	// A valid dump skeleton so only the draw invariant is under test.
	base := func(perFrame []frameStat) *benchDump {
		return &benchDump{
			ExpectedWorldDraws: 133,
			PerFrame:           perFrame,
			AnimationSamples:   make([]animationSample, 4),
			DeathTrace: []animationSample{
				{Time: 0}, {Time: 0.8}, {Fade: 0.5},
			},
		}
	}

	// Happy path: frame 0 at the floor, later frames culled below it → accepted.
	ok := base([]frameStat{
		{Frame: 0, OpaqueDraws: intPtr(133)},
		{Frame: 1, OpaqueDraws: intPtr(131)},
		{Frame: 2, OpaqueDraws: intPtr(130)},
	})
	validateDump(ok)
	t.Logf("FSV culling happy AFTER ok=%v errors=%v", ok.OK, ok.Errors)
	if !ok.OK {
		t.Fatalf("culled-below-floor run rejected: %v", ok.Errors)
	}

	// Edge 1: frame 0 below the floor → the reference frame is wrong, reject.
	bad0 := base([]frameStat{{Frame: 0, OpaqueDraws: intPtr(130)}, {Frame: 1, OpaqueDraws: intPtr(130)}})
	validateDump(bad0)
	t.Logf("FSV culling bad-frame0 AFTER ok=%v errors=%v", bad0.OK, bad0.Errors)
	if bad0.OK {
		t.Fatal("frame-0-below-floor accepted; reference-frame invariant not enforced")
	}

	// Edge 2: a frame exceeding the floor → culling cannot add draws, reject.
	over := base([]frameStat{{Frame: 0, OpaqueDraws: intPtr(133)}, {Frame: 1, OpaqueDraws: intPtr(140)}})
	validateDump(over)
	t.Logf("FSV culling over-floor AFTER ok=%v errors=%v", over.OK, over.Errors)
	if over.OK {
		t.Fatal("frame-above-floor accepted; culling-only-reduces invariant not enforced")
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
	if got, err := buildScene(core.NewNode(), sc, "vat", matUnlit); err == nil {
		t.Fatalf("unknown variant accepted: %+v", got)
	} else {
		t.Logf("FSV renderbench unknown variant AFTER err=%v", err)
	}
	sc.Columns = 0
	if got, err := buildScene(core.NewNode(), sc, variantFloor, matUnlit); err == nil {
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
