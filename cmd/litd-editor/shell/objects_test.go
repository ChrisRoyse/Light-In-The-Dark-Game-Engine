package shell

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

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
	beforeReject := app.Snapshot()
	_, err := app.PlaceUnitCell("footman", 3, 7, 7, 0, sourceform.PlacementScaleDefault, false)
	afterReject := app.Snapshot()
	t.Logf("FSV object reject before: units=%d doodads=%d status=%q", beforeReject.World.Entities, beforeReject.World.Doodads, beforeReject.Status)
	t.Logf("FSV object reject after: units=%d doodads=%d err=%v status=%q", afterReject.World.Entities, afterReject.World.Doodads, err, afterReject.Status)
	if err == nil || !strings.Contains(err.Error(), "unwalkable cell 7,7") {
		t.Fatalf("unwalkable unit placement should reject loudly, got %v", err)
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
	if u1After.Pos != ([2]int{8192, 8192}) || u1After.Rotation != 8192 || u1After.Scale != 1250 {
		t.Fatalf("unit transform not exact: %+v", u1After)
	}

	d1, err := app.PlaceDoodadCell("kaykit-hexagon/tree_single_A.glb", 0, 3, 0, 1000)
	if err != nil {
		t.Fatal(err)
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

func hasPalette(items []ObjectPaletteItem, kind ObjectKind, typ string) bool {
	for _, item := range items {
		if item.Kind == kind && item.Type == typ {
			return true
		}
	}
	return false
}
