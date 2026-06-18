package luabind

// Flicker-cycle integration FSV (#170 dim-phase empowerment, dogfooding #267):
// loads worlds/flicker-cycle — written purely against the bound surface
// (Game_Every, Game_AllUnits, Unit_HasBuff, Unit_ApplyBuff, Unit_RemoveAllBuffs,
// Storage) — and drives it headlessly. SoT = the phase published to Storage +
// each unit's buff state via the Go api, across ≥2 full cycles. Invariant:
// empowered IFF dim phase.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func flickerGame(t *testing.T, seed int64) (*api.Game, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "dimpwr", DurationTicks: 50, Stacking: data.StackRefresh, MaxStacks: 1},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	return g, u
}

func TestFlickerCycleWorldFSV(t *testing.T) {
	worldDir := filepath.Join("..", "..", "worlds", "flicker-cycle")

	g, u := flickerGame(t, 5)
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, worldDir); err != nil {
		t.Fatalf("LoadWorld(flicker-cycle): %v", err)
	}
	bt := g.BuffType("dimpwr")

	phase := func() int { v, _ := g.Storage().GetInt("flicker", "phase"); return v }

	// Sample across 2.5 cycles; at every sample the invariant must hold:
	// the unit carries the empowerment IFF the published phase is dim(1).
	// 250 ticks at 20tps; sample every 10 ticks.
	const DIM = 1
	dimSamples, brightSamples := 0, 0
	for i := 0; i < 25; i++ {
		g.Advance(10)
		ph := phase()
		has := u.HasBuff(bt)
		if (ph == DIM) != has {
			t.Fatalf("tick~%d: phase=%d HasBuff=%v — empowerment must hold IFF dim", (i+1)*10, ph, has)
		}
		if ph == DIM {
			dimSamples++
		} else {
			brightSamples++
		}
	}
	if dimSamples == 0 || brightSamples == 0 {
		t.Fatalf("did not observe both phases over 2.5 cycles: dim=%d bright=%d", dimSamples, brightSamples)
	}
	trans, _ := g.Storage().GetInt("flicker", "transitions")
	if trans < 4 { // ≥2 full cycles ⇒ ≥4 phase transitions
		t.Fatalf("transitions=%d over 2.5 cycles, want ≥4", trans)
	}
	t.Logf("FSV #170 flicker: empowered IFF dim held over 25 samples (dim=%d bright=%d, transitions=%d)", dimSamples, brightSamples, trans)

	// --- Determinism edge: a second seeded run produces the identical cycle. ---
	g2, _ := flickerGame(t, 5)
	L2 := lua.NewState()
	if err := Register(L2, g2); err != nil {
		t.Fatalf("Register#2: %v", err)
	}
	reg2 := NewChunkRegistry()
	t.Cleanup(func() { L2.Close(); reg2.Close() })
	if _, err := LoadWorld(L2, reg2, worldDir); err != nil {
		t.Fatalf("LoadWorld#2: %v", err)
	}
	g2.Advance(250) // run 1 is already at tick 250 from the sampling loop above

	p1v, _ := g.Storage().GetInt("flicker", "phase")
	p2v, _ := g2.Storage().GetInt("flicker", "phase")
	t1v, _ := g.Storage().GetInt("flicker", "transitions")
	t2v, _ := g2.Storage().GetInt("flicker", "transitions")
	if p1v != p2v || t1v != t2v || g.StateHash() != g2.StateHash() {
		t.Fatalf("non-deterministic: run1 phase=%d trans=%d hash=%#x | run2 phase=%d trans=%d hash=%#x",
			p1v, t1v, g.StateHash(), p2v, t2v, g2.StateHash())
	}
	t.Logf("FSV #170 determinism: two seeded runs identical — phase=%d transitions=%d hash=%#x", p1v, t1v, g.StateHash())
}
