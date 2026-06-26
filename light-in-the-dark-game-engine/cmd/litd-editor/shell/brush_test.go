package shell

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestTerrainBrushesHappyPathFSV(t *testing.T) {
	app := newCommandTestApp(t)
	t.Logf("FSV brushes initial: height=%s cliff=%s stack=%s", heightJSON(t, app), cliffJSON(t, app), stackJSON(t, app.StackSnapshot()))

	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(2); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV raise after: height=%s brush=%+v stack=%s", heightJSON(t, app), app.BrushSnapshot(), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Height[2][2]; got != 2 {
		t.Fatalf("raise height = %d, want 2", got)
	}

	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushLower); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV lower after: height=%s stack=%s", heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Height[2][2]; got != 1 {
		t.Fatalf("lower height = %d, want 1", got)
	}

	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	app.SetBrushLevelTarget(5)
	if err := app.SetTerrainBrush(BrushLevel); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	minH, maxH := footprintMinMax(t, app, 4, 4, 1)
	t.Logf("FSV level after: footprint min=%d max=%d height=%s stack=%s", minH, maxH, heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if minH != 5 || maxH != 5 {
		t.Fatalf("level footprint min/max = %d/%d, want 5/5", minH, maxH)
	}

	if err := app.SetTerrainBrush(BrushRamp); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushRampDirection(RampEast); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 6); err != nil {
		t.Fatal(err)
	}
	lowRamp, err := app.world.CliffStepLegal(3, 6, 4, 6)
	if err != nil {
		t.Fatal(err)
	}
	rampHigh, err := app.world.CliffStepLegal(4, 6, 5, 6)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV ramp after: cliff=%s low->ramp=%v ramp->high=%v stack=%s", cliffJSON(t, app), lowRamp, rampHigh, stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[6][4]; got != (sourceform.CliffCell{Level: 0, Ramp: true}) {
		t.Fatalf("ramp center = %+v, want r0", got)
	}
	if got := app.world.Cliff[6][5]; got.Level != 1 || got.Ramp {
		t.Fatalf("ramp high side = %+v, want plain level 1", got)
	}
	if !lowRamp || !rampHigh {
		t.Fatal("ramp pathability should be legal through low/ramp/high cells")
	}
	if snap := app.StackSnapshot(); snap.UndoDepth != 4 || snap.RedoDepth != 0 {
		t.Fatalf("brush stack depths = undo %d redo %d, want 4/0", snap.UndoDepth, snap.RedoDepth)
	}
}

