package audio

// #227 end-to-end FSV: the real trigger→process→outcome chain, public API only.
// Trigger: a script calls Sound.PlayAt on a live api.Game. Process: the Game emits
// a resolved AudioEvent to the sink installed via OnAudio. Outcome (SoT): the
// Manager's voice table reflects it with the correct resolved gain/pan — AND the
// sim StateHash is byte-identical before and after, proving audio is sim-inert
// (audio-on == audio-off, acceptance §9.1). No GL, no device — the null path.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func TestGameOnAudioToManagerFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	m := NewManager(nil) // device-absent: null backend, full accounting
	g.OnAudio(m.Handle)  // wire the sim's audio sink to the manager

	// Listener at origin; emit a positional sound at (PanWidth, 0) → full-right pan,
	// inverse-distance gain (dist=900 > ref 400 → 400/900). Known in, known out.
	hashBefore := g.StateHash()
	snd := g.CreateSound("Spells/footman-attack")
	if !snd.Valid() {
		t.Fatal("CreateSound returned invalid handle")
	}
	snd.PlayAt(api.Vec2{X: PanWidth, Y: 0}, 0)
	hashAfter := g.StateHash()

	// Outcome SoT #1: the manager actually received and resolved the event.
	s := m.Dump()
	if s.VoiceCount != 1 {
		t.Fatalf("expected 1 voice after PlayAt, got %d (sink not wired?)", s.VoiceCount)
	}
	v := s.Voices[0]
	wantGain := ReferenceDistance / PanWidth
	if !approx(v.Pan, 1.0) || !approx(v.Gain, wantGain) {
		t.Fatalf("resolved voice wrong: want pan=1.0 gain=%v, got pan=%v gain=%v", wantGain, v.Pan, v.Gain)
	}

	// Outcome SoT #2: audio did not touch the sim — hash identical.
	if hashBefore != hashAfter {
		t.Fatalf("audio changed the state hash: before=%016x after=%016x (R-AUD-1 violated)", hashBefore, hashAfter)
	}
	t.Logf("FSV #227 e2e: PlayAt → manager voice pan=%v gain=%v; StateHash %016x unchanged (sim-inert)", v.Pan, v.Gain, hashBefore)
}
