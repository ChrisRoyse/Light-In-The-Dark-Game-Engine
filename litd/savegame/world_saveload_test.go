package savegame

// FSV for #481: every prototype world script (worlds/*/main.lua driven by the
// catalog harness) round-trips a mid-game save through the production savegame
// container. The enabling fixes: the world loader runs the entry+siblings via
// REGISTERED prototypes (so closures persist), the marshaling seam covers the
// Player/UnitType/BuffType/FogModifier handles these scripts capture as Game_Every
// upvalues, the `require` shim is excluded from the saved globals, and
// RestoreEventHandlers is re-run-idempotent.
//
// SoT = Game.StateHash() (unbroken H1 vs save@N→reload→finish H2) plus the
// script's published Storage state. The load side RE-RUNS the world (LoadWorld),
// which the savegame container requires to rebuild the Game_Every periodic slots,
// then restores sim+scripts over it.

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

const wfp = uint64(0x481B0B)

// worldSetup builds a fresh game + LState + registry with the world's types
// bound, units placed, and any map registered — everything EXCEPT running the
// world chunk (the driver runs it via LoadWorld). Deterministic: identical on
// every call so entity ids / hashes line up across the three runs.
type worldSetup func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry)

// sotKey is a Storage (category,key) probe printed as evidence.
type sotKey struct{ cat, key string }

func newWorldGame(t *testing.T, seed int64, units []data.Unit, buffs []data.BuffType) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 64, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if len(units) > 0 {
		if err := g.DefineUnits(units); err != nil {
			t.Fatalf("DefineUnits: %v", err)
		}
	}
	if len(buffs) > 0 {
		if err := g.DefineBuffTypes(buffs); err != nil {
			t.Fatalf("DefineBuffTypes: %v", err)
		}
	}
	L := lua.NewState()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L, luabind.NewChunkRegistry()
}

func hfoo() []data.Unit {
	return []data.Unit{{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16}}
}

// roundTrip runs the #481 round-trip for one world and asserts H1==H2 + SoT parity.
func roundTrip(t *testing.T, name, dir string, setup worldSetup, saveAt, finish int, probes []sotKey) {
	t.Helper()
	worldDir := filepath.Join("..", "..", "worlds", dir)
	readSoT := func(g *api.Game) map[sotKey]int {
		m := map[sotKey]int{}
		for _, p := range probes {
			v, _ := g.Storage().GetInt(p.cat, p.key)
			m[p] = v
		}
		return m
	}

	// reference: unbroken to finish.
	gr, Lr, regr := setup(t)
	if _, err := luabind.LoadWorld(Lr, regr, worldDir); err != nil {
		t.Fatalf("%s ref LoadWorld: %v", name, err)
	}
	gr.Advance(finish)
	refHash := gr.StateHash()
	refSoT := readSoT(gr)
	Lr.Close()
	regr.Close()

	// save @ saveAt.
	gs, Ls, regs := setup(t)
	if _, err := luabind.LoadWorld(Ls, regs, worldDir); err != nil {
		t.Fatalf("%s save LoadWorld: %v", name, err)
	}
	gs.Advance(saveAt)
	var buf bytes.Buffer
	if err := Write(&buf, gs, Ls, regs, wfp); err != nil {
		t.Fatalf("%s Write: %v", name, err)
	}
	Ls.Close()
	regs.Close()

	// restore: fresh setup, RE-RUN the world (rebuilds periodic slots/triggers),
	// then load the container; finish and compare.
	gg, Lg, regg := setup(t)
	if _, err := luabind.LoadWorld(Lg, regg, worldDir); err != nil {
		t.Fatalf("%s restore LoadWorld: %v", name, err)
	}
	if err := Load(bytes.NewReader(buf.Bytes()), gg, Lg, regg, wfp); err != nil {
		t.Fatalf("%s Load: %v", name, err)
	}
	gg.Advance(finish - saveAt)
	gotHash := gg.StateHash()
	gotSoT := readSoT(gg)
	Lg.Close()
	regg.Close()

	t.Logf("#481 %-20s unbroken@%d=%#016x  save@%d→reload→@%d=%#016x  MATCH=%v",
		name, finish, refHash, saveAt, finish, gotHash, gotHash == refHash)
	for _, p := range probes {
		t.Logf("    SoT %s.%s: ref=%d got=%d", p.cat, p.key, refSoT[p], gotSoT[p])
	}
	if gotHash != refHash {
		t.Fatalf("%s: mid-game save/load not bit-identical: %#x != %#x", name, gotHash, refHash)
	}
	for _, p := range probes {
		if refSoT[p] != gotSoT[p] {
			t.Fatalf("%s SoT %s.%s diverged: ref=%d got=%d", name, p.cat, p.key, refSoT[p], gotSoT[p])
		}
	}
}

