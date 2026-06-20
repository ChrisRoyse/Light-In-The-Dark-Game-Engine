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
		lit, _ = g.Storage().GetInt("beacon1", "state")
		owner, _ = g.Storage().GetInt("beacon1", "owner")
		progress, _ = g.Storage().GetInt("beacon1", "progress")
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

// TestBeaconRecaptureRequiresFullDurationFSV (#169 edge): a freshly-arrived
// challenger must NOT instantly steal a fully-captured beacon. Bug: on capture
// the loop clamps progress to CAPTURE_TICKS, so the next sole contender accrues
// CAPTURE_TICKS+TICK_PROGRESS >= CAPTURE_TICKS and flips ownership in ONE 0.25s
// scan — a 2s capture stolen in one tick. Re-capture must take the full duration;
// and the prior owner's persistent fog modifier must move, not leak.
// SoT = published beacon owner/lit in Storage + the sim fog grid.
func TestBeaconRecaptureRequiresFullDurationFSV(t *testing.T) {
	beacon := api.Vec2{X: 1000, Y: 1000}
	worldDir := filepath.Join("..", "..", "worlds", "beacon-capture")

	g := loaderGame(t, 5)
	L := boundState(t, g)
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })

	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), beacon, api.Deg(0))
	if !u1.Valid() {
		t.Fatal("p1 capturing unit invalid")
	}
	if _, err := LoadWorld(L, reg, worldDir); err != nil {
		t.Fatalf("LoadWorld(beacon-capture): %v", err)
	}
	owner := func() int { o, _ := g.Storage().GetInt("beacon1", "owner"); return o }

	// p1 captures over the full duration.
	g.Advance(80)
	if owner() != 1 {
		t.Fatalf("precondition: p1 should own after 4s, got owner=%d", owner())
	}

	// p1 leaves the radius; p2 becomes the sole contender.
	u1.Kill()
	if !g.CreateUnit(g.Player(2), g.UnitType("hfoo"), beacon, api.Deg(0)).Valid() {
		t.Fatal("p2 challenger unit invalid")
	}

	// ONE capture scan (0.25s = 5 ticks): must not flip ownership.
	g.Advance(5)
	if owner() == 2 {
		lit, _ := g.Storage().GetInt("beacon1", "state")
		prog, _ := g.Storage().GetInt("beacon1", "progress")
		t.Fatalf("INSTANT RE-CAPTURE BUG: p2 stole the beacon in one 0.25s scan (owner=2 lit=%d progress=%d) — progress was clamped at CAPTURE_TICKS so it flipped immediately; re-capture must require the full 2s", lit, prog)
	}

	// After the full capture duration (8 scans = 40 ticks) p2 legitimately captures.
	g.Advance(40)
	if owner() != 2 {
		t.Fatalf("p2 never captured after the full duration: owner=%d", owner())
	}
	// The prior owner's vision must move, not leak: p1 (no units, did not re-capture)
	// must no longer see the beacon through a stale fog modifier.
	if fs := g.FogStateAt(g.Player(1), beacon); fs == api.FogVisible {
		t.Fatalf("FOG LEAK: p1 still sees the re-captured beacon (FogStateAt=Visible) — the old owner's fog modifier was never stopped on ownership transfer")
	}
	t.Logf("FSV #169 re-capture: a new challenger needs the full 2s to flip a captured beacon (not one scan); prior owner's vision moved, not leaked")
}
