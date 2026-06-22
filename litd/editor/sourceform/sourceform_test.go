package sourceform

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
)

func TestSourceFormIdempotentSaveSampleFSV(t *testing.T) {
	dir := copySampleWorld(t)
	before := mustReadTree(t, dir)

	w, err := Load(dir)
	if err != nil {
		t.Fatalf("load sample: %v", err)
	}
	if w.Dirty() {
		t.Fatal("fresh load should not be dirty")
	}
	if err := w.Save(""); err != nil {
		t.Fatalf("save sample: %v", err)
	}
	after := mustReadTree(t, dir)
	if diff := changedFiles(before, after); len(diff) != 0 {
		t.Fatalf("idempotent save changed files: %v", diff)
	}
	t.Logf("FSV idempotent source of truth: %s unchanged (%d bytes)", worldFile, len(after[worldFile]))
	t.Logf("FSV idempotent source of truth: %s unchanged:\n%s", entitiesFile, after[entitiesFile])
}

func TestSourceFormMoveEntityLocalizedDiffFSV(t *testing.T) {
	dir := copySampleWorld(t)
	before := mustReadTree(t, dir)
	w, err := Load(dir)
	if err != nil {
		t.Fatalf("load sample: %v", err)
	}

	if err := w.MoveEntity(2, [2]int{12288, 4096}, 8192); err != nil {
		t.Fatalf("move entity: %v", err)
	}
	if !w.Dirty() {
		t.Fatal("move should mark world dirty before save")
	}
	if err := w.Save(""); err != nil {
		t.Fatalf("save moved entity: %v", err)
	}
	after := mustReadTree(t, dir)
	if diff := changedFiles(before, after); !slices.Equal(diff, []string{entitiesFile}) {
		t.Fatalf("move changed files %v, want only %s", diff, entitiesFile)
	}
	line, oldLine, newLine := singleChangedLine(t, string(before[entitiesFile]), string(after[entitiesFile]))
	t.Logf("FSV entity move before line %d: %s", line, oldLine)
	t.Logf("FSV entity move after  line %d: %s", line, newLine)
	if !strings.Contains(newLine, `id = 2`) || !strings.Contains(newLine, `pos = [12288, 4096]`) || !strings.Contains(newLine, `rotation = 8192`) {
		t.Fatalf("changed line does not contain moved entity state: %s", newLine)
	}
}

