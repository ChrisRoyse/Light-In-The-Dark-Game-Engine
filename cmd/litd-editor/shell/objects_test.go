package shell

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestObjectPlacementPaletteTransformsAndEdgesFSV(t *testing.T) {
	app := newCommandTestApp(t)
	palette := app.ObjectSnapshot().Palette
	t.Logf("FSV object palette loaded: %+v", palette)
	if !hasPalette(palette, ObjectKindUnit, "footman") || !hasPalette(palette, ObjectKindUnit, "archer") {
		t.Fatalf("unit palette missing footman/archer: %+v", palette)
	}
	if !hasPalette(palette, ObjectKindDoodad, "kaykit-hexagon/tree_single_A.glb") {
		t.Fatalf("doodad palette missing tree asset: %+v", palette)
	}

	px, py := sourceform.TerrainCellCenterPathingCell(7, 7)
	blockX, blockY := px-1, py-1
	if err := app.world.SetPathingCell(blockX, blockY, 0); err != nil {
		t.Fatal(err)
	}
	beforeReject := app.Snapshot()
	_, err := app.PlaceUnitCell("footman", 3, 7, 7, 0, sourceform.PlacementScaleDefault, false)
	afterReject := app.Snapshot()
	t.Logf("FSV object reject before: units=%d doodads=%d pathing[%d,%d]=%d center=%d,%d status=%q", beforeReject.World.Entities, beforeReject.World.Doodads, blockX, blockY, app.world.Pathing[blockY][blockX], px, py, beforeReject.Status)
	t.Logf("FSV object reject after: units=%d doodads=%d err=%v pathing[%d,%d]=%d center=%d,%d status=%q", afterReject.World.Entities, afterReject.World.Doodads, err, blockX, blockY, app.world.Pathing[blockY][blockX], px, py, afterReject.Status)
	if err == nil || !strings.Contains(err.Error(), "blocked pathing footprint") {
		t.Fatalf("blocked pathing unit placement should reject loudly, got %v", err)
	}
	if afterReject.World.Entities != beforeReject.World.Entities || afterReject.World.Doodads != beforeReject.World.Doodads {
		t.Fatalf("rejected placement changed counts: before=%+v after=%+v", beforeReject.World, afterReject.World)
	}

	baseUndo := app.StackSnapshot().UndoDepth
	u1, err := app.PlaceUnitCell("footman", 0, 1, 2, 1024, 1000, false)
	if err != nil {
		t.Fatal(err)
	}
	if got := app.StackSnapshot().UndoDepth; got != baseUndo+1 {
		t.Fatalf("unit place undo depth=%d want %d", got, baseUndo+1)
	}
	if err := app.TransformEntity(u1.ID, [2]int{2 * editorTerrainCellWorldUnit, 2 * editorTerrainCellWorldUnit}, 8192, 1250); err != nil {
		t.Fatal(err)
	}
	if got := app.StackSnapshot().UndoDepth; got != baseUndo+2 {
		t.Fatalf("unit transform undo depth=%d want %d", got, baseUndo+2)
	}
	u1After, err := entityByID(app.world, u1.ID)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV object unit transform before=%+v after=%+v stack=%s", u1, u1After, stackJSON(t, app.StackSnapshot()))
	if u1After.Pos != ([2]int{2 * editorTerrainCellWorldUnit, 2 * editorTerrainCellWorldUnit}) || u1After.Rotation != 8192 || u1After.Scale != 1250 {
		t.Fatalf("unit transform not exact: %+v", u1After)
	}

	d1, err := app.PlaceDoodadCell("kaykit-hexagon/tree_single_A.glb", 7, 7, 0, 1000)
	if err != nil {
		t.Fatalf("doodad should place on blocked pathing cell: %v", err)
	}
	if err := app.TransformDoodad(d1.ID, d1.Pos, d1.Rotation, -5); err != nil {
		t.Fatal(err)
	}
	d1After, err := doodadByID(app.world, d1.ID)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV object doodad scale clamp before=%+v after=%+v", d1, d1After)
	if d1After.Scale != sourceform.PlacementScaleMin {
		t.Fatalf("negative scale should clamp to %d, got %+v", sourceform.PlacementScaleMin, d1After)
	}

	d2, err := app.PlaceDoodadCell("kaykit-hexagon/rock_single_A.glb", 1, 3, 4096, 750)
	if err != nil {
		t.Fatal(err)
	}
	beforeDelete := d2
	if err := app.DeleteDoodad(d2.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := doodadByID(app.world, d2.ID); err == nil {
		t.Fatal("deleted doodad still present")
	}
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	afterUndo, err := doodadByID(app.world, d2.ID)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV object delete/undo before=%+v afterUndo=%+v stack=%s", beforeDelete, afterUndo, stackJSON(t, app.StackSnapshot()))
	if afterUndo != beforeDelete {
		t.Fatalf("undo did not restore identical doodad: before=%+v after=%+v", beforeDelete, afterUndo)
	}

	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	entities, err := os.ReadFile(filepath.Join(app.projectPath, "map", "entities.toml"))
	if err != nil {
		t.Fatal(err)
	}
	doodads, err := os.ReadFile(filepath.Join(app.projectPath, "map", "doodads.toml"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV saved entities.toml:\n%s", entities)
	t.Logf("FSV saved doodads.toml:\n%s", doodads)
	if !bytes.Contains(entities, []byte(`type = "footman"`)) || !bytes.Contains(entities, []byte(`rotation = 8192, scale = 1250`)) {
		t.Fatalf("saved entities missing transformed unit:\n%s", entities)
	}
	if !bytes.Contains(doodads, []byte(`type = "kaykit-hexagon/tree_single_A.glb"`)) || !bytes.Contains(doodads, []byte(`scale = 1`)) {
		t.Fatalf("saved doodads missing clamped doodad:\n%s", doodads)
	}
}

func TestObjectPlacementFootprintRequiresBuildablePathingFSV(t *testing.T) {
	app := newCommandTestApp(t)
	const towerType = "fsv-tower"
	app.objectPalette = append(app.objectPalette, ObjectPaletteItem{Kind: ObjectKindUnit, Type: towerType, Source: "test"})
	app.unitData[towerType] = data.Unit{ID: towerType, Pathing: data.PathingGround, Footprint: 4}

	px, py := 1*sourceform.PathingScale, 1*sourceform.PathingScale
	beforeClear, err := app.world.PathingFootprintClear(px, py, 4, 4, true)
	if err != nil {
		t.Fatal(err)
	}
	if err := app.world.SetPathingCell(px+3, py+3, sourceform.PathWalkable); err != nil {
		t.Fatal(err)
	}
	afterBlockClear, err := app.world.PathingFootprintClear(px, py, 4, 4, true)
	if err != nil {
		t.Fatal(err)
	}
	beforeReject := app.Snapshot()
	_, err = app.PlaceUnitCell(towerType, 0, 1, 1, 0, sourceform.PlacementScaleDefault, false)
	afterReject := app.Snapshot()
	t.Logf("FSV footprint before: clear=%v units=%d pathing[%d,%d]=%d", beforeClear, beforeReject.World.Entities, px+3, py+3, app.world.Pathing[py+3][px+3])
	t.Logf("FSV footprint blocked: clear=%v err=%v units=%d status=%q", afterBlockClear, err, afterReject.World.Entities, afterReject.Status)
	if !beforeClear || afterBlockClear || err == nil || !strings.Contains(err.Error(), "blocked pathing footprint") || afterReject.World.Entities != beforeReject.World.Entities {
		t.Fatalf("buildable footprint rejection mismatch: beforeClear=%v afterClear=%v err=%v before=%+v after=%+v", beforeClear, afterBlockClear, err, beforeReject.World, afterReject.World)
	}

	if err := app.world.SetPathingCell(px+3, py+3, sourceform.PathWalkable|sourceform.PathBuildable); err != nil {
		t.Fatal(err)
	}
	afterRestoreClear, err := app.world.PathingFootprintClear(px, py, 4, 4, true)
	if err != nil {
		t.Fatal(err)
	}
	ent, err := app.PlaceUnitCell(towerType, 0, 1, 1, 0, sourceform.PlacementScaleDefault, false)
	afterPlace := app.Snapshot()
	t.Logf("FSV footprint restored: clear=%v ent=%+v units=%d", afterRestoreClear, ent, afterPlace.World.Entities)
	if !afterRestoreClear || err != nil || ent.Type != towerType || afterPlace.World.Entities != beforeReject.World.Entities+1 {
		t.Fatalf("restored buildable footprint should place exactly one unit: clear=%v ent=%+v err=%v before=%+v after=%+v", afterRestoreClear, ent, err, beforeReject.World, afterPlace.World)
	}
}

func hasPalette(items []ObjectPaletteItem, kind ObjectKind, typ string) bool {
	for _, item := range items {
		if item.Kind == kind && item.Type == typ {
			return true
		}
	}
	return false
}
