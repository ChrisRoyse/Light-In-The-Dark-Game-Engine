package main

import (
	"strings"
	"testing"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

func TestBuildAudioDomainDumpFSV(t *testing.T) {
	dump, err := buildAudioDomainDump("basecamp")
	if err != nil {
		t.Fatalf("audio domain dump rejected: %v", err)
	}
	if !dump.OK {
		t.Fatalf("audio domain dump failed: %+v", dump.Errors)
	}
	if dump.Scene != "basecamp" || dump.ReferenceDistance != litaudio.ReferenceDistance || dump.MaxAudibleDistance != litaudio.MaxAudibleDistance {
		t.Fatalf("domain constants missing/wrong: %+v", dump)
	}
	if len(dump.SoundTable) != 10 {
		t.Fatalf("sound table rows = %d, want 10", len(dump.SoundTable))
	}
	byLabel := map[string]audioDomainPlaybackDump{}
	for _, p := range dump.Playbacks {
		byLabel[p.Label] = p
	}
	center := byLabel["world-camera-center"]
	edge := byLabel["world-viewport-edge"]
	oneHalf := byLabel["world-one-point-five-screens"]
	far := byLabel["world-three-screens"]
	uiClick := byLabel["ui-click"]
	uiStinger := byLabel["ui-stinger-camera-far"]
	left := byLabel["world-hard-left"]
	right := byLabel["world-hard-right"]

	if center.Outcome != "admitted" || !audioDomainApprox(center.Gain, 1) || !audioDomainApprox(center.Pan, 0) {
		t.Fatalf("world center wrong: %+v", center)
	}
	if edge.Outcome != "admitted" || !audioDomainApprox(edge.Gain, 1) {
		t.Fatalf("world edge wrong: %+v", edge)
	}
	wantOneHalf := litaudio.ReferenceDistance / litaudio.MaxAudibleDistance
	if oneHalf.Outcome != "admitted" || !audioDomainApprox(oneHalf.Gain, wantOneHalf) || !audioDomainApprox(oneHalf.Pan, 1) {
		t.Fatalf("world one-point-five wrong: %+v want gain %.6f pan 1", oneHalf, wantOneHalf)
	}
	if far.Outcome != litaudio.CulledDistance.String() || !far.Culled || dump.Snapshot.Culled != 1 {
		t.Fatalf("world far cull wrong: far=%+v snapshot=%+v", far, dump.Snapshot)
	}
	if uiClick.Outcome != "admitted" || uiClick.Domain != "ui" || !audioDomainApprox(uiClick.Gain, 1) || !audioDomainApprox(uiClick.Pan, 0) {
		t.Fatalf("UI click wrong: %+v", uiClick)
	}
	if uiStinger.Outcome != "admitted" || uiStinger.Domain != "ui" || uiStinger.Channel != "effects" || !audioDomainApprox(uiStinger.Gain, 1) || !audioDomainApprox(uiStinger.Pan, 0) {
		t.Fatalf("far UI stinger wrong: %+v", uiStinger)
	}
	if left.Pan >= 0 || right.Pan <= 0 {
		t.Fatalf("pan signs did not flip: left=%+v right=%+v", left, right)
	}
	if !dump.VolumeGroup.OK || !audioDomainApprox(dump.VolumeGroup.WorldGainAfter, 0) || !audioDomainApprox(dump.VolumeGroup.UIGainAfter, 1) {
		t.Fatalf("volume group edge wrong: %+v", dump.VolumeGroup)
	}
	if !dump.AssetcheckRejection.OK || !strings.Contains(dump.AssetcheckRejection.Output, "SOUND-CLASS") {
		t.Fatalf("assetcheck rejection missing: %+v", dump.AssetcheckRejection)
	}
	t.Logf("FSV #231 renderdemo domains: center=%.1f edge=%.1f max=%.4f far=%s uiFar=%.1f pan=%+.1f/%+.1f groupWorldAfter=%.1f assetcheck=%q",
		center.Gain, edge.Gain, oneHalf.Gain, far.Outcome, uiStinger.Gain, left.Pan, right.Pan,
		dump.VolumeGroup.WorldGainAfter, dump.AssetcheckRejection.Output)
}

func TestBuildAudioDomainDumpRequiresBasecampFSV(t *testing.T) {
	dump, err := buildAudioDomainDump("counted")
	if err == nil {
		t.Fatalf("non-basecamp scene accepted: %+v", dump)
	}
	if dump == nil || dump.OK || len(dump.Errors) == 0 {
		t.Fatalf("invalid scene did not produce failed dump: %+v err=%v", dump, err)
	}
	t.Logf("FSV #231 renderdemo domains invalid scene BEFORE scene=counted AFTER ok=%v err=%v", dump.OK, err)
}
