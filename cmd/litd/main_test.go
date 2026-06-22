package main

// #268 regression coverage for the shipped world-loader binary. The manual FSV
// (closing comment) runs ./bin/litd against worlds/dev-sandbox and reads the
// state JSON; these tests lock the same behavior against the sim SoT (unit
// state) and the loud-failure contract.

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

const (
	tomlDamageTable = "attack-types = [\"normal\"]\narmor-types = [\"unarmored\"]\n[coefficients]\nnormal = [1000]\n"
	tomlUnit        = "[[unit]]\nid = \"hfoo\"\nlife = 100\narmor-type = \"unarmored\"\nmove-speed = 270\nturn-rate = 0.6\ncollision-size = 16\npathing = \"ground\"\n"
)

// writeWorld lays out a world dir (data/combat + data/units + main.lua) under a
// temp root and returns its path. Pass empty damageTable to omit it.
func writeWorld(t *testing.T, damageTable, mainLua string) string {
	t.Helper()
	root := t.TempDir()
	if damageTable != "" {
		mk(t, filepath.Join(root, "data", "combat", "damage-table.toml"), damageTable)
	}
	mk(t, filepath.Join(root, "data", "units", "units.toml"), tomlUnit)
	mk(t, filepath.Join(root, "main.lua"), mainLua)
	return root
}

func mk(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// TestLoadWorldDevSandboxFSV: the shipped dev-sandbox loads and its Lua runs —
// SoT is the sim, which must hold exactly the unit the script spawned.
func TestLoadWorldDevSandboxFSV(t *testing.T) {
	g, cleanup, err := loadWorld(filepath.Join("..", "..", "worlds", "dev-sandbox"), 1, 50_000_000)
	if err != nil {
		t.Fatalf("dev-sandbox must load: %v", err)
	}
	defer cleanup()
	g.Advance(40)

	units := g.UnitsInRange(api.Vec2{X: 320, Y: 256}, 8, nil)
	t.Logf("FSV dev-sandbox: tod=%v unitsAtSpawn=%d", g.TimeOfDay(), len(units))
	if len(units) != 1 {
		t.Fatalf("want 1 unit at (320,256), got %d (script did not run)", len(units))
	}
	p := units[0].Position()
	if p.X != 320 || p.Y != 256 || units[0].Life() != 100 {
		t.Fatalf("unit state wrong: pos=(%v,%v) life=%v", p.X, p.Y, units[0].Life())
	}
	if g.TimeOfDay() < 12.0 || g.TimeOfDay() > 12.2 {
		t.Fatalf("tod=%v, want ~12.1 (script set 12.0 + 40 ticks)", g.TimeOfDay())
	}
}

// TestLoadWorldInstallsAbilities: a world shipping an abilities table now loads
// (#394 install seam), and the ability is genuinely installed — proven by
// granting it to a freshly created unit through the public api (SoT = the unit's
// ability handle), not by trusting the load's nil error.
func TestLoadWorldInstallsAbilities(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	mk(t, filepath.Join(w, "data", "abilities", "core.toml"), "[[ability]]\nid = \"defend\"\nname = \"Defend\"\n")
	g, cleanup, err := loadWorld(w, 1, 50_000_000)
	if err != nil {
		t.Fatalf("world with abilities must now load (#394): %v", err)
	}
	defer cleanup()
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 64}, api.Deg(0))
	a := u.AddAbility(api.AbilityRef(1))
	t.Logf("FSV install: world loaded; AbilityRef(1).Valid=%v", a.Valid())
	if !a.Valid() {
		t.Fatal("AbilityRef(1) not grantable — ability table was not installed")
	}
}

// (TestUninstallableTablesGate moved to litd/worldhost with the function it tests, #490.)

