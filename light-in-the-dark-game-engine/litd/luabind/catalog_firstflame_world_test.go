package luabind

// Headless FSV of the canonical First Flame beacon world (#169 — the state-JSON
// SoT; the screenshot SoT is render/asset-gated). Exercises three #169 edge
// cases against the REAL map's three beacons: simultaneous capture of two
// beacons in the same tick window, a contested beacon staying frozen, and
// determinism (run twice → identical published state). SoT = the per-beacon
// owner/progress/state the world publishes to Storage, cross-checked against the
// Go sim.

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

type beaconState struct{ id, owner, progress, state int }

// runFirstFlame loads the map + the canonical world, places units, advances, and
// returns the published state of all three beacons (keyed by storage index).
func runFirstFlame(t *testing.T) map[int]beaconState {
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
	// Make players 1 and 2 mutual enemies so the shared beacon is contested.
	if g.Player(1).IsAlly(g.Player(2)) {
		g.Player(1).SetAlliance(g.Player(2), 0)
		g.Player(2).SetAlliance(g.Player(1), 0)
	}

	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	defer reg.Close()

	// Beacon world coords (cell*32+16): id1 (4112,4112), id2 (2832,2832),
	// id3 (5392,5392). Place P1 alone on id1 & id2 (→ both capture), and P1+P2
	// together on id3 (→ contested, frozen).
	at := func(cx, cy int) api.Vec2 { return api.Vec2{X: float64(cx*32 + 16), Y: float64(cy*32 + 16)} }
	mk := func(p, cx, cy int) {
		if !g.CreateUnit(g.Player(p), g.UnitType("hfoo"), at(cx, cy), api.Deg(0)).Valid() {
			t.Fatalf("unit P%d at (%d,%d) invalid", p, cx, cy)
		}
	}
	mk(1, 128, 128) // id1 central
	mk(1, 88, 88)   // id2 flank
	mk(1, 168, 168) // id3 — P1
	mk(2, 168, 168) // id3 — P2 (enemy) → contested

	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		L.Close()
		t.Fatalf("LoadWorld(firstflame): %v", err)
	}
	g.Advance(60) // > CAPTURE_STEPS*5 ticks
	defer L.Close()

	out := map[int]beaconState{}
	for i := 1; i <= 3; i++ {
		key := "beacon" + itoa(i)
		id, _ := g.Storage().GetInt(key, "id")
		owner, _ := g.Storage().GetInt(key, "owner")
		prog, _ := g.Storage().GetInt(key, "progress")
		st, _ := g.Storage().GetInt(key, "state")
		out[i] = beaconState{id: id, owner: owner, progress: prog, state: st}
	}
	return out
}

func TestFirstFlameBeaconWorldFSV(t *testing.T) {
	got := runFirstFlame(t)

	// Find the storage slots for ids 1,2,3 (id-sorted, so slot i == id i).
	b1, b2, b3 := got[1], got[2], got[3]
	if b1.id != 1 || b2.id != 2 || b3.id != 3 {
		t.Fatalf("beacon id order unexpected: %+v", got)
	}

	// Edge — simultaneous capture: id1 and id2 both lit for player 1.
	if b1.owner != 1 || b1.state != 1 {
		t.Fatalf("beacon1 not captured by P1: %+v", b1)
	}
	if b2.owner != 1 || b2.state != 1 {
		t.Fatalf("beacon2 not captured by P1: %+v", b2)
	}
	t.Logf("FSV #169 simultaneous: beacon1 + beacon2 both lit owner=1 in the same run")

	// Edge — contested: id3 has P1 and P2 enemies in radius → frozen neutral.
	if b3.owner != -1 || b3.state != 0 || b3.progress != 0 {
		t.Fatalf("contested beacon3 not frozen-neutral: %+v (want owner=-1 state=0 progress=0)", b3)
	}
	t.Logf("FSV #169 contested: beacon3 (P1 vs P2 in radius) frozen — owner=-1 progress=0 over 60 ticks")

	// Edge — determinism: a second identical run yields identical published state.
	again := runFirstFlame(t)
	for i := 1; i <= 3; i++ {
		if got[i] != again[i] {
			t.Fatalf("non-deterministic beacon %d: run1=%+v run2=%+v", i, got[i], again[i])
		}
	}
	t.Logf("FSV #169 determinism: double-run beacon state identical for all 3 beacons %v", got)

	// Vision: the captured central beacon reveals its radius for player 1.
	// (Re-load a fresh run to read the sim fog at the central beacon.)
	if fs := freshFogAtCentral(t); fs != api.FogVisible {
		t.Fatalf("captured central beacon did not reveal: FogStateAt=%d", int(fs))
	}
	t.Logf("FSV #169 vision: captured central beacon stamps FogVisible for P1")
}

// freshFogAtCentral reproduces a run and reads the sim fog at the central beacon.
func freshFogAtCentral(t *testing.T) api.FogState {
	t.Helper()
	m, _ := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	g, _ := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	g.DefineUnits([]data.Unit{{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16}})
	L := lua.NewState()
	defer L.Close()
	Register(L, g)
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	defer reg.Close()
	central := api.Vec2{X: 128*32 + 16, Y: 128*32 + 16}
	g.CreateUnit(g.Player(1), g.UnitType("hfoo"), central, api.Deg(0))
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		t.Fatalf("LoadWorld: %v", err)
	}
	g.Advance(60)
	return g.FogStateAt(g.Player(1), central)
}

