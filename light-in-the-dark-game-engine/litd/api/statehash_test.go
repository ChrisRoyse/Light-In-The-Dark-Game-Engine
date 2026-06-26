package litd

// FSV for Game.StateHash (#267/#271). SoT = the actual authoritative sim state:
// two games built identically must hash equal; any real mutation must change the
// digest; and reverting the mutation must restore the original digest (the hash
// is a pure function of state, not of history).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestStateHashFSV(t *testing.T) {
	build := func(seed int64) *Game {
		g, err := NewGame(GameOptions{MaxUnits: 16, Seed: seed})
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

	// Identical construction -> identical digest.
	ga, gb := build(7), build(7)
	ha, hb := ga.StateHash(), gb.StateHash()
	t.Logf("FSV identical: ga=%#x gb=%#x", ha, hb)
	if ha != hb {
		t.Fatalf("identical games hash differently: %#x != %#x", ha, hb)
	}

	// A real mutation on one game diverges the digest.
	u := ga.CreateUnit(ga.Player(1), ga.UnitType("hfoo"), Vec2{X: 50, Y: 50}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit invalid")
	}
	hMut := ga.StateHash()
	t.Logf("FSV after spawn: ga=%#x (was %#x), gb still %#x", hMut, ha, gb.StateHash())
	if hMut == ha {
		t.Fatal("spawning a unit did not change StateHash")
	}
	if hMut == gb.StateHash() {
		t.Fatal("mutated game still hashes equal to the untouched twin")
	}

	// Reproducing the SAME mutation on the twin reconverges the digest (the hash
	// is a function of state, so equal states hash equal regardless of when).
	u2 := gb.CreateUnit(gb.Player(1), gb.UnitType("hfoo"), Vec2{X: 50, Y: 50}, Deg(0))
	if !u2.Valid() {
		t.Fatal("twin CreateUnit invalid")
	}
	t.Logf("FSV reconverge: ga=%#x gb=%#x", ga.StateHash(), gb.StateHash())
	if ga.StateHash() != gb.StateHash() {
		t.Fatalf("twins with identical mutations diverge: %#x != %#x", ga.StateHash(), gb.StateHash())
	}

	// A nil/zero game hashes to 0 (fail-closed sentinel).
	if (*Game)(nil).StateHash() != 0 {
		t.Fatal("nil game must hash to 0")
	}
}