// TestLoadWorldInstallsHeroes: #396 — a world shipping a hero table now loads
// (DefineHeroes wired into loadWorld), and the heroes are genuinely installed —
// proven by the BindHeroes rebind-refusal guard (a second, otherwise-valid bind
// fails ONLY because the first was stored), not by trusting the load's nil error.
func TestLoadWorldInstallsHeroes(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	mk(t, filepath.Join(w, "data", "abilities", "core.toml"), "[[ability]]\nid = \"holy-light\"\nname = \"Holy Light\"\n")
	mk(t, filepath.Join(w, "data", "heroes", "heroes.toml"),
		"xp-curve = [0, 100]\n\n[[bounty]]\nunit = \"hfoo\"\nxp = 25\n\n"+
			"[[hero]]\nunit = \"hfoo\"\nstr = 10.0\nagi = 10.0\nint = 10.0\n\n"+
			"[[hero.skill]]\nability = \"holy-light\"\nmin-hero-level = [1, 2]\n\n"+
			"[revive]\nbase-seconds = 10.0\nseconds-per-level = 5.0\n") // no costs → no economy needed
	g, cleanup, err := loadWorld(w, 1, 50_000_000)
	if err != nil {
		t.Fatalf("world with heroes must now load (#396): %v", err)
	}
	defer cleanup()
	// SoT: a fresh, valid hero rule set (1 unit → 1 bounty, 2-level curve) would
	// bind on an unbound game; here it must be REFUSED, proving the world's heroes
	// already occupy the registry.
	if err := g.DefineHeroes(&data.HeroTables{Curve: []int64{0, 100}, Bounty: []int64{0}}); err == nil {
		t.Fatal("DefineHeroes after world install must fail (rebind refused) — heroes were not installed")
	}
	t.Logf("FSV #396: hero world loaded; rebind refused — heroes occupy the sim registry")
}

// TestLoadWorldInstallsResourceNodes: #401 — a world shipping an economy table
// with resource nodes now loads (DefineResourceNodes wired into loadWorld; was
// refused by the uninstallable gate), and the node type is genuinely usable —
// proven by resolving it and spawning a live node through the public api, not by
// trusting the load's nil error.
func TestLoadWorldInstallsResourceNodes(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	mk(t, filepath.Join(w, "data", "economy", "resources.toml"),
		"resource-types = [\"gold\", \"lumber\"]\n\n[[node]]\nid = \"goldmine\"\nresource = \"gold\"\namount = 500\n")
	g, cleanup, err := loadWorld(w, 1, 50_000_000)
	if err != nil {
		t.Fatalf("world shipping a resource-node table must now load (#401): %v", err)
	}
	defer cleanup()
	typ := g.ResourceNodeType("goldmine")
	if typ.IsZero() {
		t.Fatal("goldmine node type not installed after world load")
	}
	node := g.CreateResourceNode(typ, api.Vec2{X: 300, Y: 300})
	if !node.Valid() {
		t.Fatal("CreateResourceNode failed after world load — node table not usable")
	}
	t.Logf("FSV #401 world-load: node-shipping world loaded; goldmine resolved + spawned a live node")
}

// goldenDetLua is the committed 10k-tick state hash of worlds/determinism-lua
// (#271 G5.7). Re-derive from the TestLoadWorldDeterminismLuaFSV log after an
// intentional sim/Lua change, with justification (SimVersion discipline).
//
// Bumped 0xbf6367e3b9e444f4 → 0xb79ac7578742efdc (2026-06-20, #455): the ECA
// handler-identity registry appends a "handlers" system to HashSystems (ADR
// #451, R-SIM-6), shifting every World.HashState TopHash by a constant. Not a
// sim-outcome change — run1==run2 stays identical (deterministic).
//
// Bumped 0xb79ac7578742efdc → 0x53bbf47c6cd4c49c (2026-06-20, #456): the
// first-class ECA trigger slab adds a "triggers" system to HashSystems —
// another constant TopHash shift (empty slab here). run1==run2 unchanged.
//
// Bumped 0x53bbf47c6cd4c49c → 0x2a0ce0ec9ea1f157 (2026-06-20, #457): the
// boolexpr condition arena adds a "boolexpr" system to HashSystems — another
// constant TopHash shift (empty arena here). run1==run2 unchanged.
//
// Bumped 0x2a0ce0ec9ea1f157 → 0xc9cd34dbd50ecb55 (2026-06-20, #462): OnEvent
// is now sugar over a Trigger; the Go subscriptions this world registers now
// live in the hashed handler/trigger/boolexpr substrate instead of the
// non-hashed legacy subs table. Behavior unchanged (event-behavior suites
// green, dispatch order identical); only the carrier moved into the hash.
// run1==run2 stays identical (deterministic).
const goldenDetLua uint64 = 0xc9cd34dbd50ecb55

