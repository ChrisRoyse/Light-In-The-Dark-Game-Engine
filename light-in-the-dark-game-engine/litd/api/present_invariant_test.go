package litd

// FSV for the #449/#471 invariant: presentation consumers are non-hashing. An
// audio-on game (sinks installed AND firing) must produce the SAME Game.StateHash
// as an audio-off game over identical sim input. SoT = the two StateHash values.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func presentGame(t *testing.T) *Game {
	t.Helper()
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 1234})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	return g
}

// runMatch drives an identical scripted match: spawn a unit, kill it, advance.
func runMatch(t *testing.T, g *Game) {
	t.Helper()
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), Vec2{X: 50, Y: 50}, Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	g.Advance(2)
	u.Kill()
	g.Advance(3)
}

// TestPresentationConsumersDoNotHashFSV — audio-on vs audio-off, equal hash.
func TestPresentationConsumersDoNotHashFSV(t *testing.T) {
	// audio-OFF: no presentation sinks, no presentation calls.
	gOff := presentGame(t)
	runMatch(t, gOff)
	hOff := gOff.StateHash()

	// audio-ON: install firing sinks and exercise the presentation surface
	// (PlayMusic → emitAudio → sink) during the SAME scripted match.
	gOn := presentGame(t)
	audioCalls, camCalls := 0, 0
	gOn.OnAudio(func(AudioEvent) { audioCalls++ })
	gOn.OnCamera(func(CameraEvent) { camCalls++ })
	u := gOn.CreateUnit(gOn.Player(1), gOn.UnitType("hfoo"), Vec2{X: 50, Y: 50}, Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	gOn.PlayMusic("battle") // fires the audio sink — must not perturb the sim
	gOn.SetMusicVolume(0.5)
	gOn.Advance(2)
	u.Kill()
	gOn.Advance(3)
	hOn := gOn.StateHash()

	t.Logf("FSV #449: hashOff=%#016x hashOn=%#016x audioCalls=%d camCalls=%d", hOff, hOn, audioCalls, camCalls)
	if audioCalls == 0 {
		t.Fatal("audio sink never fired — invariant test is vacuous (consumers not active)")
	}
	if hOn != hOff {
		t.Fatalf("HASH DIVERGENCE: audio-on %#016x != audio-off %#016x — a presentation consumer perturbed the sim (#449)", hOn, hOff)
	}
	t.Log("FSV #449 holds: active audio/camera consumers leave Game.StateHash identical")
}
