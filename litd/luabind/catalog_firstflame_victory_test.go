package luabind

// Headless FSV of beacon-control victory (#200 path a) in the canonical First
// Flame world. SoT = the sim's terminal PlayerResult (Go api, resolved in the
// deterministic phase) + the hold-timer/flow state the world publishes to
// Storage. The screenshot-free state-JSON SoT is exactly what #200 specifies;
// the destruction-victory path (b) and the save/load edge (gated on #204/#270)
// are tracked separately.

import (
	"os"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// runVictory loads the firstflame world with P1 (slot 1) units on the given
// beacon cells, advances `ticks`, and returns (winner-slot, decided, P1 result).
func runVictory(t *testing.T, holdCells [][2]int, ticks int) (winner, decided int, p1Result api.MatchResult) {
	t.Helper()
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	defer reg.Close()

	for _, c := range holdCells {
		pos := api.Vec2{X: float64(c[0]*32 + 16), Y: float64(c[1]*32 + 16)}
		if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), pos, api.Deg(0)).Valid() {
			t.Fatalf("unit at cell %v invalid", c)
		}
	}
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		t.Fatalf("LoadWorld: %v", err)
	}
	g.Advance(ticks)

	winner, _ = g.Storage().GetInt("match", "winner")
	decided, _ = g.Storage().GetInt("match", "decided")
	return winner, decided, g.Player(1).Result()
}

func TestFirstFlameVictoryFSV(t *testing.T) {
	// Beacon cells: id1 (128,128), id2 (88,88). Holding both ≥ HOLD_THRESHOLD(2)
	// for HOLD_STEPS(12) after ~8 capture steps → P1 (slot 1) wins. 20 steps =
	// 100 ticks; advance 130 for margin.
	bothBeacons := [][2]int{{128, 128}, {88, 88}}

	// Scenario 1 — victory: SoT = sim PlayerResult latches Won for P1.
	winner, decided, res := runVictory(t, bothBeacons, 130)
	if decided != 1 || winner != 1 {
		t.Fatalf("expected P1 (slot 1) beacon victory: decided=%d winner=%d", decided, winner)
	}
	if res != api.ResultWon {
		t.Fatalf("sim PlayerResult(P1) = %d, want ResultWon(%d)", int(res), int(api.ResultWon))
	}
	t.Logf("FSV #200 victory: P1 held 2 beacons → decided, winner=slot1, sim PlayerResult=Won")

	// Edge — below threshold: holding ONE beacon never reaches the threshold, so
	// no victory ever latches (hold timer stays 0, result stays Playing).
	winner2, decided2, res2 := runVictory(t, [][2]int{{128, 128}}, 200)
	if decided2 != 0 || winner2 != -1 || res2 != api.ResultPlaying {
		t.Fatalf("single-beacon hold must NOT win: decided=%d winner=%d result=%d", decided2, winner2, int(res2))
	}
	t.Logf("FSV #200 below-threshold: holding 1 of 2 required beacons for 200 ticks → no victory (result=Playing)")

	// Edge — determinism: the victory outcome is identical across identical runs.
	w3, d3, r3 := runVictory(t, bothBeacons, 130)
	if w3 != winner || d3 != decided || r3 != res {
		t.Fatalf("non-deterministic victory: run1(w=%d d=%d r=%d) run2(w=%d d=%d r=%d)", winner, decided, int(res), w3, d3, int(r3))
	}
	t.Logf("FSV #200 determinism: double-run identical (winner=slot1 decided=1 result=Won)")
}