// TestLoadWorldPlacementSpawnsEntities — #403: a world ships a placement table
// (data/placement) and the loader spawns those entities after the type tables
// install and before main.lua runs. SoT = the sim holds exactly the placed
// units at their declared coordinates, with NO main.lua spawn calls.
func TestLoadWorldPlacementSpawnsEntities(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n") // main.lua spawns nothing
	mk(t, filepath.Join(w, "data", "economy", "resources.toml"),
		"resource-types = [\"gold\"]\n\n[[node]]\nid = \"goldmine\"\nresource = \"gold\"\namount = 500\n")
	mk(t, filepath.Join(w, "data", "placement", "place.toml"),
		"[[unit]]\ntype = \"hfoo\"\nowner = 1\nx = 100\ny = 200\nfacing = 0\n\n"+
			"[[unit]]\ntype = \"hfoo\"\nowner = 1\nx = 300\ny = 400\n\n"+
			"[[node]]\ntype = \"goldmine\"\nx = 500\ny = 600\n")
	g, cleanup, err := loadWorld(w, 1, 50_000_000)
	if err != nil {
		t.Fatalf("world shipping a placement table must load + spawn (#403): %v", err)
	}
	defer cleanup()

	// SoT: query the sim for the placed units (main.lua spawned none).
	units := g.UnitsInRange(api.Vec2{X: 200, Y: 300}, 1000, nil)
	if len(units) != 2 {
		t.Fatalf("placement spawned %d units, want exactly 2 (the two placed rows)", len(units))
	}
	got := map[string]bool{}
	for _, u := range units {
		if !u.Valid() {
			t.Fatal("placed unit is not Valid in sim")
		}
		p := u.Position()
		got[fmt.Sprintf("%.0f,%.0f", p.X, p.Y)] = true
	}
	for _, want := range []string{"100,200", "300,400"} {
		if !got[want] {
			t.Fatalf("no placed unit at (%s); positions found: %v", want, got)
		}
	}
	t.Logf("FSV #403 placement: 2 units spawned at their declared coords %v + 1 node (load succeeded => node spawned), no main.lua spawns", got)
}

// TestLoadWorldInputArchiveFSV — #134: the game command can load the exact
// production .litdworld archive format, not just source directories. SoT = the
// sim holds the placement row read from the verified archive FS, and the
// autotest screenshot path writes a PNG from that archive-backed game.
func TestLoadWorldInputArchiveFSV(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	mk(t, filepath.Join(w, "data", "placement", "place.toml"), "[[unit]]\ntype = \"hfoo\"\nowner = 1\nx = 300\ny = 400\nfacing = 0\n")
	archive := filepath.Join(t.TempDir(), "cmd-litd-archive-fsv.litdworld")
	err := worldpack.Pack(w, archive, ">=0.1.0 <0.2.0", worldpack.Hosting{
		Author:      "cmd/litd test",
		Title:       "Archive FSV",
		Description: "cmd/litd archive entrypoint",
		Players:     worldpack.Players{Min: 1, Max: 1, Suggested: 1},
		StartLocations: []worldpack.StartLocation{
			{Player: 1, Cell: [2]int{1, 1}},
		},
	}, nil)
	if err != nil {
		t.Fatal(err)
	}

	g, cleanup, err := loadWorldInput("", archive, 1, 50_000_000)
	if err != nil {
		t.Fatalf("archive load: %v", err)
	}
	defer cleanup()
	units := g.UnitsInRange(api.Vec2{X: 300, Y: 400}, 1, nil)
	if len(units) != 1 {
		t.Fatalf("archive-backed load spawned %d units at 300,400, want 1", len(units))
	}
	p := units[0].Position()
	if p.X != 300 || p.Y != 400 {
		t.Fatalf("archive-backed unit position=(%v,%v), want 300,400", p.X, p.Y)
	}

	shot := filepath.Join(t.TempDir(), "archive-autotest.png")
	if err := run("", archive, true, true, 5, 1, 50_000_000, shot); err != nil {
		t.Fatalf("archive autotest run: %v", err)
	}
	st, err := os.Stat(shot)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV #134 archive entrypoint: verified archive=%s unit=(%.0f,%.0f) shot=%s bytes=%d", archive, p.X, p.Y, shot, st.Size())
}

// TestLoadWorldInstallsCombatMatrix — #406: the world's required damage table
// (data/combat, parsed to tables.Coeff) is now installed by loadWorld via
// Game.DefineCombat. SoT = damage actually resolves after load — before this
// wiring the matrix was unbound and every hit dropped (victim Life unchanged).
// The shipped table is 1.0x (normal=[1000]) against unarmored, so a 40-damage
// hit drops Life by exactly 40.
func TestLoadWorldInstallsCombatMatrix(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	g, cleanup, err := loadWorld(w, 1, 50_000_000)
	if err != nil {
		t.Fatalf("loadWorld: %v", err)
	}
	defer cleanup()
	src := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	tgt := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 5, Y: 0}, api.Deg(0))
	before := tgt.Life()
	if !src.Damage(tgt, 40) {
		t.Fatal("Unit.Damage did not queue")
	}
	g.Advance(2)
	drop := before - tgt.Life()
	t.Logf("FSV #406 world-load combat: data/combat 1.0x matrix installed; 40 damage dropped Life %.1f (want 40)", drop)
	if drop != 40 {
		t.Fatalf("Life dropped %.1f, want 40 — combat matrix not installed by loadWorld (every hit would drop)", drop)
	}
}

