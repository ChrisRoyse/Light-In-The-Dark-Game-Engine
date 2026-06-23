package main

// #538 FSV — the renderbench audio-load model. SoT = the live VoiceCount the audio
// Manager reports (len(m.voices)) after one frame of admission for a scenario, which
// is what the bench writes into the per-frame voiceCount column. X+X=Y: a known
// scenario's positional cues => a known concurrent-voice count, capped by the audio
// system's world-voice partition. Headless (null backend, no GL, no assets).

import (
	"testing"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

func TestDriveAudioLoadFSV(t *testing.T) {
	// battle500: 60 skinned units, all within the audible falloff of a centre
	// listener. They are positional (PartitionWorld), so the count saturates the
	// 24-voice world partition, NOT the 32 total pool. The expected value is the
	// structural cap litaudio.WorldVoices, verified by reading it from the SoT.
	sc, err := scenarioFor("battle500")
	if err != nil {
		t.Fatal(err)
	}
	m := litaudio.NewManager(nil)
	m.SetListener(litaudio.Vec3{X: 0, Y: 0, Z: 0})
	defer m.Close()

	t.Logf("FSV BEFORE: voices=%d (fresh manager)", m.Dump().VoiceCount)
	got := driveAudioLoad(m, sc)
	t.Logf("FSV AFTER battle500 (%d units): voiceCount=%d, world-partition cap=%d", sc.SkinnedUnits, got, litaudio.WorldVoices)

	if got != litaudio.WorldVoices {
		t.Fatalf("voiceCount=%d, want %d (60 positional cues must saturate the world-voice partition)", got, litaudio.WorldVoices)
	}

	// Idempotent across frames: re-driving the SAME scene coalesces (stable cues),
	// so the count holds steady rather than growing — the bench reports a flat line.
	got2 := driveAudioLoad(m, sc)
	if got2 != litaudio.WorldVoices {
		t.Fatalf("second frame voiceCount=%d, want steady %d (cues must coalesce, not restart)", got2, litaudio.WorldVoices)
	}
	t.Logf("FSV steady: second frame voiceCount=%d (coalesced, no growth)", got2)
}

// Edge: a scene with FEWER units than the partition cap reports exactly that many
// voices (the cap does not bind) — and an empty scene reports zero, not a
// fabricated number.
func TestDriveAudioLoadBelowCapAndEmptyFSV(t *testing.T) {
	m := litaudio.NewManager(nil)
	m.SetListener(litaudio.Vec3{X: 0, Y: 0, Z: 0})
	defer m.Close()

	// Below cap: 5 units near the listener => exactly 5 voices.
	small := benchScenario{Name: "synthetic-small", SkinnedUnits: 5, Columns: 5}
	if got := driveAudioLoad(m, small); got != 5 {
		t.Fatalf("5-unit scene voiceCount=%d, want 5 (below the %d cap)", got, litaudio.WorldVoices)
	}
	t.Logf("FSV below-cap: 5 units => voiceCount=5")

	// Empty: a fresh manager, no units => zero voices (honest, not n/a-as-number).
	m2 := litaudio.NewManager(nil)
	m2.SetListener(litaudio.Vec3{X: 0, Y: 0, Z: 0})
	defer m2.Close()
	empty := benchScenario{Name: "synthetic-empty", SkinnedUnits: 0, Columns: 1}
	if got := driveAudioLoad(m2, empty); got != 0 {
		t.Fatalf("empty scene voiceCount=%d, want 0", got)
	}
	t.Logf("FSV empty: 0 units => voiceCount=0")
}
