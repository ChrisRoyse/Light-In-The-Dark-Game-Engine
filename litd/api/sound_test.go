package litd

// #244 sound + music public-API FSV. Audio is presentation-only and must be
// sim-inert (R-AUD-1). Two SoTs: (a) the state hash, which must be byte-
// identical before and after any audio script (audio cannot touch the sim);
// (b) a recording sink installed via Game.OnAudio, which captures the resolved
// (clamped) events so the no-op path is still observable.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func soundWorld(t *testing.T) (*sim.World, *Game, Unit) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	return w, g, Unit{id: id, g: g}
}

func hashTop(w *sim.World) uint64 {
	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap.Top
}

// TestAudioSimInertAndClampFSV — an audio script never changes the state hash,
// and the sink receives clamped values.
func TestAudioSimInertAndClampFSV(t *testing.T) {
	w, g, u := soundWorld(t)
	var rec []AudioEvent
	g.OnAudio(func(ev AudioEvent) { rec = append(rec, ev) })

	before := hashTop(w)
	snd := g.CreateSound("Spells/footman-attack")
	if !snd.Valid() {
		t.Fatal("CreateSound returned invalid handle")
	}
	snd.Play()
	snd.PlayAt(Vec2{X: 100, Y: 200}, 5)
	snd.PlayOn(u)
	snd.SetVolume(1.5)  // over-range → clamp to 1.0
	snd.SetVolume(-0.5) // under-range → clamp to 0.0
	snd.SetPitch(10)    // over-range → clamp to 4.0
	snd.Stop()
	g.PlayMusic("Music/vigil-theme")
	g.SetMusicVolume(2.0)                  // → 1.0
	g.SetChannelVolume(ChannelAmbient, -1) // → 0.0
	g.StopMusic()
	after := hashTop(w)

	t.Logf("FSV sim-inert: hash before=%016x after=%016x (want identical)", before, after)
	if before != after {
		t.Fatalf("audio mutated sim state: %016x -> %016x", before, after)
	}

	// SoT: the recorded (resolved) events.
	t.Logf("FSV recorded %d events", len(rec))
	want := []struct {
		k AudioEventKind
		v float64
	}{
		{AudioPlay, 1}, {AudioPlayAt, 1}, {AudioPlayOn, 1},
		{AudioSetVolume, 1}, {AudioSetVolume, 0}, {AudioSetPitch, 0},
		{AudioStop, 0}, {AudioPlayMusic, 0},
		{AudioSetMusicVolume, 1}, {AudioSetChannelVolume, 0}, {AudioStopMusic, 0},
	}
	if len(rec) != len(want) {
		t.Fatalf("recorded %d events, want %d: %+v", len(rec), len(want), rec)
	}
	for i, wnt := range want {
		if rec[i].Kind != wnt.k {
			t.Fatalf("event %d kind=%d want %d", i, rec[i].Kind, wnt.k)
		}
		if wnt.k == AudioSetVolume || wnt.k == AudioSetMusicVolume || wnt.k == AudioSetChannelVolume {
			t.Logf("FSV clamp evt %d: volume=%.2f (want %.2f)", i, rec[i].Volume, wnt.v)
			if rec[i].Volume != wnt.v {
				t.Fatalf("event %d volume=%.2f want %.2f", i, rec[i].Volume, wnt.v)
			}
		}
	}
	// pitch clamp + positional payload
	if rec[5].Pitch != 4 {
		t.Fatalf("pitch over-range not clamped to 4: %.3f", rec[5].Pitch)
	}
	if !rec[1].HasPos || rec[1].Pos.X != 100 || rec[1].Z != 5 {
		t.Fatalf("PlayAt payload wrong: %+v", rec[1])
	}
	if rec[2].Target.id != u.id {
		t.Fatal("PlayOn did not carry the target unit")
	}
}

// TestAudioHeadlessNoOpFSV — with no sink (the headless default) every verb is
// a silent no-op: no panic, no recorded events, hash unchanged.
func TestAudioHeadlessNoOpFSV(t *testing.T) {
	w, g, u := soundWorld(t)
	before := hashTop(w)
	snd := g.CreateSound("x")
	snd.Play()
	snd.PlayAt(Vec2{X: 1, Y: 2}, 0)
	snd.PlayOn(u)
	snd.SetVolume(0.5)
	snd.Stop()
	g.PlayMusic("y")
	g.SetChannelVolume(ChannelUI, 0.3)
	g.StopMusic()
	after := hashTop(w)
	t.Logf("FSV headless no-op: hash before=%016x after=%016x", before, after)
	if before != after {
		t.Fatal("headless audio touched the sim")
	}
}

// TestAudioInvalidHandleFSV — zero/invalid handles are safe no-ops and (in
// debug) report the call site; nothing reaches the sink.
func TestAudioInvalidHandleFSV(t *testing.T) {
	_, g, _ := soundWorld(t)
	var rec []AudioEvent
	g.OnAudio(func(ev AudioEvent) { rec = append(rec, ev) })
	var reports []string
	g.OnInvalidHandle(func(r string) { reports = append(reports, r) })
	g.SetDebug(true)

	// zero-value handle: no game pointer → a pure no-op, cannot report.
	var zero Sound
	zero.Play()
	zero.SetVolume(0.5)
	zero.Stop()
	// game-bound but invalid handle (id 0): no-op that DOES report in debug.
	bad := Sound{g: g}
	bad.Play()
	bad.SetVolume(0.5)
	if g.CreateSound("").Valid() {
		t.Fatal("empty cue should yield invalid Sound")
	}
	t.Logf("FSV invalid: recorded=%d reports=%v", len(rec), reports)
	if len(rec) != 0 {
		t.Fatalf("invalid-handle verbs reached the sink: %+v", rec)
	}
	if !reportsContain(reports, "Sound.Play") {
		t.Fatalf("debug report missing Sound.Play: %v", reports)
	}
}
