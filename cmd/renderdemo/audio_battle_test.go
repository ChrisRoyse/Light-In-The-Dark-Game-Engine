package main

import (
	"testing"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/g3n/engine/core"
)

func TestBuildBattle500FSV(t *testing.T) {
	spec, dump, err := buildBattle500FSV(core.NewNode())
	if err != nil {
		t.Fatalf("battle500 rejected: %v", err)
	}
	if spec.name != "battle500" || spec.expected.VisibleGraphics != battle500Units+1 {
		t.Fatalf("battle500 render spec mismatch: %+v", spec)
	}
	if !dump.OK {
		t.Fatalf("battle500 voice dump failed: %+v", dump.Errors)
	}
	if dump.Units != 500 || dump.Columns != 25 || dump.Rows != 20 {
		t.Fatalf("battle500 visual context wrong: units=%d cols=%d rows=%d", dump.Units, dump.Columns, dump.Rows)
	}
	if dump.Volley.Admitted != litaudio.MaxConcurrentPerAsset || dump.Volley.Coalesced != 40-litaudio.MaxConcurrentPerAsset || dump.Volley.ActiveInstances != litaudio.MaxConcurrentPerAsset {
		t.Fatalf("volley mismatch: %+v", dump.Volley)
	}
	if dump.MaxWorldVoices != litaudio.WorldVoices || dump.MaxUIVoices != litaudio.UIVoices || dump.MaxTotalVoices != litaudio.TotalVoices {
		t.Fatalf("voice count maxima wrong: world=%d ui=%d total=%d", dump.MaxWorldVoices, dump.MaxUIVoices, dump.MaxTotalVoices)
	}
	if !dump.UI.OK || dump.UI.Outcome != litaudio.Admitted.String() || dump.UI.WorldBefore != litaudio.WorldVoices {
		t.Fatalf("UI partition edge mismatch: %+v", dump.UI)
	}
	if !dump.Alert.OK || dump.Alert.Outcome != litaudio.Stolen.String() || dump.Alert.FadeMs != litaudio.FadeMs || dump.Alert.VictimCue == 0 {
		t.Fatalf("alert steal edge mismatch: %+v", dump.Alert)
	}
	if !dump.EqualPriority.OK || dump.EqualPriority.NearOutcome != litaudio.Stolen.String() || dump.EqualPriority.FarOutcome != litaudio.Dropped.String() {
		t.Fatalf("equal priority edge mismatch: %+v", dump.EqualPriority)
	}
	if !dump.DistanceCull.OK || dump.DistanceCull.Outcome != litaudio.CulledDistance.String() || dump.DistanceCull.ActiveAfter != 0 {
		t.Fatalf("distance cull edge mismatch: %+v", dump.DistanceCull)
	}
	if !dump.LateRetrigger.OK || dump.LateRetrigger.Inside != litaudio.Coalesced.String() || dump.LateRetrigger.Late != litaudio.Dropped.String() {
		t.Fatalf("late retrigger edge mismatch: %+v", dump.LateRetrigger)
	}
	if !dump.Budget.OK || dump.Budget.World != litaudio.WorldVoices || dump.Budget.UI != litaudio.UIVoices || dump.Budget.Stream != litaudio.StreamVoices || dump.Budget.Total != litaudio.TotalVoices || dump.Budget.ExtraOutcome != litaudio.Dropped.String() {
		t.Fatalf("full budget edge mismatch: %+v", dump.Budget)
	}
	if !dump.ZeroAlloc || dump.AllocsPerRun != 0 {
		t.Fatalf("admission must be zero alloc: %.3f", dump.AllocsPerRun)
	}
	if len(dump.Events) < 40 || len(dump.VoiceCountTrace) != len(dump.Events) {
		t.Fatalf("event log/trace missing: events=%d trace=%d", len(dump.Events), len(dump.VoiceCountTrace))
	}
	t.Logf("FSV #230 battle500: units=%d volley admitted=%d coalesced=%d max world/ui/total=%d/%d/%d alert victim=%d fade=%dms near=%s far=%s late=%s budget=%d allocs=%.1f",
		dump.Units, dump.Volley.Admitted, dump.Volley.Coalesced, dump.MaxWorldVoices, dump.MaxUIVoices, dump.MaxTotalVoices,
		dump.Alert.VictimCue, dump.Alert.FadeMs, dump.EqualPriority.NearOutcome, dump.EqualPriority.FarOutcome, dump.LateRetrigger.Late, dump.Budget.Total, dump.AllocsPerRun)
}
