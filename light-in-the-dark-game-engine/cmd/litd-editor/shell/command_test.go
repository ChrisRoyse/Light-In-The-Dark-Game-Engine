package shell

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestCommandStackUndoRedoFSV(t *testing.T) {
	app := newCommandTestApp(t)
	initialHash := editorStateHash(t, app)
	initialStack := stackJSON(t, app.StackSnapshot())
	t.Logf("FSV command stack initial: hash=%s stack=%s state=%s", initialHash, initialStack, editorStateJSON(t, app))

	if err := app.EditTerrainHeight(1, 1, 7); err != nil {
		t.Fatal(err)
	}
	if err := app.MoveEntity(1, [2]int{2048, 3072}, 90); err != nil {
		t.Fatal(err)
	}
	if err := app.SetMetadataName("Undo FSV World"); err != nil {
		t.Fatal(err)
	}
	postEditHash := editorStateHash(t, app)
	t.Logf("FSV command stack post-edit: hash=%s stack=%s state=%s", postEditHash, stackJSON(t, app.StackSnapshot()), editorStateJSON(t, app))
	if postEditHash == initialHash {
		t.Fatal("post-edit hash should differ from initial hash")
	}
	if snap := app.StackSnapshot(); snap.UndoDepth != 3 || snap.RedoDepth != 0 {
		t.Fatalf("post-edit stack depths = undo %d redo %d, want 3/0", snap.UndoDepth, snap.RedoDepth)
	}

	for i := 0; i < 3; i++ {
		if err := app.Undo(); err != nil {
			t.Fatalf("undo %d: %v", i+1, err)
		}
		t.Logf("FSV command stack after undo %d: hash=%s stack=%s state=%s", i+1, editorStateHash(t, app), stackJSON(t, app.StackSnapshot()), editorStateJSON(t, app))
	}
	afterUndoHash := editorStateHash(t, app)
	if afterUndoHash != initialHash {
		t.Fatalf("after undo hash %s, want initial %s", afterUndoHash, initialHash)
	}
	if snap := app.StackSnapshot(); snap.UndoDepth != 0 || snap.RedoDepth != 3 {
		t.Fatalf("after undo stack depths = undo %d redo %d, want 0/3", snap.UndoDepth, snap.RedoDepth)
	}

	for i := 0; i < 3; i++ {
		if err := app.Redo(); err != nil {
			t.Fatalf("redo %d: %v", i+1, err)
		}
		t.Logf("FSV command stack after redo %d: hash=%s stack=%s state=%s", i+1, editorStateHash(t, app), stackJSON(t, app.StackSnapshot()), editorStateJSON(t, app))
	}
	if got := editorStateHash(t, app); got != postEditHash {
		t.Fatalf("after redo hash %s, want post-edit %s", got, postEditHash)
	}
}

func TestCommandStackEmptyUndoRedoFSV(t *testing.T) {
	app := newCommandTestApp(t)
	beforeHash := editorStateHash(t, app)
	beforeStack := stackJSON(t, app.StackSnapshot())
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	if err := app.Redo(); err != nil {
		t.Fatal(err)
	}
	afterHash := editorStateHash(t, app)
	afterStack := stackJSON(t, app.StackSnapshot())
	t.Logf("FSV empty undo/redo before: hash=%s stack=%s", beforeHash, beforeStack)
	t.Logf("FSV empty undo/redo after: hash=%s stack=%s", afterHash, afterStack)
	if afterHash != beforeHash || afterStack != beforeStack {
		t.Fatalf("empty undo/redo changed state: before hash=%s stack=%s after hash=%s stack=%s", beforeHash, beforeStack, afterHash, afterStack)
	}
}

