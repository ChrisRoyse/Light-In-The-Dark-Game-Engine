package shell

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestCliffBrushPlateauRampLowerAndPathabilityFSV(t *testing.T) {
	app := newCommandTestApp(t)
	initialHash := cliffStateHash(t, app)
	t.Logf("FSV cliff initial: hash=%s cliff=%s flags=%s stack=%s", initialHash, cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))

	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff plateau after: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	for _, p := range [][2]int{{4, 3}, {3, 4}, {4, 4}, {5, 4}, {4, 5}} {
		if got := app.world.Cliff[p[1]][p[0]]; got != (sourceform.CliffCell{Level: 1}) {
			t.Fatalf("plateau cell %v = %+v, want plain level 1", p, got)
		}
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
	if err := app.ApplyTerrainBrush(3, 4); err != nil {
		t.Fatal(err)
	}
	lowRamp, err := app.CliffStepLegal(2, 4, 3, 4)
	if err != nil {
		t.Fatal(err)
	}
	rampHigh, err := app.CliffStepLegal(3, 4, 4, 4)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff ramp after: cliff=%s low->ramp=%v ramp->high=%v flags=%s stack=%s", cliffJSON(t, app), lowRamp, rampHigh, cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[4][3]; got != (sourceform.CliffCell{Level: 0, Ramp: true}) {
		t.Fatalf("ramp center = %+v, want r0", got)
	}
	if got := app.world.Cliff[4][4]; got != (sourceform.CliffCell{Level: 1}) {
		t.Fatalf("ramp high side = %+v, want level 1", got)
	}
	if !lowRamp || !rampHigh {
		t.Fatalf("ramp pathability = low/ramp %v ramp/high %v, want both true", lowRamp, rampHigh)
	}

	if err := app.SetTerrainBrush(BrushCliffLower); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	afterLowCenter, err := app.CliffStepLegal(2, 4, 3, 4)
	if err != nil {
		t.Fatal(err)
	}
	afterCenterHigh, err := app.CliffStepLegal(3, 4, 4, 4)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff lower after: cliff=%s low->center=%v center->high=%v flags=%s stack=%s", cliffJSON(t, app), afterLowCenter, afterCenterHigh, cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[4][3]; got != (sourceform.CliffCell{Level: 0}) {
		t.Fatalf("invalidated ramp cell = %+v, want plain level 0", got)
	}
	if !hasCliffFlag(app.cliffFlags, CliffFlagRampInvalidated, 3, 4, 0) {
		t.Fatalf("missing ramp invalidation flag: %s", cliffFlagsJSON(t, app))
	}
	if stack := app.StackSnapshot(); stack.UndoDepth != 3 || !strings.HasPrefix(stack.NewestUndo, "stroke:cliff-lower/points=1") {
		t.Fatalf("cliff lower stack = %+v, want third one-stroke undo entry", stack)
	}

	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff lower undo: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[4][3]; got != (sourceform.CliffCell{Level: 0, Ramp: true}) {
		t.Fatalf("undo lower ramp center = %+v, want r0 restored", got)
	}
	if hasCliffFlag(app.cliffFlags, CliffFlagRampInvalidated, 3, 4, 0) {
		t.Fatalf("undo lower should restore pre-flag state, got %s", cliffFlagsJSON(t, app))
	}
	if err := app.Redo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff lower redo: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if !hasCliffFlag(app.cliffFlags, CliffFlagRampInvalidated, 3, 4, 0) {
		t.Fatalf("redo lower missing ramp invalidation flag: %s", cliffFlagsJSON(t, app))
	}
}

