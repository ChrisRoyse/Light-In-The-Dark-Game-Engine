package litd

// FSV for the public DefineUnits setup verb (#387). SoT = whether a unit of a
// defined code can actually be created and resolves Valid() afterwards (the
// data was really installed), plus the fail-closed edges (empty table, and a
// conflicting rebind both error).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestDefineUnitsFSV(t *testing.T) {
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}

	// Before: the code is unknown, so it cannot spawn.
	if !g.UnitType("hfoo").IsZero() {
		t.Fatal("UnitType(hfoo) should be null before DefineUnits")
	}

	// Define -> the code resolves and a unit spawns + validates.
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	typ := g.UnitType("hfoo")
	if typ.IsZero() {
		t.Fatal("UnitType(hfoo) still null after DefineUnits")
	}
	u := g.CreateUnit(Player{idx: 1, g: g}, typ, Vec2{X: 32, Y: 32}, Deg(0))
	if !u.Valid() {
		t.Fatal("unit created from a DefineUnits code is invalid")
	}
	t.Logf("FSV: DefineUnits installed hfoo; spawned unit Valid=%v", u.Valid())

	// Edge: empty table fails closed.
	if err := g.DefineUnits(nil); err == nil {
		t.Fatal("DefineUnits(nil) must error")
	} else {
		t.Logf("FSV edge empty: %v", err)
	}
	// Edge: conflicting rebind (different length) fails closed.
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, CollisionSize: 16},
		{ID: "hbar", Life: 50, CollisionSize: 16},
	}); err == nil {
		t.Fatal("conflicting rebind must error")
	} else {
		t.Logf("FSV edge conflict: %v", err)
	}
}
