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