func TestSourceFormTerrainAndScriptOnlyEdgesFSV(t *testing.T) {
	dir := copySampleWorld(t)
	if err := os.MkdirAll(filepath.Join(dir, "scripts"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "scripts", "main.lua"), []byte("-- base\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	beforeTerrain := mustReadTree(t, dir)
	w, err := Load(dir)
	if err != nil {
		t.Fatalf("load with script: %v", err)
	}
	if err := w.SetGridCell(GridHeight, 1, 1, 7); err != nil {
		t.Fatalf("set height: %v", err)
	}
	if err := w.Save(""); err != nil {
		t.Fatalf("save height edit: %v", err)
	}
	afterTerrain := mustReadTree(t, dir)
	if diff := changedFiles(beforeTerrain, afterTerrain); !slices.Equal(diff, []string{heightFile}) {
		t.Fatalf("height edit changed files %v, want only %s", diff, heightFile)
	}
	t.Logf("FSV terrain-only before row: %q", strings.Split(strings.TrimSuffix(string(beforeTerrain[heightFile]), "\n"), "\n")[1])
	t.Logf("FSV terrain-only after  row: %q", strings.Split(strings.TrimSuffix(string(afterTerrain[heightFile]), "\n"), "\n")[1])

	beforeScript := mustReadTree(t, dir)
	w, err = Load(dir)
	if err != nil {
		t.Fatalf("reload before script edit: %v", err)
	}
	if err := w.SetScript("scripts/main.lua", []byte("-- changed\nGame_SetTimeOfDay(12.0)\n")); err != nil {
		t.Fatalf("set script: %v", err)
	}
	if err := w.Save(""); err != nil {
		t.Fatalf("save script edit: %v", err)
	}
	afterScript := mustReadTree(t, dir)
	if diff := changedFiles(beforeScript, afterScript); !slices.Equal(diff, []string{"scripts/main.lua"}) {
		t.Fatalf("script edit changed files %v, want only scripts/main.lua", diff)
	}
	t.Logf("FSV script-only before:\n%s", beforeScript["scripts/main.lua"])
	t.Logf("FSV script-only after:\n%s", afterScript["scripts/main.lua"])
}

func TestSourceFormCliffRampRoundTripFSV(t *testing.T) {
	dir := t.TempDir()
	w := &World{
		Metadata: Metadata{
			Format:      1,
			ID:          "ramp-fsv",
			Name:        "Ramp FSV",
			Description: "Synthetic ramp source-form world",
			Authors:     []string{"FSV"},
			Engine:      ">=0.1.0 <0.2.0",
			Players:     Players{Min: 1, Max: 2, Suggested: 1},
			SeedPolicy:  "host",
		},
		Terrain: Terrain{Width: 3, Height: 1, Tileset: "test", StartLocations: []StartLocation{{Player: 1, Cell: [2]int{0, 0}}}},
		Height:  [][]int{{0, 0, 0}},
		Pathing: DefaultPathingGrid(3, 1),
		Cliff:   [][]CliffCell{{{Level: 0}, {Level: 0, Ramp: true}, {Level: 1}}},
		Splat:   [][]SplatWeight{{{A: 255}, {A: 255}, {A: 255}}},
	}
	if err := w.Save(dir); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(cliffFile)))
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	lowToRamp, err := loaded.CliffStepLegal(0, 0, 1, 0)
	if err != nil {
		t.Fatal(err)
	}
	rampToHigh, err := loaded.CliffStepLegal(1, 0, 2, 0)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV ramp source bytes: %q", strings.TrimSpace(string(body)))
	t.Logf("FSV ramp cells: low=%+v ramp=%+v high=%+v pathable low->ramp=%v ramp->high=%v", loaded.Cliff[0][0], loaded.Cliff[0][1], loaded.Cliff[0][2], lowToRamp, rampToHigh)
	if strings.TrimSpace(string(body)) != "0 r0 1" {
		t.Fatalf("cliff.txt = %q, want canonical ramp row", string(body))
	}
	if !loaded.Cliff[0][1].Ramp || !lowToRamp || !rampToHigh {
		t.Fatalf("ramp did not round-trip as pathable: %+v", loaded.Cliff[0])
	}

	bad := t.TempDir()
	writeSyntheticWorld(t, bad, 1)
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(terrainFile)), []byte("width = 3\nheight = 1\ntileset = \"test\"\n\n[[start]]\nplayer = 1\ncell = [0, 0]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(heightFile)), []byte("0 0 0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(pathingFile)), []byte("3 3 3 3 3 3 3 3 3 3 3 3\n3 3 3 3 3 3 3 3 3 3 3 3\n3 3 3 3 3 3 3 3 3 3 3 3\n3 3 3 3 3 3 3 3 3 3 3 3\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(cliffFile)), []byte("0 r0 0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(splatFile)), []byte("255,0,0,0 255,0,0,0 255,0,0,0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(bad)
	t.Logf("FSV invalid ramp load: row=%q err=%v", "0 r0 0", err)
	if err == nil || !strings.Contains(err.Error(), "ramp at (1,0)") {
		t.Fatalf("invalid ramp should fail closed, got %v", err)
	}
}

func TestSourceFormSplatWeightsRoundTripFSV(t *testing.T) {
	dir := t.TempDir()
	writeSyntheticWorld(t, dir, 1)
	w, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.SetSplatCell(0, 0, SplatWeight{A: 10, B: 20, C: 30, D: 195}); err != nil {
		t.Fatal(err)
	}
	if err := w.Save(""); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(splatFile)))
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV splat source bytes: %q", strings.TrimSpace(string(body)))
	t.Logf("FSV splat cell after load: %+v", loaded.Splat[0][0])
	if strings.TrimSpace(string(body)) != "10,20,30,195" || loaded.Splat[0][0] != (SplatWeight{A: 10, B: 20, C: 30, D: 195}) {
		t.Fatalf("splat did not round-trip canonically: body=%q cell=%+v", string(body), loaded.Splat[0][0])
	}

	bad := t.TempDir()
	writeSyntheticWorld(t, bad, 1)
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(splatFile)), []byte("10,20,30,40\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(bad)
	t.Logf("FSV invalid splat load: row=%q err=%v", "10,20,30,40", err)
	if err == nil || !strings.Contains(err.Error(), "weights sum to 100, want 255") {
		t.Fatalf("invalid splat should fail closed, got %v", err)
	}
}

