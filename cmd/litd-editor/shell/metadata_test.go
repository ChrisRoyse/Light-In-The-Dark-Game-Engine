package shell

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestMetadataStartsExportFSV(t *testing.T) {
	app := newTestApp(t)
	dir := filepath.Join(t.TempDir(), "world")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	players := sourceform.Players{Min: 1, Max: 8, Suggested: 4}
	if err := app.SetMapMetadata("Metadata FSV", "four-start shell export", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		t.Fatal(err)
	}
	for _, start := range []struct {
		player int
		x, y   int
	}{
		{1, 1, 1},
		{2, 2, 1},
		{3, 1, 2},
		{4, 2, 2},
	} {
		if err := app.PutStartLocationCell(start.player, start.x, start.y); err != nil {
			t.Fatalf("put start p%d: %v", start.player, err)
		}
	}
	happy := app.Snapshot()
	t.Logf("FSV metadata happy path snapshot: starts=%+v players=%+v engine=%q", happy.World.Starts, happy.World.Players, happy.World.EngineRange)
	if len(happy.World.Starts) != 4 || happy.World.Players.Suggested != 4 {
		t.Fatalf("metadata happy path snapshot mismatch: %+v", happy.World)
	}

	beforeDup := append([]sourceform.StartLocation(nil), happy.World.Starts...)
	dupErr := app.AddStartLocationCell(2, 3, 3)
	afterDup := app.Snapshot()
	t.Logf("FSV duplicate start edge before=%+v err=%v after=%+v status=%q", beforeDup, dupErr, afterDup.World.Starts, afterDup.Status)
	if dupErr == nil || !slices.Equal(beforeDup, afterDup.World.Starts) || !strings.Contains(afterDup.Error, "duplicate start location") {
		t.Fatalf("duplicate start was not rejected cleanly: err=%v after=%+v", dupErr, afterDup)
	}

	if err := app.SwitchMode(ModeTerrain); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(7, 7); err != nil {
		t.Fatal(err)
	}
	beforeUnwalkable := app.Snapshot().World.Starts
	unwalkableErr := app.PutStartLocationCell(5, 7, 7)
	afterUnwalkable := app.Snapshot()
	t.Logf("FSV unwalkable start edge before=%+v err=%v after=%+v status=%q", beforeUnwalkable, unwalkableErr, afterUnwalkable.World.Starts, afterUnwalkable.Status)
	if unwalkableErr == nil || !slices.Equal(beforeUnwalkable, afterUnwalkable.World.Starts) || !strings.Contains(afterUnwalkable.Error, "unwalkable cell") {
		t.Fatalf("unwalkable start was not rejected cleanly: err=%v after=%+v", unwalkableErr, afterUnwalkable)
	}

	beforeName := app.Snapshot().World.Name
	emptyNameErr := app.SetMapMetadata("", "four-start shell export", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat")
	afterEmptyName := app.Snapshot()
	t.Logf("FSV empty-name metadata edge beforeName=%q err=%v afterName=%q error=%q", beforeName, emptyNameErr, afterEmptyName.World.Name, afterEmptyName.Error)
	if emptyNameErr == nil || afterEmptyName.World.Name != beforeName || !strings.Contains(afterEmptyName.Error, "name, description, and engine are required") {
		t.Fatalf("empty name should be rejected without changing world: err=%v after=%+v", emptyNameErr, afterEmptyName)
	}

	if err := app.Save(); err != nil {
		t.Fatalf("save valid metadata state: %v", err)
	}
	worldBytes, err := os.ReadFile(filepath.Join(dir, "world.toml"))
	if err != nil {
		t.Fatal(err)
	}
	terrainBytes, err := os.ReadFile(filepath.Join(dir, "map", "terrain.toml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV shell saved world.toml:\n%s", worldBytes)
	t.Logf("FSV shell saved terrain.toml:\n%s", terrainBytes)
	if !strings.Contains(string(worldBytes), `name = "Metadata FSV"`) || !strings.Contains(string(terrainBytes), "player = 4") {
		t.Fatalf("saved source bytes missing metadata/start edits")
	}

	arc := filepath.Join(t.TempDir(), "metadata-fsv.litdworld")
	if err := app.ExportArchive(arc); err != nil {
		t.Fatalf("export archive: %v", err)
	}
	opened, err := worldarchive.Open(arc, "")
	if err != nil {
		t.Fatalf("open exported archive: %v", err)
	}
	defer opened.Close()
	t.Logf("FSV shell archive manifest: title=%q desc=%q players=%+v tileset=%q splat=%q starts=%+v", opened.Manifest.Title, opened.Manifest.Description, opened.Manifest.Players, opened.Manifest.Tileset, opened.Manifest.SplatSet, opened.Manifest.StartLocations)
	wantStarts := []worldarchive.StartLocation{
		{Player: 1, Cell: [2]int{1, 1}},
		{Player: 2, Cell: [2]int{2, 1}},
		{Player: 3, Cell: [2]int{1, 2}},
		{Player: 4, Cell: [2]int{2, 2}},
	}
	if opened.Manifest.Title != "Metadata FSV" || opened.Manifest.Description != "four-start shell export" || opened.Manifest.Players.Suggested != 4 || opened.Manifest.Tileset != "vigil-lowlands" || opened.Manifest.SplatSet != "dawn-splat" || !slices.Equal(opened.Manifest.StartLocations, wantStarts) {
		t.Fatalf("manifest did not carry metadata/start state: %+v", opened.Manifest)
	}
}