func TestTerrainBrushBorderLevelInvalidRampAndCoalescingFSV(t *testing.T) {
	app := newCommandTestApp(t)

	if err := app.SetTerrainBrush(BrushRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(2); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(3); err != nil {
		t.Fatal(err)
	}
	beforeBorder := heightJSON(t, app)
	if err := app.ApplyTerrainBrush(0, 0); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV border before: %s", beforeBorder)
	t.Logf("FSV border after:  %s", heightJSON(t, app))
	for _, p := range [][2]int{{0, 0}, {1, 0}, {2, 0}, {0, 1}, {1, 1}, {0, 2}} {
		if got := app.world.Height[p[1]][p[0]]; got != 3 {
			t.Fatalf("border cell %v = %d, want 3", p, got)
		}
	}
	if got := app.world.Height[2][2]; got != 0 {
		t.Fatalf("outside circular border footprint cell [2,2] = %d, want 0", got)
	}

	if err := app.EditTerrainHeight(5, 5, 1); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(6, 5, 9); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushLevel); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	app.SetBrushLevelTarget(4)
	beforeMin, beforeMax := footprintMinMax(t, app, 5, 5, 1)
	if err := app.ApplyTerrainBrush(5, 5); err != nil {
		t.Fatal(err)
	}
	afterMin, afterMax := footprintMinMax(t, app, 5, 5, 1)
	t.Logf("FSV mixed level before min/max=%d/%d after min/max=%d/%d height=%s", beforeMin, beforeMax, afterMin, afterMax, heightJSON(t, app))
	if beforeMin == beforeMax || afterMin != 4 || afterMax != 4 {
		t.Fatalf("level mixed heights min/max before=%d/%d after=%d/%d, want mixed then 4/4", beforeMin, beforeMax, afterMin, afterMax)
	}

	highBefore, err := cliffCellValue(app.world, 3, 3)
	if err != nil {
		t.Fatal(err)
	}
	if err := app.executeCommand(cliffCellCommand{x: 3, y: 3, before: highBefore, after: sourceform.CliffCell{Level: 2}}); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushRamp); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushRampDirection(RampEast); err != nil {
		t.Fatal(err)
	}
	beforeCliff := cliffJSON(t, app)
	err = app.ApplyTerrainBrush(2, 3)
	afterCliff := cliffJSON(t, app)
	t.Logf("FSV invalid ramp before: cliff=%s", beforeCliff)
	t.Logf("FSV invalid ramp after:  cliff=%s err=%v", afterCliff, err)
	if err == nil || !strings.Contains(err.Error(), "differ by 2") {
		t.Fatalf("non-adjacent ramp should reject, got %v", err)
	}
	if afterCliff != beforeCliff {
		t.Fatalf("invalid ramp changed source of truth: before=%s after=%s", beforeCliff, afterCliff)
	}

	coalesce := newCommandTestApp(t)
	initialHash := editorStateHash(t, coalesce)
	if err := coalesce.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := coalesce.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := coalesce.ApplyTerrainStroke([][2]int{{2, 2}, {3, 2}, {4, 2}}); err != nil {
		t.Fatal(err)
	}
	stack := coalesce.StackSnapshot()
	t.Logf("FSV coalesced stroke after apply: hash=%s height=%s stack=%s", editorStateHash(t, coalesce), heightJSON(t, coalesce), stackJSON(t, stack))
	if stack.UndoDepth != 1 || !strings.HasPrefix(stack.NewestUndo, "stroke:raise/points=3") {
		t.Fatalf("coalesced stroke stack = %+v, want one stroke entry", stack)
	}
	if err := coalesce.Undo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV coalesced stroke after undo: hash=%s height=%s stack=%s", editorStateHash(t, coalesce), heightJSON(t, coalesce), stackJSON(t, coalesce.StackSnapshot()))
	if got := editorStateHash(t, coalesce); got != initialHash {
		t.Fatalf("coalesced stroke undo hash %s, want initial %s", got, initialHash)
	}
}

func TestTerrainBrushLiveDragUpdatesGridAndRecordsOneUndoFSV(t *testing.T) {
	app := newCommandTestApp(t)
	initialHash := editorStateHash(t, app)
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV live drag before: height=%s stack=%s", heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	stroke, err := app.BeginTerrainStroke(2, 2)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV live drag after mouse-down: height=%s stack=%s", heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Height[2][2]; got != 1 {
		t.Fatalf("live mouse-down height[2][2] = %d, want 1", got)
	}
	if stack := app.StackSnapshot(); stack.UndoDepth != 0 || stack.RedoDepth != 0 {
		t.Fatalf("live stroke should not record undo until mouse-up, got %+v", stack)
	}
	if err := stroke.AddPoint(3, 2); err != nil {
		t.Fatal(err)
	}
	if err := stroke.AddPoint(4, 2); err != nil {
		t.Fatal(err)
	}
	if err := stroke.AddPoint(4, 2); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV live drag before mouse-up: height=%s stack=%s", heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	for _, x := range []int{2, 3, 4} {
		if got := app.world.Height[2][x]; got != 1 {
			t.Fatalf("live drag height[2][%d] = %d, want 1", x, got)
		}
	}
	if err := stroke.End(); err != nil {
		t.Fatal(err)
	}
	stack := app.StackSnapshot()
	t.Logf("FSV live drag after mouse-up: height=%s stack=%s", heightJSON(t, app), stackJSON(t, stack))
	if stack.UndoDepth != 1 || !strings.HasPrefix(stack.NewestUndo, "stroke:raise/points=3") {
		t.Fatalf("live drag stack = %+v, want one coalesced stroke", stack)
	}
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV live drag after undo: hash=%s height=%s stack=%s", editorStateHash(t, app), heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := editorStateHash(t, app); got != initialHash {
		t.Fatalf("live drag undo hash %s, want initial %s", got, initialHash)
	}
	if err := app.Redo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV live drag after redo: height=%s stack=%s", heightJSON(t, app), stackJSON(t, app.StackSnapshot()))
	for _, x := range []int{2, 3, 4} {
		if got := app.world.Height[2][x]; got != 1 {
			t.Fatalf("live drag redo height[2][%d] = %d, want 1", x, got)
		}
	}
}

