package shell

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestPaintBrushOverlappingLayersAndSimHashFSV(t *testing.T) {
	app := newCommandTestApp(t)
	beforeHash := app.SimRelevantHash()
	t.Logf("FSV paint initial: simHash=%s splat=%s stack=%s", beforeHash, splatJSON(t, app), stackJSON(t, app.StackSnapshot()))

	if err := app.SetPaintLayer(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintStrength(255); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyPaintBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV paint layer B after: simHash=%s splat=%s stack=%s", app.SimRelevantHash(), splatJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Splat[2][2]; got != (sourceform.SplatWeight{B: 255}) {
		t.Fatalf("paint B cell = %+v, want full B", got)
	}

	if err := app.SetPaintLayer(2); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyPaintBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	afterHash := app.SimRelevantHash()
	t.Logf("FSV paint layer C overlap after: simHash=%s splat=%s stack=%s", afterHash, splatJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Splat[2][2]; got != (sourceform.SplatWeight{C: 255}) {
		t.Fatalf("paint C overlap cell = %+v, want full C", got)
	}
	if afterHash != beforeHash {
		t.Fatalf("paint changed sim-relevant hash: before=%s after=%s", beforeHash, afterHash)
	}
	if stack := app.StackSnapshot(); stack.UndoDepth != 2 || !strings.HasPrefix(stack.NewestUndo, "paint:layer=2/points=1") {
		t.Fatalf("paint stack = %+v, want one undo per stroke", stack)
	}
}

func TestPaintBrushBoundaryBorderAndNormalizationFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetPaintLayer(3); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintStrength(255); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintSize(1); err != nil {
		t.Fatal(err)
	}
	beforeBoundary := splatJSON(t, app)
	if err := app.ApplyPaintBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV paint boundary before: %s", beforeBoundary)
	t.Logf("FSV paint boundary after:  %s", splatJSON(t, app))
	for _, p := range [][2]int{{4, 3}, {3, 4}, {4, 4}, {5, 4}, {4, 5}} {
		if got := app.world.Splat[p[1]][p[0]]; got != (sourceform.SplatWeight{D: 255}) {
			t.Fatalf("boundary paint cell %v = %+v, want full D", p, got)
		}
	}

	if err := app.SetPaintLayer(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintSize(2); err != nil {
		t.Fatal(err)
	}
	beforeBorder := splatJSON(t, app)
	if err := app.ApplyPaintBrush(0, 0); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV paint border before: %s", beforeBorder)
	t.Logf("FSV paint border after:  %s", splatJSON(t, app))
	for _, p := range [][2]int{{0, 0}, {1, 0}, {2, 0}, {0, 1}, {1, 1}, {0, 2}} {
		if got := app.world.Splat[p[1]][p[0]]; got != (sourceform.SplatWeight{B: 255}) {
			t.Fatalf("border paint cell %v = %+v, want full B", p, got)
		}
	}
	if got := app.world.Splat[2][2]; got != (sourceform.SplatWeight{A: 255}) {
		t.Fatalf("outside circular border footprint cell [2,2] = %+v, want default A", got)
	}

	mixed := sourceform.SplatWeight{A: 100, B: 80, C: 75}
	painted, err := paintSplatWeight(mixed, 3, 128)
	if err != nil {
		t.Fatal(err)
	}
	sum := int(painted.A) + int(painted.B) + int(painted.C) + int(painted.D)
	t.Logf("FSV paint normalization: before=%+v after=%+v sum=%d", mixed, painted, sum)
	if sum != 255 || painted.D == 0 {
		t.Fatalf("partial paint should normalize and add target layer: %+v sum=%d", painted, sum)
	}
}

func TestPaintBrushSaveWritesSplatSourceFormFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetPaintLayer(2); err != nil {
		t.Fatal(err)
	}
	if err := app.SetPaintStrength(255); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyPaintBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(app.projectPath, "map", "splat.txt"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV saved splat.txt:\n%s", body)
	if !strings.Contains(string(body), "255,0,0,0 255,0,0,0 0,0,255,0 255,0,0,0") {
		t.Fatalf("saved splat.txt missing painted C cell:\n%s", body)
	}
}

func splatJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(splatGridWeights(app.world.Splat))
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}