func TestSourceFormPathingRoundTripAndEdgesFSV(t *testing.T) {
	dir := t.TempDir()
	writeSyntheticWorld(t, dir, 1)
	w, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	before := renderPathingGrid(w.Pathing)
	if err := w.SetPathingCell(0, 0, 0); err != nil {
		t.Fatal(err)
	}
	clear, err := w.PathingFootprintClear(0, 0, 1, 1, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.Save(""); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(pathingFile)))
	if err != nil {
		t.Fatal(err)
	}
	loaded, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV pathing before bytes:\n%s", before)
	t.Logf("FSV pathing after bytes:\n%s", body)
	t.Logf("FSV pathing cell after load: flags=%d footprintClear=%v", loaded.Pathing[0][0], clear)
	if clear || loaded.Pathing[0][0] != 0 || !strings.HasPrefix(string(body), "0 3 3 3\n") {
		t.Fatalf("pathing did not round-trip blocked cell: clear=%v cell=%d body=%q", clear, loaded.Pathing[0][0], string(body))
	}

	beforeOOB := loaded.Pathing[0][0]
	oobErr := loaded.SetPathingCell(4, 0, PathWalkable)
	t.Logf("FSV pathing out-of-bounds beforeCell=%d err=%v afterCell=%d", beforeOOB, oobErr, loaded.Pathing[0][0])
	if oobErr == nil || loaded.Pathing[0][0] != beforeOOB {
		t.Fatalf("out-of-bounds pathing edit should fail without mutation: err=%v cell=%d", oobErr, loaded.Pathing[0][0])
	}

	missing := t.TempDir()
	writeSyntheticWorld(t, missing, 1)
	if err := os.Remove(filepath.Join(missing, filepath.FromSlash(pathingFile))); err != nil {
		t.Fatal(err)
	}
	_, err = Load(missing)
	t.Logf("FSV missing pathing edge: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "missing required file map/pathing.txt") {
		t.Fatalf("missing pathing should fail closed, got %v", err)
	}

	badFlags := t.TempDir()
	writeSyntheticWorld(t, badFlags, 1)
	grid := DefaultPathingGrid(1, 1)
	grid[0][0] = PathWalkable | PathWater
	if err := os.WriteFile(filepath.Join(badFlags, filepath.FromSlash(pathingFile)), renderPathingGrid(grid), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(badFlags)
	t.Logf("FSV invalid pathing flags edge: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "water path flags") {
		t.Fatalf("invalid water+walk flags should fail closed, got %v", err)
	}

	truncated := t.TempDir()
	writeSyntheticWorld(t, truncated, 1)
	if err := os.WriteFile(filepath.Join(truncated, filepath.FromSlash(pathingFile)), []byte("3 3 3\n3 3 3 3\n3 3 3 3\n3 3 3 3\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(truncated)
	t.Logf("FSV truncated pathing row edge: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "row 0 has 3 columns, want 4") {
		t.Fatalf("truncated pathing row should fail closed, got %v", err)
	}
}

func TestSourceFormPlacementTransformRoundTripFSV(t *testing.T) {
	dir := t.TempDir()
	writeSyntheticWorld(t, dir, 1)
	legacy := `# one element per line; ordered by id; ids never reused
entities = [
  { id = 1, type = "footman", player = 0, pos = [4096, 4096], facing = 8192 },
]
`
	doodads := `# hand-authored unsorted scenery
doodads = [
  { id = 2, type = "kaykit-hexagon/rock_single_A.glb", pos = [8192, 4096], rotation = 4096, scale = 750 },
  { id = 1, type = "kaykit-hexagon/tree_single_A.glb", pos = [4096, 8192], rotation = 0, scale = 1250 },
]
`
	if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(entitiesFile)), []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(doodadsFile)), []byte(doodads), 0o644); err != nil {
		t.Fatal(err)
	}
	w, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV legacy entity after load: %+v", w.Entities[0])
	t.Logf("FSV doodads after load sorted: %+v", w.Doodads)
	if w.Entities[0].Rotation != 8192 || w.Entities[0].Scale != PlacementScaleDefault {
		t.Fatalf("legacy facing/default scale did not normalize: %+v", w.Entities[0])
	}
	if len(w.Doodads) != 2 || w.Doodads[0].ID != 1 || w.Doodads[1].Scale != 750 {
		t.Fatalf("doodads did not parse/sort with exact scale: %+v", w.Doodads)
	}
	if err := w.Save(""); err != nil {
		t.Fatal(err)
	}
	entitiesBody, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(entitiesFile)))
	if err != nil {
		t.Fatal(err)
	}
	doodadsBody, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(doodadsFile)))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV canonical entities.toml:\n%s", entitiesBody)
	t.Logf("FSV canonical doodads.toml:\n%s", doodadsBody)
	if bytes.Contains(entitiesBody, []byte("facing")) || !bytes.Contains(entitiesBody, []byte(`rotation = 8192, scale = 1000`)) {
		t.Fatalf("entities did not canonicalize rotation/scale:\n%s", entitiesBody)
	}
	if !bytes.Contains(doodadsBody, []byte(`id = 1`)) || !bytes.Contains(doodadsBody, []byte(`scale = 1250`)) {
		t.Fatalf("doodads canonical source missing expected records:\n%s", doodadsBody)
	}

	bad := t.TempDir()
	writeSyntheticWorld(t, bad, 1)
	if err := os.WriteFile(filepath.Join(bad, filepath.FromSlash(doodadsFile)), []byte(`doodads = [
  { id = 1, type = "tree", pos = [0, 0], rotation = 0, scale = 0 },
]
`), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(bad)
	t.Logf("FSV invalid doodad scale: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "scale 0 outside") {
		t.Fatalf("invalid doodad scale should fail closed, got %v", err)
	}
}

