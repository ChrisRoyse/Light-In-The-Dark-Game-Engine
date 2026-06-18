package luabind

// Beacon-capture integration FSV (#169 mechanic, dogfooding #267): loads the
// worlds/beacon-capture world — which is written purely against the bound Lua
// surface (Game_Every, Game_UnitsInRange, Unit_Owner, Player_Slot/IsEnemy,
// Game_NewFogModifier, Storage) — and drives it headlessly. SoT = the beacon
// state the world publishes to Storage + the sim fog grid (Game.FogStateAt),
// read via the Go api. No VFX/map needed: the issue's primary SoT is headless
// state, screenshots are secondary.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func TestBeaconCaptureWorldFSV(t *testing.T) {
	beacon := api.Vec2{X: 1000, Y: 1000}
	worldDir := filepath.Join("..", "..", "worlds", "beacon-capture")

	// load builds a game, runs setup (unit placement / alliances) against it, then
	// loads the beacon world (registering its Game_Every capture loop). The LState
	// must outlive Advance (the timer callback runs on it), so it is closed via
	// Cleanup, not defer.
	load := func(setup func(g *api.Game)) *api.Game {
		g := loaderGame(t, 5)
		L := boundState(t, g)
		reg := NewChunkRegistry()
		t.Cleanup(func() { L.Close(); reg.Close() })
		setup(g)
		if _, err := LoadWorld(L, reg, worldDir); err != nil {
			t.Fatalf("LoadWorld(beacon-capture): %v", err)
		}
		return g
	}
	beaconState := func(g *api.Game) (lit, owner, progress int) {
		lit, _ = g.Storage().GetInt("beacon", "lit")
		owner, _ = g.Storage().GetInt("beacon", "owner")
		progress, _ = g.Storage().GetInt("beacon", "progress")
		return
	}

	// --- Happy path: one player's unit in radius captures the beacon. ---
	g := load(func(g *api.Game) {
		if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), beacon, api.Deg(0)).Valid() {
			t.Fatal("capturing unit invalid")
		}
	})
	// Before capture completes (1s < 2s threshold): not yet lit (teeth).
	g.Advance(20)
	if lit, _, prog := beaconState(g); lit != 0 {
		t.Fatalf("beacon lit too early at 1s: lit=%d progress=%d", lit, prog)
	}
	// Past the 2s threshold: lit for player 1, vision stamped.
	g.Advance(60)
	lit, owner, prog := beaconState(g)
	if lit != 1 || owner != 1 {
		t.Fatalf("beacon not captured: lit=%d owner=%d progress=%d (want lit=1 owner=1)", lit, owner, prog)
	}
	if fs := g.FogStateAt(g.Player(1), beacon); fs != api.FogVisible {
		t.Fatalf("captured beacon did not reveal its radius: FogStateAt=%d, want Visible(%d)", int(fs), int(api.FogVisible))
	}
	t.Logf("FSV #169 capture: not lit at 1s; lit=1 owner=1 progress=%d at 4s; owner vision stamped (FogVisible)", prog)

	// --- Contest: an enemy unit in radius freezes progress (no capture). ---
	g2 := load(func(g *api.Game) {
		g.Player(1).SetTeam(0)
		g.Player(2).SetTeam(1)
		if !g.Player(1).IsEnemy(g.Player(2)) {
			t.Skip("players 1 and 2 are not enemies under default alliances; contest case needs hostile setup")
		}
		if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), beacon, api.Deg(0)).Valid() ||
			!g.CreateUnit(g.Player(2), g.UnitType("hfoo"), beacon, api.Deg(0)).Valid() {
			t.Fatal("contest units invalid")
		}
	})
	g2.Advance(80) // well past the uncontested capture time
	if lit, owner, prog := beaconState(g2); lit != 0 {
		t.Fatalf("contested beacon was captured: lit=%d owner=%d progress=%d (want lit=0, frozen)", lit, owner, prog)
	}
	t.Logf("FSV #169 contest: enemy in radius froze capture — beacon stayed neutral over 4s")
}