// TestLoadWorldDeterminismLuaFSV: #271 — the committed determinism-lua scenario
// (math.random + pairs + string.format + coroutines + OnEvent) runs 10k ticks
// headless and produces a bit-identical state hash across same-seed runs, and a
// DIFFERENT hash under a different seed (teeth). SoT = g.StateHash() + the live
// unit count (the scenario kills exactly one of four).
func TestLoadWorldDeterminismLuaFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick determinism scenario is slow; run without -short")
	}
	const world = "../../worlds/determinism-lua"
	run := func(seed int64) (uint64, int) {
		g, cleanup, err := loadWorld(world, seed, 50_000_000)
		if err != nil {
			t.Fatalf("determinism-lua must load: %v", err)
		}
		defer cleanup()
		g.Advance(10000)
		alive := len(g.UnitsInRange(api.Vec2{X: 300, Y: 300}, 5000, nil))
		return g.StateHash(), alive
	}
	h1, alive1 := run(7)
	h2, alive2 := run(7)
	hOther, _ := run(9)
	t.Logf("FSV determinism-lua: seed7 run1=%#x run2=%#x | seed9=%#x | alive=%d (golden=%#x)", h1, h2, hOther, alive1, goldenDetLua)
	if h1 != h2 {
		t.Fatalf("NOT deterministic across same-seed runs: %#x != %#x", h1, h2)
	}
	if alive1 != alive2 {
		t.Fatalf("alive count not deterministic: %d != %d", alive1, alive2)
	}
	if hOther == h1 {
		t.Fatal("different seed produced the same hash — math.random not feeding the scenario (gate is blind)")
	}
	if goldenDetLua != 0 && h1 != goldenDetLua {
		t.Fatalf("golden mismatch: got %#x want %#x (intentional? update goldenDetLua)", h1, goldenDetLua)
	}
}

// TestLoadWorldMathRandomBoundFSV: #400 — a world using math.random must load
// (RandomSource bound to the sim PRNG) and be deterministic, not raise "no
// deterministic source bound". SoT = the unit position math.random computed,
// read back over two same-seed loads.
func TestLoadWorldMathRandomBoundFSV(t *testing.T) {
	lua := "Game_SetTimeOfDay(9.0)\n" +
		"local x = math.random() * 1000\n" +
		"Game_CreateUnit(Game_Player(0), Game_UnitType(\"hfoo\"), { x = x, y = 50 }, 0)\n"
	w := writeWorld(t, tomlDamageTable, lua)
	readX := func() float64 {
		g, cleanup, err := loadWorld(w, 7, 50_000_000)
		if err != nil {
			t.Fatalf("math.random world must load (#400): %v", err)
		}
		defer cleanup()
		us := g.UnitsInRange(api.Vec2{X: 500, Y: 50}, 2000, nil)
		if len(us) != 1 {
			t.Fatalf("want 1 unit placed by math.random, got %d", len(us))
		}
		return us[0].Position().X
	}
	x1, x2 := readX(), readX()
	t.Logf("FSV #400: math.random world loaded; unit x run1=%.4f run2=%.4f (seed 7)", x1, x2)
	if x1 != x2 {
		t.Fatalf("math.random not deterministic across same-seed loads: %v != %v", x1, x2)
	}
}

// TestLoadWorldMissingDamageTableFailsLoud: edge 4 — a missing data table is a
// loud load-time failure.
func TestLoadWorldMissingDamageTableFailsLoud(t *testing.T) {
	w := writeWorld(t, "", "Game_SetTimeOfDay(12.0)\n") // no damage-table
	_, _, err := loadWorld(w, 1, 50_000_000)
	if err == nil {
		t.Fatal("missing damage-table must fail loudly, got nil")
	}
	t.Logf("FSV missing-table: %v", err)
	if !strings.Contains(err.Error(), "damage-table") {
		t.Errorf("error %q should name the missing table", err)
	}
}

// TestLoadWorldSyntaxErrorFailsLoud: edge 1 — a Lua syntax error fails at load
// with file:line, before any tick.
func TestLoadWorldSyntaxErrorFailsLoud(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\nthis is not lua $$$\n")
	_, _, err := loadWorld(w, 1, 50_000_000)
	if err == nil {
		t.Fatal("syntax error must fail at load, got nil")
	}
	t.Logf("FSV syntax-error: %v", err)
	if !strings.Contains(err.Error(), "main.lua") {
		t.Errorf("error %q should name the offending file", err)
	}
}