// TestFirstFlameBeaconRecaptureRequiresFullDurationFSV (#169/#410 edge on the
// canonical shipping world): a fresh enemy challenger must NOT instantly steal an
// owned beacon. Bug: light() set progress = CAPTURE_STEPS on capture, so the next
// non-owner claimant's first +ACCRUE step reached CAPTURE_STEPS+1 >= CAPTURE_STEPS
// and flipped ownership in a single 0.25s scan; and the prior owner's fog modifier
// was never stopped on transfer (vision leak). SoT = published beacon owner +
// the sim fog grid (Game.FogStateAt).
func TestFirstFlameBeaconRecaptureRequiresFullDurationFSV(t *testing.T) {
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
	if g.Player(1).IsAlly(g.Player(2)) { // P1 and P2 mutual enemies (a contest, then a steal attempt)
		g.Player(1).SetAlliance(g.Player(2), 0)
		g.Player(2).SetAlliance(g.Player(1), 0)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })

	central := api.Vec2{X: 128*32 + 16, Y: 128*32 + 16} // map beacon id1 → "beacon1"
	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), central, api.Deg(0))
	if !u1.Valid() {
		t.Fatal("p1 capturing unit invalid")
	}
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		t.Fatalf("LoadWorld(firstflame): %v", err)
	}
	owner := func() int { o, _ := g.Storage().GetInt("beacon1", "owner"); return o }

	g.Advance(60) // P1 captures the central beacon over the full duration
	if owner() != 1 {
		t.Fatalf("precondition: P1 should own the central beacon, got owner=%d", owner())
	}

	u1.Kill() // P1 leaves; P2 becomes the sole contender
	if !g.CreateUnit(g.Player(2), g.UnitType("hfoo"), central, api.Deg(0)).Valid() {
		t.Fatal("p2 challenger unit invalid")
	}

	g.Advance(5) // ONE capture step: ownership must NOT flip
	if owner() == 2 {
		prog, _ := g.Storage().GetInt("beacon1", "progress")
		t.Fatalf("INSTANT RE-CAPTURE BUG: P2 stole the central beacon in one step (owner=2 progress=%d) — light() clamped progress at CAPTURE_STEPS so the next non-owner claimant flips on its first +ACCRUE step; re-capture must take the full duration", prog)
	}

	g.Advance(40) // full capture duration → P2 legitimately captures
	if owner() != 2 {
		t.Fatalf("P2 never captured after the full duration: owner=%d", owner())
	}
	if fs := g.FogStateAt(g.Player(1), central); fs == api.FogVisible {
		t.Fatalf("FOG LEAK: P1 still sees the re-captured central beacon (FogStateAt=Visible) — light() never stopped the prior owner's fog modifier on transfer")
	}
	t.Logf("FSV #169/#410 re-capture: a fresh challenger needs the full duration to flip an owned beacon; prior owner's vision moved, not leaked")
}

// TestFirstFlameBeaconProgressIsPerChallengerFSV (#169 edge): capture progress
// belongs to the player accruing it — a rival must not INHERIT a challenger's
// partial charge via a contest hand-off. Bug: `progress` was a single shared
// per-beacon var, so P1 charging a neutral beacon to 7/8, P2 contesting (freeze),
// then P1 leaving handed P2 a 7-step lead — P2 captured on its first solo step
// (1 step of work stealing 7). Reachable with the two shipping competitors. SoT =
// beacon1 owner/progress in Storage.
func TestFirstFlameBeaconProgressIsPerChallengerFSV(t *testing.T) {
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
	if g.Player(1).IsAlly(g.Player(2)) {
		g.Player(1).SetAlliance(g.Player(2), 0)
		g.Player(2).SetAlliance(g.Player(1), 0)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })

	central := api.Vec2{X: 128*32 + 16, Y: 128*32 + 16} // map beacon id1 → "beacon1"
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		t.Fatalf("LoadWorld(firstflame): %v", err)
	}
	owner := func() int { o, _ := g.Storage().GetInt("beacon1", "owner"); return o }
	prog := func() int { p, _ := g.Storage().GetInt("beacon1", "progress"); return p }

	// P1 charges the neutral central beacon alone: 7 steps (35 ticks), one short of
	// the 8-step capture.
	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), central, api.Deg(0))
	if !u1.Valid() {
		t.Fatal("p1 unit invalid")
	}
	g.Advance(35)
	if owner() != -1 || prog() != 7 {
		t.Fatalf("precondition: want neutral beacon at progress 7 after P1's 7 steps, got owner=%d progress=%d", owner(), prog())
	}

	// P2 contests for one step → capture freezes (progress held at 7).
	u2 := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), central, api.Deg(0))
	if !u2.Valid() {
		t.Fatal("p2 unit invalid")
	}
	g.Advance(5)
	if prog() != 7 {
		t.Fatalf("contested step should FREEZE progress at 7, got %d", prog())
	}

	// P1 leaves; P2 is the sole contender for ONE step. P2 must NOT inherit P1's 7.
	u1.Kill()
	g.Advance(5)
	if owner() == 2 {
		t.Fatalf("PROGRESS-THEFT BUG: P2 captured on its first solo step (owner=2, progress=%d) by inheriting P1's 7-step charge through the contest hand-off — capture progress must be per-challenger", prog())
	}
	if prog() != 1 {
		t.Fatalf("after the hand-off P2 should accrue its OWN progress from zero (want 1), got %d", prog())
	}
	t.Logf("FSV #169 per-challenger: a contest hand-off does not let a rival inherit accrued progress; P2 restarts from zero (progress=%d, owner=%d)", prog(), owner())
}
