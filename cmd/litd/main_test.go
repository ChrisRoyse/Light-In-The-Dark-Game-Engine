package main

// #268 regression coverage for the shipped world-loader binary. The manual FSV
// (closing comment) runs ./bin/litd against worlds/dev-sandbox and reads the
// state JSON; these tests lock the same behavior against the sim SoT (unit
// state) and the loud-failure contract.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
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

// TestUninstallableTablesGate: the fail-closed gate now passes the #394-installable
// tables and still refuses the two that remain seamless (resource nodes, heroes).
func TestUninstallableTablesGate(t *testing.T) {
	if got := uninstallableTables(&data.Tables{Abilities: []data.Ability{{ID: "x"}}, Items: []data.Item{{ID: "y"}}}); got != "" {
		t.Errorf("abilities+items must be installable now, gate said %q", got)
	}
	if got := uninstallableTables(&data.Tables{Hero: &data.HeroTables{}}); !strings.Contains(got, "hero") {
		t.Errorf("hero tables must still be refused, got %q", got)
	}
	if got := uninstallableTables(&data.Tables{Nodes: []data.ResourceNodeType{{}}}); !strings.Contains(got, "resource-node") {
		t.Errorf("resource-node tables must still be refused, got %q", got)
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
