package luabind

// Win/lose integration FSV (#200 destruction path, dogfooding #267): loads
// worlds/victory-destruction — written purely against the bound Lua surface
// (Game_AllUnits, Unit_Type, Unit_Owner, Player_Slot, Player_Result,
// Game_Victory/Defeat) — and drives it headlessly. SoT = the sim per-player
// result store (Player.Result via the Go api) after a base is destroyed.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func TestVictoryDestructionWorldFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "hall", Life: 500, TurnRatePerTick: 65535, CollisionSize: 32},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })

	hallT := g.UnitType("hall")
	hall1 := g.CreateUnit(g.Player(1), hallT, api.Vec2{X: 200, Y: 200}, api.Deg(0))
	hall2 := g.CreateUnit(g.Player(2), hallT, api.Vec2{X: 800, Y: 800}, api.Deg(0))
	if !hall1.Valid() || !hall2.Valid() {
		t.Fatal("hall fixtures invalid")
	}

	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "victory-destruction")); err != nil {
		t.Fatalf("LoadWorld(victory-destruction): %v", err)
	}

	// Both halls standing → nobody resolved (teeth).
	g.Advance(20)
	if r1, r2 := g.Player(1).Result(), g.Player(2).Result(); r1 != api.ResultPlaying || r2 != api.ResultPlaying {
		t.Fatalf("premature result with both halls alive: p1=%d p2=%d", int(r1), int(r2))
	}

	// Destroy player 2's hall → p2 defeated, p1 last standing wins.
	hall2.Kill()
	g.Advance(20)

	r1, r2 := g.Player(1).Result(), g.Player(2).Result()
	if r2 != api.ResultLost {
		t.Fatalf("player 2 result = %d after hall destroyed, want Lost(%d)", int(r2), int(api.ResultLost))
	}
	if r1 != api.ResultWon {
		t.Fatalf("player 1 result = %d (last standing), want Won(%d)", int(r1), int(api.ResultWon))
	}
	t.Logf("FSV #200 destruction: both Playing with halls; after p2 hall killed → p2 Lost, p1 Won (Go sim result store)")

	// Latch: further ticks don't flip the terminal results.
	g.Advance(40)
	if g.Player(1).Result() != api.ResultWon || g.Player(2).Result() != api.ResultLost {
		t.Fatal("terminal results not latched after resolution")
	}
	t.Logf("FSV #200 destruction: results latched over 2 more seconds")
}