func TestWorldFlickerSaveLoadFSV(t *testing.T) {
	roundTrip(t, "flicker-cycle", "flicker-cycle", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5, hfoo(),
			[]data.BuffType{{ID: "dimpwr", DurationTicks: 50, Stacking: data.StackRefresh, MaxStacks: 1}})
		g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
		return g, L, reg
	}, 70, 160, []sotKey{{"flicker", "phase"}, {"flicker", "tick"}, {"flicker", "transitions"}})
}

func TestWorldTheFlameSaveLoadFSV(t *testing.T) {
	roundTrip(t, "the-flame", "the-flame", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5,
			[]data.Unit{
				{ID: "pyre_wagon", Life: 200, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
				{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
			},
			[]data.BuffType{{ID: "flame_aura", DurationTicks: 10, Stacking: data.StackRefresh, MaxStacks: 1}})
		g.CreateUnit(g.Player(0), g.UnitType("pyre_wagon"), api.Vec2{X: 500, Y: 500}, api.Deg(0))
		g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 550, Y: 500}, api.Deg(0)) // in flame radius
		return g, L, reg
	}, 20, 60, []sotKey{{"flame", "lit"}, {"flame", "wagons"}, {"flame", "x"}})
}

func TestWorldMatchFlowSaveLoadFSV(t *testing.T) {
	roundTrip(t, "match-flow", "match-flow", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5, nil, nil)
		return g, L, reg
	}, 20, 60, []sotKey{{"match", "state"}, {"match", "tick"}})
}

func TestWorldBeaconCaptureSaveLoadFSV(t *testing.T) {
	roundTrip(t, "beacon-capture", "beacon-capture", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5, hfoo(), nil)
		g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 1000, Y: 1000}, api.Deg(0)) // on the beacon
		return g, L, reg
	}, 20, 60, []sotKey{{"beacon1", "owner"}, {"beacon1", "progress"}, {"beacon1", "state"}})
}

func TestWorldVictoryDestructionSaveLoadFSV(t *testing.T) {
	roundTrip(t, "victory-destruction", "victory-destruction", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5,
			[]data.Unit{{ID: "hall", Life: 500, MoveSpeedPerTick: 0, TurnRatePerTick: 65535, CollisionSize: 32}}, nil)
		g.CreateUnit(g.Player(1), g.UnitType("hall"), api.Vec2{X: 300, Y: 300}, api.Deg(0))
		g.CreateUnit(g.Player(2), g.UnitType("hall"), api.Vec2{X: 900, Y: 900}, api.Deg(0))
		return g, L, reg
	}, 10, 40, []sotKey{{"match", "resolved"}})
}

func TestWorldTheDarkSaveLoadFSV(t *testing.T) {
	roundTrip(t, "the-dark", "the-dark", func(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
		g, L, reg := newWorldGame(t, 5,
			[]data.Unit{
				{ID: "gloam_whelp", Life: 40, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
				{ID: "gloam_stalker", Life: 80, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
				{ID: "gloam_horror", Life: 160, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
			}, nil)
		return g, L, reg
	}, 40, 130, []sotKey{{"dark", "waves"}, {"dark", "total"}, {"dark", "darkcount"}})
}

// firstFlameMapSetup builds a game with the real First Flame map registered, two
// enemy players, and one P1 unit on the central beacon (id1) so a capture +
// FogModifier happen — the firstflame scripts capture a FogModifier upvalue.
func firstFlameMapSetup(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
	t.Helper()
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("mapdata.Load: %v", err)
	}
	g, L, reg := newWorldGame(t, 5, hfoo(), nil)
	if g.Player(1).IsAlly(g.Player(2)) {
		g.Player(1).SetAlliance(g.Player(2), 0)
		g.Player(2).SetAlliance(g.Player(1), 0)
	}
	luabind.RegisterMap(L, m)
	g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 4112, Y: 4112}, api.Deg(0)) // beacon id1
	return g, L, reg
}

func TestWorldFirstFlameSaveLoadFSV(t *testing.T) {
	roundTrip(t, "firstflame", "firstflame", firstFlameMapSetup,
		30, 90, []sotKey{{"beacon1", "owner"}, {"beacon1", "state"}, {"match", "decided"}})
}

func TestWorldFirstFlameBeaconSaveLoadFSV(t *testing.T) {
	roundTrip(t, "firstflame-beacon", "firstflame-beacon", firstFlameMapSetup,
		30, 90, []sotKey{{"beacon1", "owner"}, {"beacon1", "progress"}, {"beacon1", "state"}})
}
