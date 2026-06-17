package luabind

// #268 runtime world-loader FSV. SoT = the live sim state the world's Lua
// produced (Game.TimeOfDay, unit positions, Game.StateHash) and the loud error
// text on a broken world — never a trusted return code. The loader runs world
// chunks through the SAME bound binding surface a shipped binary would.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func loaderGame(t *testing.T, seed int64) *api.Game {
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
	return g
}

// boundState returns an LState with the api surface bound to g and the world's
// data-table binding (`footman` UnitType) injected — the seam the asset layer
// will fill for a real world.
func boundState(t *testing.T, g *api.Game) *lua.LState {
	t.Helper()
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	ud := L.NewUserData()
	ud.Value = g.UnitType("hfoo")
	L.SetGlobal("footman", ud)
	return L
}

func writeWorld(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	for name, src := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(src), 0o644); err != nil {
			t.Fatalf("write %s: %v", name, err)
		}
	}
	return dir
}

// TestLoadWorldRunsEntryFSV loads the committed dev-sandbox world and verifies
// its Lua actually ran against the live game (clock set, unit spawned at known
// coords) — SoT is the sim state, read via the Go api.
func TestLoadWorldRunsEntryFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	reg := NewChunkRegistry()
	defer reg.Close()

	info, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "dev-sandbox"))
	if err != nil {
		t.Fatalf("LoadWorld: %v", err)
	}
	units := g.UnitsInRange(api.Vec2{X: 320, Y: 256}, 8, nil)
	if len(units) != 1 {
		t.Fatalf("units near spawn = %d, want 1 (world script did not create the unit)", len(units))
	}
	pos := units[0].Position()
	t.Logf("FSV world ran: tod=%v unit@(%v,%v) entry=%s chunks=%d", g.TimeOfDay(), pos.X, pos.Y, info.Entry, len(info.Chunks))
	if g.TimeOfDay() != 12.0 {
		t.Fatalf("TimeOfDay = %v, want 12.0 (world script did not set the clock)", g.TimeOfDay())
	}
	if pos.X != 320 || pos.Y != 256 {
		t.Fatalf("unit position = (%v,%v), want (320,256)", pos.X, pos.Y)
	}
}

// TestLoadWorldEditReloadNoRebuildFSV proves the headline #268 property: edit
// the world .lua and reload — the change takes effect with NO engine rebuild.
func TestLoadWorldEditReloadNoRebuildFSV(t *testing.T) {
	dir := writeWorld(t, map[string]string{"main.lua": "Game_SetTimeOfDay(12.0)\n"})
	mainPath := filepath.Join(dir, "main.lua")

	load := func(seed int64) float64 {
		g := loaderGame(t, seed)
		L := boundState(t, g)
		defer L.Close()
		reg := NewChunkRegistry()
		defer reg.Close()
		if _, err := LoadWorld(L, reg, dir); err != nil {
			t.Fatalf("LoadWorld: %v", err)
		}
		return g.TimeOfDay()
	}

	if tod := load(1); tod != 12.0 {
		t.Fatalf("first load TimeOfDay = %v, want 12.0", tod)
	}
	// Edit the world source — no recompile of the engine binary.
	if err := os.WriteFile(mainPath, []byte("Game_SetTimeOfDay(6.0)\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	tod := load(1)
	t.Logf("FSV edit-reload (no rebuild): load1 tod=12.0 -> edit main.lua -> load2 tod=%v", tod)
	if tod != 6.0 {
		t.Fatalf("post-edit load TimeOfDay = %v, want 6.0 (reload did not pick up the edit)", tod)
	}
}

// TestLoadWorldSyntaxErrorFailsLoudFSV: a world that will not compile fails at
// load with a file-named error, and leaves the game untouched (fail-closed).
func TestLoadWorldSyntaxErrorFailsLoudFSV(t *testing.T) {
	dir := writeWorld(t, map[string]string{"main.lua": "this is ) not lua\n"})
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	reg := NewChunkRegistry()
	defer reg.Close()

	before := g.StateHash()
	_, err := LoadWorld(L, reg, dir)
	if err == nil {
		t.Fatal("a world with a syntax error must fail loudly at load, got nil")
	}
	t.Logf("FSV syntax error: %v", err)
	if !strings.Contains(err.Error(), "main.lua") {
		t.Errorf("load error must name the offending chunk: %v", err)
	}
	if g.StateHash() != before {
		t.Fatalf("a broken world mutated sim state: %#x -> %#x", before, g.StateHash())
	}
}

// TestLoadWorldMissingEntryFailsLoudFSV: a directory with .lua files but no
// main.lua is a loud load error, not a silent no-op.
func TestLoadWorldMissingEntryFailsLoudFSV(t *testing.T) {
	dir := writeWorld(t, map[string]string{"helper.lua": "local x = 1\n"})
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	reg := NewChunkRegistry()
	defer reg.Close()

	_, err := LoadWorld(L, reg, dir)
	if err == nil || !strings.Contains(err.Error(), "no "+WorldEntry) {
		t.Fatalf("missing entry must fail loudly naming %s, got: %v", WorldEntry, err)
	}
	t.Logf("FSV missing entry: %v", err)
}

// TestLoadWorldDeterministicFSV: two loads of the same world produce identical
// chunk-ids AND identical sim state hashes — the property the persister relies
// on (#264/#270).
func TestLoadWorldDeterministicFSV(t *testing.T) {
	dir := writeWorld(t, map[string]string{
		"main.lua": "Game_SetTimeOfDay(9.0)\nGame_CreateUnit(Game_Player(0), footman, { x = 100, y = 100 }, 0)\n",
	})
	load := func() (string, uint64) {
		g := loaderGame(t, 7)
		L := boundState(t, g)
		defer L.Close()
		reg := NewChunkRegistry()
		defer reg.Close()
		info, err := LoadWorld(L, reg, dir)
		if err != nil {
			t.Fatalf("LoadWorld: %v", err)
		}
		return info.Chunks[0].ID, g.StateHash()
	}
	id1, h1 := load()
	id2, h2 := load()
	t.Logf("FSV determinism: chunkID=%s stateHash=%#x (both loads)", id1, h1)
	if id1 != id2 {
		t.Fatalf("chunk-id not deterministic: %s != %s", id1, id2)
	}
	if h1 != h2 {
		t.Fatalf("post-load state hash not deterministic: %#x != %#x", h1, h2)
	}
}

// TestLoadWorldQuotaBreachNamedFSV: a world whose entry breaches the instruction
// budget fails loudly at load, naming the world — never a silent hang.
func TestLoadWorldQuotaBreachNamedFSV(t *testing.T) {
	dir := writeWorld(t, map[string]string{"main.lua": "while true do end\n"})
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	L.SetInstructionBudget(200000) // armed: the infinite loop must trip it
	reg := NewChunkRegistry()
	defer reg.Close()

	_, err := LoadWorld(L, reg, dir)
	if err == nil {
		t.Fatal("an infinite-loop world must trip the instruction budget, got nil")
	}
	t.Logf("FSV quota breach: %v", err)
	if !strings.Contains(err.Error(), dir) && !strings.Contains(err.Error(), WorldEntry) {
		t.Errorf("quota error should name the world/entry: %v", err)
	}
}