func TestCliffBrushPlacementFlagBorderClampAndMaxLevelFSV(t *testing.T) {
	app := newCommandTestApp(t)
	t.Logf("FSV cliff placement/border before: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))

	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(1, 1); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff unit flag after: cliff=%s flags=%s stack=%s entity=%+v", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()), app.world.Entities[0])
	if got := app.world.Cliff[1][1]; got != (sourceform.CliffCell{Level: 1}) {
		t.Fatalf("unit cliff cell = %+v, want level 1", got)
	}
	if !hasCliffFlag(app.cliffFlags, CliffFlagPlacementChangedCliff, 1, 1, 1) {
		t.Fatalf("missing placement flag: %s", cliffFlagsJSON(t, app))
	}
	if got := app.world.Entities[0].Pos; got != ([2]int{editorTerrainCellWorldUnit, editorTerrainCellWorldUnit}) {
		t.Fatalf("cliff edit relocated entity: pos=%v", got)
	}

	beforeBorder := cliffStateJSON(t, app)
	if err := app.SetTerrainBrush(BrushCliffLower); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(2); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(64); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(0, 0); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff border before: %s", beforeBorder)
	t.Logf("FSV cliff border after:  %s flags=%s stack=%s", cliffStateJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	for _, p := range [][2]int{{0, 0}, {1, 0}, {2, 0}, {0, 1}, {1, 1}, {0, 2}} {
		if got := app.world.Cliff[p[1]][p[0]].Level; got != 0 {
			t.Fatalf("border cell %v level = %d, want clamped 0", p, got)
		}
	}
	if got := app.world.Cliff[2][2]; got != (sourceform.CliffCell{}) {
		t.Fatalf("outside circular border footprint [2,2] = %+v, want untouched zero cell", got)
	}
	if !hasCliffFlag(app.cliffFlags, CliffFlagPlacementChangedCliff, 1, 1, 1) {
		t.Fatalf("border edit lost placement flag: %s", cliffFlagsJSON(t, app))
	}
	if stack := app.StackSnapshot(); stack.UndoDepth != 2 || !strings.HasPrefix(stack.NewestUndo, "stroke:cliff-lower/points=1/size=2") {
		t.Fatalf("border stack = %+v, want one cliff-lower undo entry", stack)
	}

	app.SetBrushLevelTarget(999)
	if err := app.SetTerrainBrush(BrushCliffLevel); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(7, 7); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff max clamp after: cliff=%s brush=%+v stack=%s", cliffJSON(t, app), app.BrushSnapshot(), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[7][7].Level; got != maxCliffLevel {
		t.Fatalf("cliff level target 999 wrote %d, want clamp %d", got, maxCliffLevel)
	}
}

func TestCliffRampFlagsPlacementFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetTerrainBrush(BrushRamp); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushRampDirection(RampEast); err != nil {
		t.Fatal(err)
	}
	before := cliffStateJSON(t, app)
	if err := app.ApplyTerrainBrush(1, 1); err != nil {
		t.Fatal(err)
	}
	lowRamp, err := app.CliffStepLegal(0, 1, 1, 1)
	if err != nil {
		t.Fatal(err)
	}
	rampHigh, err := app.CliffStepLegal(1, 1, 2, 1)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff ramp placement before: %s", before)
	t.Logf("FSV cliff ramp placement after: cliff=%s low->ramp=%v ramp->high=%v flags=%s stack=%s entity=%+v", cliffJSON(t, app), lowRamp, rampHigh, cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()), app.world.Entities[0])
	if got := app.world.Cliff[1][1]; got != (sourceform.CliffCell{Level: 0, Ramp: true}) {
		t.Fatalf("unit ramp center = %+v, want r0", got)
	}
	if got := app.world.Cliff[1][2]; got != (sourceform.CliffCell{Level: 1}) {
		t.Fatalf("unit ramp high side = %+v, want level 1", got)
	}
	if !lowRamp || !rampHigh {
		t.Fatalf("unit ramp pathability = low/ramp %v ramp/high %v, want both true", lowRamp, rampHigh)
	}
	if !hasCliffFlag(app.cliffFlags, CliffFlagPlacementChangedCliff, 1, 1, 1) {
		t.Fatalf("missing ramp placement flag: %s", cliffFlagsJSON(t, app))
	}
	if got := app.world.Entities[0].Pos; got != ([2]int{editorTerrainCellWorldUnit, editorTerrainCellWorldUnit}) {
		t.Fatalf("ramp edit relocated entity: pos=%v", got)
	}
}

func TestCliffBrushLiveStrokeFlagsOneUndoFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	if err := app.SetTerrainBrush(BrushRamp); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushRampDirection(RampEast); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(3, 4); err != nil {
		t.Fatal(err)
	}
	beforeLive := cliffStateJSON(t, app)
	if err := app.SetTerrainBrush(BrushCliffLower); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(0); err != nil {
		t.Fatal(err)
	}
	stroke, err := app.BeginTerrainStroke(3, 4)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff live before mouse-up: before=%s after=%s flags=%s stack=%s", beforeLive, cliffStateJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.StackSnapshot().UndoDepth; got != 2 {
		t.Fatalf("live cliff stroke recorded before mouse-up: undo depth=%d, want setup depth 2", got)
	}
	if !hasCliffFlag(app.cliffFlags, CliffFlagRampInvalidated, 3, 4, 0) {
		t.Fatalf("live cliff stroke missing immediate marker: %s", cliffFlagsJSON(t, app))
	}
	centerHigh, err := app.CliffStepLegal(3, 4, 4, 4)
	if err != nil {
		t.Fatal(err)
	}
	if centerHigh {
		t.Fatal("lowering the ramp cell should remove the ramp span and block the high-side step")
	}
	if err := stroke.End(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff live after mouse-up: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if stack := app.StackSnapshot(); stack.UndoDepth != 3 || !strings.HasPrefix(stack.NewestUndo, "stroke:cliff-lower/points=1") {
		t.Fatalf("live cliff stroke stack = %+v, want one coalesced cliff-lower entry", stack)
	}
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV cliff live after undo: cliff=%s flags=%s stack=%s", cliffJSON(t, app), cliffFlagsJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if got := app.world.Cliff[4][3]; got != (sourceform.CliffCell{Level: 0, Ramp: true}) {
		t.Fatalf("live undo ramp center = %+v, want r0 restored", got)
	}
	if hasCliffFlag(app.cliffFlags, CliffFlagRampInvalidated, 3, 4, 0) {
		t.Fatalf("live undo should restore pre-flag state, got %s", cliffFlagsJSON(t, app))
	}
}

func TestCliffBrushSaveWritesCliffSourceFormFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
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
	if err := app.ApplyTerrainBrush(3, 4); err != nil {
		t.Fatal(err)
	}
	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(app.projectPath, "map", "cliff.txt"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV saved cliff.txt:\n%s", body)
	if !strings.Contains(string(body), "0 0 0 r0 1 1 0 0") {
		t.Fatalf("saved cliff.txt missing plateau/ramp row:\n%s", body)
	}
}

type cliffStateDump struct {
	Cliff [][]string          `json:"cliff"`
	Flags []CliffFlagSnapshot `json:"flags"`
}

func cliffStateJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(cliffStateDump{Cliff: cliffGridStrings(app.world.Cliff), Flags: app.cliffFlags})
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func cliffStateHash(t *testing.T, app *App) string {
	t.Helper()
	sum := sha256.Sum256([]byte(cliffStateJSON(t, app)))
	return hex.EncodeToString(sum[:])
}

func cliffFlagsJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(app.cliffFlags)
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func hasCliffFlag(flags []CliffFlagSnapshot, kind CliffFlagKind, x, y int, entityID uint32) bool {
	for _, flag := range flags {
		if flag.Kind == kind && flag.X == x && flag.Y == y && flag.EntityID == entityID {
			return true
		}
	}
	return false
}
