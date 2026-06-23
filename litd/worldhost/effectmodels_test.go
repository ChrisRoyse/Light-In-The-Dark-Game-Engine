package worldhost_test

// #530 end-to-end: an authored world whose data declares special-effect models
// and whose main.lua calls Game_AddSpecialEffect resolves to LIVE effect handles
// after load. SoT = host.Game.Effects() (the #529 enumeration) cross-checked
// against the coords main.lua spawned. Before #530 worldhost never registered
// effect models, so those calls failed closed and Effects() was empty.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func TestEffectModelsRegisterFromWorldDataFSV(t *testing.T) {
	host, err := worldhost.Load("../../worlds/effect-demo", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load effect-demo: %v", err)
	}
	defer host.Close()
	g := host.Game

	// SoT: main.lua spawned "fx/glow" at (100,200) and "fx/spark" at (300,400).
	effs := g.Effects()
	t.Logf("FSV: Effects() = %d after load (want 2)", len(effs))
	if len(effs) != 2 {
		t.Fatalf("Effects() = %d, want 2 (AddSpecialEffect must resolve from world data)", len(effs))
	}
	got := map[api.Vec2]bool{}
	for _, e := range effs {
		p := e.Position()
		t.Logf("  effect id=%d pos=(%.1f,%.1f)", e.ID(), p.X, p.Y)
		got[p] = true
	}
	for _, want := range []api.Vec2{{X: 100, Y: 200}, {X: 300, Y: 400}} {
		if !got[want] {
			t.Fatalf("no effect at %+v — registered models did not produce the spawn", want)
		}
	}

	// Edge — unknown model key fails closed: an unregistered key yields no handle
	// and does NOT add an effect (the count stays 2).
	if bad := g.AddSpecialEffect("fx/unregistered", api.Vec2{X: 5, Y: 5}); bad.Valid() {
		t.Fatal("AddSpecialEffect with an unregistered key returned a valid handle, want fail-closed")
	}
	if n := len(g.Effects()); n != 2 {
		t.Fatalf("unknown-key spawn changed effect count to %d, want 2", n)
	}
	t.Log("FSV: unknown key fails closed, count unchanged")

	// Edge — a registered key DOES spawn (proves the registry, not a fluke): one
	// more "fx/glow" -> count 3.
	if more := g.AddSpecialEffect("fx/glow", api.Vec2{X: 7, Y: 7}); !more.Valid() {
		t.Fatal("AddSpecialEffect with a registered key returned invalid, want a live handle")
	}
	if n := len(g.Effects()); n != 3 {
		t.Fatalf("registered-key spawn produced %d effects, want 3", n)
	}
	t.Log("FSV: registered key spawns, count 2 -> 3")
}