func TestTerrainBrushScreenshotsFSV(t *testing.T) {
	app := newCommandTestApp(t)
	dir := t.TempDir()
	steps := []struct {
		name string
		op   BrushOp
		do   func() error
	}{
		{name: "raise", op: BrushRaise, do: func() error {
			if err := app.SetBrushSize(0); err != nil {
				return err
			}
			if err := app.SetBrushStrength(2); err != nil {
				return err
			}
			return app.ApplyTerrainBrush(2, 2)
		}},
		{name: "lower", op: BrushLower, do: func() error {
			if err := app.SetTerrainBrush(BrushLower); err != nil {
				return err
			}
			if err := app.SetBrushStrength(1); err != nil {
				return err
			}
			return app.ApplyTerrainBrush(2, 2)
		}},
		{name: "level", op: BrushLevel, do: func() error {
			if err := app.SetTerrainBrush(BrushLevel); err != nil {
				return err
			}
			if err := app.SetBrushSize(1); err != nil {
				return err
			}
			app.SetBrushLevelTarget(5)
			return app.ApplyTerrainBrush(4, 4)
		}},
		{name: "ramp", op: BrushRamp, do: func() error {
			if err := app.SetTerrainBrush(BrushRamp); err != nil {
				return err
			}
			if err := app.SetBrushRampDirection(RampEast); err != nil {
				return err
			}
			return app.ApplyTerrainBrush(4, 6)
		}},
	}
	for i, step := range steps {
		if err := step.do(); err != nil {
			t.Fatalf("%s brush: %v", step.name, err)
		}
		path := filepath.Join(dir, step.name+".png")
		if err := RenderPNG(path, app.Snapshot()); err != nil {
			t.Fatal(err)
		}
		t.Logf("FSV brush screenshot %d %s: path=%s height=%s cliff=%s brush=%+v", i+1, step.name, path, heightJSON(t, app), cliffJSON(t, app), app.BrushSnapshot())
	}
}

func TestTerrainBrushSaveWritesSourceFormFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(3); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(2, 2); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushRamp); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushRampDirection(RampEast); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 6); err != nil {
		t.Fatal(err)
	}
	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	height, err := os.ReadFile(filepath.Join(app.projectPath, "map", "height.txt"))
	if err != nil {
		t.Fatal(err)
	}
	cliff, err := os.ReadFile(filepath.Join(app.projectPath, "map", "cliff.txt"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV saved height.txt:\n%s", height)
	t.Logf("FSV saved cliff.txt:\n%s", cliff)
	if !strings.Contains(string(height), "0 0 3 0 0 0 0 0") {
		t.Fatalf("saved height.txt missing raised cell:\n%s", height)
	}
	if !strings.Contains(string(cliff), "0 0 0 0 r0 1 0 0") {
		t.Fatalf("saved cliff.txt missing ramp row:\n%s", cliff)
	}
}

func heightJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(app.world.Height)
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func cliffJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(cliffGridStrings(app.world.Cliff))
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func footprintMinMax(t *testing.T, app *App, x, y, size int) (int, int) {
	t.Helper()
	points, err := brushFootprint(app.world, x, y, size)
	if err != nil {
		t.Fatal(err)
	}
	minV, maxV := app.world.Height[points[0][1]][points[0][0]], app.world.Height[points[0][1]][points[0][0]]
	for _, p := range points[1:] {
		v := app.world.Height[p[1]][p[0]]
		if v < minV {
			minV = v
		}
		if v > maxV {
			maxV = v
		}
	}
	return minV, maxV
}
