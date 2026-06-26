package worldhost

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

const (
	worldhostDamageTable = "attack-types = [\"normal\"]\narmor-types = [\"unarmored\"]\n[coefficients]\nnormal = [1000]\n"
	worldhostUnitTable   = "[[unit]]\nid = \"hfoo\"\nlife = 100\narmor-type = \"unarmored\"\nmove-speed = 270\nturn-rate = 0.6\ncollision-size = 16\npathing = \"ground\"\n"
)

// TestUninstallableTablesGate: every content table now has an install seam —
// abilities/items (#394), heroes (#396), resource-node types (#401) — so the
// fail-closed gate refuses nothing today. (Moved from cmd/litd with the function
// it covers, #490.)
func TestUninstallableTablesGate(t *testing.T) {
	if got := uninstallableTables(&data.Tables{Abilities: []data.Ability{{ID: "x"}}, Items: []data.Item{{ID: "y"}}}); got != "" {
		t.Errorf("abilities+items must be installable now, gate said %q", got)
	}
	if got := uninstallableTables(&data.Tables{Hero: &data.HeroTables{}}); got != "" {
		t.Errorf("hero tables must be installable now (#396), gate said %q", got)
	}
	if got := uninstallableTables(&data.Tables{Nodes: []data.ResourceNodeType{{}}}); got != "" {
		t.Errorf("resource-node tables must be installable now (#401), gate said %q", got)
	}
}

func TestLoadFSInstallsRuntimeMapPathingFSV(t *testing.T) {
	withMap := writeWorldhostPathingWorld(t, true)
	h, err := Load(withMap, 1, 50_000_000)
	if err != nil {
		t.Fatalf("load mapped world: %v", err)
	}
	defer h.Close()
	mappedUnit := firstUnitAt(t, h.Game, api.Vec2{X: 64, Y: 64})
	beforeMapped := mappedUnit.Position()
	mappedIssued := mappedUnit.Order(api.OrderMove, api.TargetPoint(api.Vec2{X: 20_000, Y: 64}))
	h.Game.Advance(1)
	afterMapped := mappedUnit.Position()

	withoutMap := writeWorldhostPathingWorld(t, false)
	h2, err := Load(withoutMap, 1, 50_000_000)
	if err != nil {
		t.Fatalf("load mapless world: %v", err)
	}
	defer h2.Close()
	maplessUnit := firstUnitAt(t, h2.Game, api.Vec2{X: 64, Y: 64})
	beforeMapless := maplessUnit.Position()
	maplessIssued := maplessUnit.Order(api.OrderMove, api.TargetPoint(api.Vec2{X: 20_000, Y: 64}))
	h2.Game.Advance(1)
	afterMapless := maplessUnit.Position()

	t.Logf("FSV runtime map pathing: mapped before=%+v issued=%v after=%+v; mapless before=%+v issued=%v after=%+v",
		beforeMapped, mappedIssued, afterMapped, beforeMapless, maplessIssued, afterMapless)
	if afterMapped != beforeMapped {
		t.Fatalf("mapped world should hold position after out-of-grid move: issued=%v before=%+v after=%+v", mappedIssued, beforeMapped, afterMapped)
	}
	if !maplessIssued || afterMapless == beforeMapless {
		t.Fatalf("mapless world should keep legacy direct movement: issued=%v before=%+v after=%+v", maplessIssued, beforeMapless, afterMapless)
	}
}

func TestLoadRuntimeMapSelectionEdgesFSV(t *testing.T) {
	empty := t.TempDir()
	noMap, err := loadRuntimeMap(os.DirFS(empty))
	t.Logf("FSV runtime map edge no maps dir: map=%v err=%v", noMap, err)
	if err != nil || noMap != nil {
		t.Fatalf("no maps directory should be legacy nil map, got map=%v err=%v", noMap, err)
	}

	ignored := t.TempDir()
	writeWorldhostFile(t, ignored, "maps/readme.txt", "not a map\n")
	noTerrain, err := loadRuntimeMap(os.DirFS(ignored))
	t.Logf("FSV runtime map edge maps dir without terrain child: map=%v err=%v", noTerrain, err)
	if err != nil || noTerrain != nil {
		t.Fatalf("maps dir without terrain child should be legacy nil map, got map=%v err=%v", noTerrain, err)
	}

	multi := t.TempDir()
	writeWorldhostFile(t, multi, "maps/a/terrain.toml", "version = 1\n")
	writeWorldhostFile(t, multi, "maps/b/terrain.toml", "version = 1\n")
	_, err = loadRuntimeMap(os.DirFS(multi))
	t.Logf("FSV runtime map edge multiple map dirs: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "2 runtime maps") {
		t.Fatalf("multiple runtime maps should fail closed, got %v", err)
	}
}

func writeWorldhostPathingWorld(t *testing.T, includeMap bool) string {
	t.Helper()
	root := t.TempDir()
	writeWorldhostFile(t, root, "data/combat/damage-table.toml", worldhostDamageTable)
	writeWorldhostFile(t, root, "data/units/units.toml", worldhostUnitTable)
	writeWorldhostFile(t, root, "data/placement/editor.toml", "[[unit]]\ntype = \"hfoo\"\nowner = 0\nx = 64\ny = 64\nfacing = 0\n")
	writeWorldhostFile(t, root, "main.lua", "Game_SetTimeOfDay(12.0)\n")
	if includeMap {
		writeWorldhostFile(t, root, "data/maps/pathing/terrain.toml", "version = 1\nwidth = 2\nheight = 2\nbiome = \"test\"\npathing-scale = 4\n\n[[start]]\nplayer = 0\ncell = [2, 2]\n")
		writeWorldhostFile(t, root, "data/maps/pathing/pathing.txt", "@repeat 8 3*8\n")
		writeWorldhostFile(t, root, "data/maps/pathing/cliff.txt", "@repeat 8 0*8\n")
		writeWorldhostFile(t, root, "data/maps/pathing/height.txt", "@repeat 3 0*3\n")
		writeWorldhostFile(t, root, "data/maps/pathing/splat.txt", "@repeat 2 255,0,0,0*2\n")
		writeWorldhostFile(t, root, "data/maps/pathing/doodads.toml", "")
	}
	return root
}

func writeWorldhostFile(t *testing.T, root, rel, body string) {
	t.Helper()
	out := filepath.Join(root, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(out, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func firstUnitAt(t *testing.T, g *api.Game, pos api.Vec2) api.Unit {
	t.Helper()
	units := g.UnitsInRange(pos, 8, nil)
	if len(units) != 1 {
		t.Fatalf("UnitsInRange(%+v)=%d, want one", pos, len(units))
	}
	return units[0]
}