func TestCommandStackOverflowDropsOldestFSV(t *testing.T) {
	app := newCommandTestApp(t)
	for value := 1; value <= CommandStackLimit+1; value++ {
		if err := app.EditTerrainHeight(0, 0, value); err != nil {
			t.Fatalf("edit %d: %v", value, err)
		}
	}
	overflow := app.StackSnapshot()
	t.Logf("FSV overflow after 257 edits: cell=%d stack=%s", app.world.Height[0][0], stackJSON(t, overflow))
	if overflow.UndoDepth != CommandStackLimit {
		t.Fatalf("undo depth after overflow = %d, want %d", overflow.UndoDepth, CommandStackLimit)
	}
	if overflow.OldestUndo != "height[0,0]:1->2" {
		t.Fatalf("oldest undo = %q, want first retained command height[0,0]:1->2", overflow.OldestUndo)
	}
	for i := 0; i < CommandStackLimit; i++ {
		if err := app.Undo(); err != nil {
			t.Fatalf("undo %d: %v", i+1, err)
		}
	}
	after := app.StackSnapshot()
	t.Logf("FSV overflow after undo retained commands: cell=%d stack=%s", app.world.Height[0][0], stackJSON(t, after))
	if got := app.world.Height[0][0]; got != 1 {
		t.Fatalf("height after undoing retained commands = %d, want 1 because oldest 0->1 was dropped", got)
	}
	if after.RedoDepth != CommandStackLimit || after.UndoDepth != 0 {
		t.Fatalf("after overflow undo stack depths = undo %d redo %d, want 0/%d", after.UndoDepth, after.RedoDepth, CommandStackLimit)
	}
}

func TestCommandStackEditAfterUndoClearsRedoFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.EditTerrainHeight(1, 1, 4); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 8); err != nil {
		t.Fatal(err)
	}
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	before := app.StackSnapshot()
	t.Logf("FSV redo invalidation before new edit: cell=%d stack=%s", app.world.Height[1][1], stackJSON(t, before))
	if before.RedoDepth != 1 {
		t.Fatalf("redo depth before new edit = %d, want 1", before.RedoDepth)
	}
	if err := app.EditTerrainHeight(1, 1, 3); err != nil {
		t.Fatal(err)
	}
	after := app.StackSnapshot()
	t.Logf("FSV redo invalidation after new edit: cell=%d stack=%s", app.world.Height[1][1], stackJSON(t, after))
	if after.RedoDepth != 0 {
		t.Fatalf("redo depth after new edit = %d, want 0", after.RedoDepth)
	}
	if got := app.world.Height[1][1]; got != 3 {
		t.Fatalf("height after new edit = %d, want 3", got)
	}
}

func TestCommandStackCompositeStrokeCoalescesFSV(t *testing.T) {
	app := newCommandTestApp(t)
	stroke, err := NewCompositeCommand("stroke:height-two-cells", []Command{
		gridCellCommand{kind: sourceform.GridHeight, x: 2, y: 2, before: 0, after: 5},
		gridCellCommand{kind: sourceform.GridHeight, x: 3, y: 2, before: 0, after: 6},
	})
	if err != nil {
		t.Fatal(err)
	}
	beforeHash := editorStateHash(t, app)
	if err := app.executeCommand(stroke); err != nil {
		t.Fatal(err)
	}
	afterStroke := app.StackSnapshot()
	t.Logf("FSV composite stroke after apply: cells=(%d,%d) hash=%s stack=%s", app.world.Height[2][2], app.world.Height[2][3], editorStateHash(t, app), stackJSON(t, afterStroke))
	if afterStroke.UndoDepth != 1 || afterStroke.NewestUndo != "stroke:height-two-cells" {
		t.Fatalf("composite stroke stack = %+v, want one undo entry", afterStroke)
	}
	if app.world.Height[2][2] != 5 || app.world.Height[2][3] != 6 {
		t.Fatalf("composite stroke did not apply both cells: row=%v", app.world.Height[2])
	}
	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV composite stroke after undo: cells=(%d,%d) hash=%s stack=%s", app.world.Height[2][2], app.world.Height[2][3], editorStateHash(t, app), stackJSON(t, app.StackSnapshot()))
	if got := editorStateHash(t, app); got != beforeHash {
		t.Fatalf("composite stroke undo hash %s, want before %s", got, beforeHash)
	}
}

func newCommandTestApp(t *testing.T) *App {
	t.Helper()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(t.TempDir(), "world")); err != nil {
		t.Fatal(err)
	}
	return app
}

type editorStateDump struct {
	Name     string              `json:"name"`
	Height   [][]int             `json:"height"`
	Entities []sourceform.Entity `json:"entities"`
}

func editorStateJSON(t *testing.T, app *App) string {
	t.Helper()
	body, err := json.Marshal(editorStateDump{
		Name:     app.world.Metadata.Name,
		Height:   app.world.Height,
		Entities: app.world.Entities,
	})
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func editorStateHash(t *testing.T, app *App) string {
	t.Helper()
	sum := sha256.Sum256([]byte(editorStateJSON(t, app)))
	return hex.EncodeToString(sum[:])
}

func stackJSON(t *testing.T, snap StackSnapshot) string {
	t.Helper()
	body, err := json.Marshal(snap)
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}
