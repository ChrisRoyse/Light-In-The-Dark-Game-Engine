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

// TestLoadWorldFailsClosedOnAbilities: a world shipping a table with no install
// seam is refused loudly — never partially loaded.
func TestLoadWorldFailsClosedOnAbilities(t *testing.T) {
	w := writeWorld(t, tomlDamageTable, "Game_SetTimeOfDay(12.0)\n")
	mk(t, filepath.Join(w, "data", "abilities", "core.toml"), "[[ability]]\nid = \"defend\"\nname = \"Defend\"\n")
	_, _, err := loadWorld(w, 1, 50_000_000)
	if err == nil {
		t.Fatal("world with abilities must be refused (fail-closed), got nil")
	}
	t.Logf("FSV fail-closed: %v", err)
	if !strings.Contains(err.Error(), "ability") {
		t.Errorf("error %q should name the uninstallable table", err)
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