func TestSourceFormRejectsMalformedEdgesFSV(t *testing.T) {
	_, err := Load(filepath.Join(t.TempDir(), "missing"))
	t.Logf("FSV missing directory edge: before=no directory after=err %v", err)
	if err == nil {
		t.Fatal("missing directory should error")
	}

	unknown := copySampleWorld(t)
	if err := os.WriteFile(filepath.Join(unknown, "typo.toml"), []byte("oops = true\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(unknown)
	t.Logf("FSV unknown file edge: before=typo.toml present after=err %v", err)
	if err == nil || !strings.Contains(err.Error(), "unknown file") {
		t.Fatalf("unknown file should error loudly, got %v", err)
	}

	dup := copySampleWorld(t)
	body := `entities = [
  { id = 1, type = "a", player = 0, pos = [0, 0], facing = 0 },
  { id = 1, type = "b", player = 0, pos = [1, 1], facing = 0 },
]
`
	if err := os.WriteFile(filepath.Join(dup, filepath.FromSlash(entitiesFile)), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err = Load(dup)
	t.Logf("FSV duplicate entity edge: before=two id=1 rows after=err %v", err)
	if err == nil || !strings.Contains(err.Error(), "duplicate entity id") {
		t.Fatalf("duplicate entity should error loudly, got %v", err)
	}
}

func TestSourceFormExportArchiveFSV(t *testing.T) {
	dir := copySampleWorld(t)
	if err := os.MkdirAll(filepath.Join(dir, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".git", "config"), []byte("[core]\nrepositoryformatversion = 0\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".gitattributes"), []byte("assets/** filter=lfs\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".gitignore"), []byte("*.tmp\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".gitmodules"), []byte(""), 0o644); err != nil {
		t.Fatal(err)
	}
	w, err := Load(dir)
	if err != nil {
		t.Fatalf("load sample: %v", err)
	}
	if err := w.MoveEntity(1, [2]int{2048, 2048}, 1024); err != nil {
		t.Fatal(err)
	}
	arc := filepath.Join(t.TempDir(), "sample.litdworld")
	opts := ExportOptions{
		EngineRange: ">=0.1.0 <0.2.0",
		Hosting:     worldpack.Hosting{Author: "Paula", Title: "First Light Sample", Description: "sourceform export fsv"},
	}
	if err := w.ExportArchive(arc, opts); err != nil {
		t.Fatalf("export archive: %v", err)
	}
	if w.Dirty() {
		t.Fatal("export should save pending edits and clear dirty state")
	}
	entitiesBytes, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(entitiesFile)))
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(entitiesBytes)
	wantHash := hex.EncodeToString(sum[:])

	opened, err := worldarchive.Open(arc, "")
	if err != nil {
		t.Fatalf("open exported archive: %v", err)
	}
	defer opened.Close()
	gotEntry, ok := opened.Manifest.Files[entitiesFile]
	if !ok {
		t.Fatalf("archive manifest missing %s", entitiesFile)
	}
	t.Logf("FSV export archive manifest: engine-range=%q files=%d %s hash=%s", opened.Manifest.EngineRange, len(opened.Manifest.Files), entitiesFile, gotEntry.Hash)
	if opened.Manifest.EngineRange != opts.EngineRange {
		t.Fatalf("engine range %q, want %q", opened.Manifest.EngineRange, opts.EngineRange)
	}
	for rel := range opened.Manifest.Files {
		if strings.HasPrefix(rel, ".git/") || rel == ".git" || rel == ".gitattributes" || rel == ".gitignore" || rel == ".gitmodules" {
			t.Fatalf("archive manifest leaked VCS metadata entry %q", rel)
		}
	}
	if gotEntry.Hash != wantHash {
		t.Fatalf("archive manifest hash %s, want source file hash %s", gotEntry.Hash, wantHash)
	}
	if !bytes.Contains(entitiesBytes, []byte(`pos = [2048, 2048]`)) {
		t.Fatalf("saved entities missing exported dirty edit:\n%s", entitiesBytes)
	}
}

func TestSourceFormMetadataStartsArchiveManifestFSV(t *testing.T) {
	dir := copySampleWorld(t)
	w, err := Load(dir)
	if err != nil {
		t.Fatalf("load sample: %v", err)
	}
	players := Players{Min: 1, Max: 8, Suggested: 4}
	if err := w.SetMapMetadata("Four Start FSV", "archive metadata source-form test", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		t.Fatalf("set metadata: %v", err)
	}
	for _, start := range []StartLocation{
		{Player: 1, Cell: [2]int{1, 1}},
		{Player: 2, Cell: [2]int{2, 1}},
		{Player: 3, Cell: [2]int{1, 2}},
		{Player: 4, Cell: [2]int{2, 2}},
	} {
		if err := w.PutStartLocation(start); err != nil {
			t.Fatalf("put start %+v: %v", start, err)
		}
	}
	beforeDuplicate := append([]StartLocation(nil), w.Terrain.StartLocations...)
	dupErr := w.AddStartLocation(StartLocation{Player: 2, Cell: [2]int{3, 3}})
	t.Logf("FSV duplicate start before=%+v err=%v after=%+v", beforeDuplicate, dupErr, w.Terrain.StartLocations)
	if dupErr == nil || len(w.Terrain.StartLocations) != 4 {
		t.Fatalf("duplicate start should be rejected without changing count: err=%v starts=%+v", dupErr, w.Terrain.StartLocations)
	}
	px, py := TerrainCellCenterPathingCell(7, 7)
	if err := w.SetPathingCell(px, py, 0); err != nil {
		t.Fatalf("block pathing for unbuildable edge: %v", err)
	}
	beforeUnwalkable := append([]StartLocation(nil), w.Terrain.StartLocations...)
	unwalkableErr := w.PutStartLocation(StartLocation{Player: 5, Cell: [2]int{7, 7}})
	t.Logf("FSV unwalkable start before=%+v err=%v after=%+v", beforeUnwalkable, unwalkableErr, w.Terrain.StartLocations)
	if unwalkableErr == nil || len(w.Terrain.StartLocations) != 4 {
		t.Fatalf("unwalkable start should be rejected without changing count: err=%v starts=%+v", unwalkableErr, w.Terrain.StartLocations)
	}
	validName := w.Metadata.Name
	w.Metadata.Name = ""
	saveErr := w.Save("")
	t.Logf("FSV empty-name save edge: beforeName=%q saveErr=%v sourceDir=%q", validName, saveErr, dir)
	if saveErr == nil || !strings.Contains(saveErr.Error(), "name, description, and engine are required") {
		t.Fatalf("empty name should block save, got %v", saveErr)
	}
	w.Metadata.Name = validName
	if err := w.Save(""); err != nil {
		t.Fatalf("save valid metadata/starts: %v", err)
	}
	worldBytes, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(worldFile)))
	if err != nil {
		t.Fatal(err)
	}
	terrainBytes, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(terrainFile)))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV saved world.toml:\n%s", worldBytes)
	t.Logf("FSV saved terrain.toml:\n%s", terrainBytes)
	arc := filepath.Join(t.TempDir(), "metadata-starts.litdworld")
	if err := w.ExportArchive(arc, ExportOptions{}); err != nil {
		t.Fatalf("export archive: %v", err)
	}
	opened, err := worldarchive.Open(arc, "")
	if err != nil {
		t.Fatalf("open exported archive: %v", err)
	}
	defer opened.Close()
	t.Logf("FSV manifest metadata: title=%q desc=%q players=%+v tileset=%q splat=%q starts=%+v", opened.Manifest.Title, opened.Manifest.Description, opened.Manifest.Players, opened.Manifest.Tileset, opened.Manifest.SplatSet, opened.Manifest.StartLocations)
	if opened.Manifest.Title != "Four Start FSV" || opened.Manifest.Description != "archive metadata source-form test" {
		t.Fatalf("manifest hosting metadata mismatch: %+v", opened.Manifest)
	}
	if opened.Manifest.Players.Min != 1 || opened.Manifest.Players.Max != 8 || opened.Manifest.Players.Suggested != 4 {
		t.Fatalf("manifest players mismatch: %+v", opened.Manifest.Players)
	}
	if opened.Manifest.Tileset != "vigil-lowlands" || opened.Manifest.SplatSet != "dawn-splat" {
		t.Fatalf("manifest terrain metadata mismatch: tileset=%q splat=%q", opened.Manifest.Tileset, opened.Manifest.SplatSet)
	}
	wantStarts := []worldarchive.StartLocation{
		{Player: 1, Cell: [2]int{1, 1}},
		{Player: 2, Cell: [2]int{2, 1}},
		{Player: 3, Cell: [2]int{1, 2}},
		{Player: 4, Cell: [2]int{2, 2}},
	}
	if !slices.Equal(opened.Manifest.StartLocations, wantStarts) {
		t.Fatalf("manifest starts = %+v, want %+v", opened.Manifest.StartLocations, wantStarts)
	}
}

func TestSourceFormConcurrentNonOverlappingMergeFSV(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	writeSyntheticWorld(t, dir, 500)
	runGit(t, dir, "init")
	runGit(t, dir, "config", "user.email", "fsv@example.invalid")
	runGit(t, dir, "config", "user.name", "FSV")
	runGit(t, dir, "add", ".")
	runGit(t, dir, "commit", "-m", "base")

	runGit(t, dir, "checkout", "-b", "branch-a")
	editEntity(t, dir, 10, [2]int{10, 9900}, 10)
	diffA := runGit(t, dir, "diff", "--", filepath.FromSlash(entitiesFile))
	t.Logf("FSV branch-a diff:\n%s", diffA)
	runGit(t, dir, "commit", "-am", "move entity 10")

	runGit(t, dir, "checkout", "master")
	runGit(t, dir, "checkout", "-b", "branch-b")
	editEntity(t, dir, 490, [2]int{490, 9900}, 490)
	diffB := runGit(t, dir, "diff", "--", filepath.FromSlash(entitiesFile))
	t.Logf("FSV branch-b diff:\n%s", diffB)
	runGit(t, dir, "commit", "-am", "move entity 490")

	mergeOut := runGit(t, dir, "merge", "branch-a")
	body, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(entitiesFile)))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV non-overlapping merge output:\n%s", mergeOut)
	if !bytes.Contains(body, []byte(`id = 10, type = "unit-010", player = 0, pos = [10, 9900], rotation = 10, scale = 1000`)) {
		t.Fatal("merged source of truth missing branch-a entity 10 edit")
	}
	if !bytes.Contains(body, []byte(`id = 490, type = "unit-490", player = 0, pos = [490, 9900], rotation = 490, scale = 1000`)) {
		t.Fatal("merged source of truth missing branch-b entity 490 edit")
	}
}

func copySampleWorld(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	root := filepath.Clean(filepath.Join(filepath.Dir(file), "../../.."))
	src := filepath.Join(root, "examples", "firstlight-sample")
	dst := t.TempDir()
	copyDir(t, src, dst)
	return dst
}

func copyDir(t *testing.T, src, dst string) {
	t.Helper()
	err := filepath.WalkDir(src, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		out := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(out, 0o755)
		}
		body, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(out, body, 0o644)
	})
	if err != nil {
		t.Fatal(err)
	}
}

func mustReadTree(t *testing.T, dir string) map[string][]byte {
	t.Helper()
	files, err := readTree(dir)
	if err != nil {
		t.Fatal(err)
	}
	return files
}

func changedFiles(before, after map[string][]byte) []string {
	seen := map[string]bool{}
	for rel := range before {
		seen[rel] = true
	}
	for rel := range after {
		seen[rel] = true
	}
	var changed []string
	for rel := range seen {
		if !bytes.Equal(before[rel], after[rel]) {
			changed = append(changed, rel)
		}
	}
	sort.Strings(changed)
	return changed
}

func singleChangedLine(t *testing.T, before, after string) (line int, oldLine, newLine string) {
	t.Helper()
	a := strings.Split(strings.TrimSuffix(before, "\n"), "\n")
	b := strings.Split(strings.TrimSuffix(after, "\n"), "\n")
	if len(a) != len(b) {
		t.Fatalf("line counts differ: %d vs %d", len(a), len(b))
	}
	count := 0
	for i := range a {
		if a[i] != b[i] {
			count++
			line, oldLine, newLine = i+1, a[i], b[i]
		}
	}
	if count != 1 {
		t.Fatalf("changed line count = %d, want 1", count)
	}
	return line, oldLine, newLine
}

func writeSyntheticWorld(t *testing.T, dir string, nEntities int) {
	t.Helper()
	meta := Metadata{
		Format:      1,
		ID:          "merge-fsv",
		Name:        "Merge FSV",
		Description: "Synthetic merge world",
		Authors:     []string{"FSV"},
		Engine:      ">=0.1.0 <0.2.0",
		Players:     Players{Min: 1, Max: 2, Suggested: 1},
		SeedPolicy:  "host",
	}
	terrain := Terrain{Width: 1, Height: 1, Tileset: "test", StartLocations: []StartLocation{{Player: 1, Cell: [2]int{0, 0}}}}
	entities := make([]Entity, 0, nEntities)
	for i := 1; i <= nEntities; i++ {
		entities = append(entities, Entity{
			ID:       uint32(i),
			Type:     fmt.Sprintf("unit-%03d", i),
			Player:   0,
			Pos:      [2]int{i, i},
			Rotation: 0,
			Scale:    PlacementScaleDefault,
		})
	}
	files := map[string][]byte{
		worldFile:    renderWorld(meta),
		terrainFile:  renderTerrain(terrain),
		pathingFile:  renderPathingGrid(DefaultPathingGrid(terrain.Width, terrain.Height)),
		heightFile:   []byte("0\n"),
		cliffFile:    []byte("0\n"),
		splatFile:    []byte("255,0,0,0\n"),
		entitiesFile: renderEntities(entities),
		doodadsFile:  renderDoodads(nil),
	}
	for rel, body := range files {
		if err := writeFileIfChanged(dir, rel, body); err != nil {
			t.Fatal(err)
		}
	}
}

func editEntity(t *testing.T, dir string, id uint32, pos [2]int, facing int) {
	t.Helper()
	w, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if err := w.MoveEntity(id, pos, facing); err != nil {
		t.Fatal(err)
	}
	if err := w.Save(""); err != nil {
		t.Fatal(err)
	}
}

func runGit(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %s failed: %v\n%s", strings.Join(args, " "), err, out)
	}
	return string(out)
}
