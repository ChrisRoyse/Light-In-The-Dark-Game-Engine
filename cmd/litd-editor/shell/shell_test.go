package shell

import (
	"image/png"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

func TestModeSwitchPreservesDirtyStateFSV(t *testing.T) {
	app := newTestApp(t)
	dir := filepath.Join(t.TempDir(), "world")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 7); err != nil {
		t.Fatal(err)
	}
	before := app.Snapshot()
	if !before.Dirty || before.World.HeightCell != 7 {
		t.Fatalf("before switch dirty/cell = %v/%d, want true/7", before.Dirty, before.World.HeightCell)
	}
	if err := app.SwitchMode(ModeObjects); err != nil {
		t.Fatal(err)
	}
	objects := app.Snapshot()
	if err := app.SwitchMode(ModeMetadata); err != nil {
		t.Fatal(err)
	}
	after := app.Snapshot()
	t.Logf("FSV mode switch before: mode=%s dirty=%v cell=%d title=%q", before.Mode, before.Dirty, before.World.HeightCell, before.Title)
	t.Logf("FSV mode switch objects: mode=%s dirty=%v cell=%d", objects.Mode, objects.Dirty, objects.World.HeightCell)
	t.Logf("FSV mode switch after: mode=%s dirty=%v cell=%d title=%q", after.Mode, after.Dirty, after.World.HeightCell, after.Title)
	if !after.Dirty || after.World.HeightCell != 7 || after.Mode != ModeMetadata {
		t.Fatalf("mode switch lost dirty edit: %+v", after)
	}
}

func TestOpenMissingProjectShowsErrorAndShellAliveFSV(t *testing.T) {
	app := newTestApp(t)
	dir := filepath.Join(t.TempDir(), "world")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	before := app.Snapshot()
	err := app.OpenProject(filepath.Join(t.TempDir(), "missing.litdworld"))
	after := app.Snapshot()
	t.Logf("FSV missing open before: path=%s mode=%s err=%q", before.ProjectPath, before.Mode, before.Error)
	t.Logf("FSV missing open after: path=%s mode=%s err=%q", after.ProjectPath, after.Mode, after.Error)
	if err == nil || after.Error == "" {
		t.Fatal("missing project should return and expose an error")
	}
	if after.ProjectPath != before.ProjectPath || after.Mode != before.Mode {
		t.Fatalf("shell did not stay on current project/mode: before=%+v after=%+v", before, after)
	}
}

func TestNewWhileDirtyCancelPreservesStateFSV(t *testing.T) {
	app := newTestApp(t)
	root := t.TempDir()
	if err := app.NewProject(filepath.Join(root, "world-a")); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 7); err != nil {
		t.Fatal(err)
	}
	if err := app.NewProject(filepath.Join(root, "world-b")); err != nil {
		t.Fatal(err)
	}
	prompt := app.Snapshot()
	app.CancelConfirm()
	after := app.Snapshot()
	t.Logf("FSV new-dirty prompt: confirm=%+v path=%s dirty=%v", prompt.Confirm, prompt.ProjectPath, prompt.Dirty)
	t.Logf("FSV new-dirty cancel: confirm=%+v path=%s dirty=%v cell=%d", after.Confirm, after.ProjectPath, after.Dirty, after.World.HeightCell)
	if prompt.Confirm == nil {
		t.Fatal("dirty new project should show confirm prompt")
	}
	if after.Confirm != nil || !after.Dirty || after.World.HeightCell != 7 || !strings.HasSuffix(after.ProjectPath, "world-a") {
		t.Fatalf("cancel did not preserve dirty state: %+v", after)
	}
}

func TestRenderPNGScreenshotsFSV(t *testing.T) {
	app := newTestApp(t)
	dir := filepath.Join(t.TempDir(), "world")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 7); err != nil {
		t.Fatal(err)
	}
	for _, mode := range Modes() {
		if err := app.SwitchMode(mode); err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(t.TempDir(), string(mode)+".png")
		if err := RenderPNG(path, app.Snapshot()); err != nil {
			t.Fatalf("render %s: %v", mode, err)
		}
		f, err := os.Open(path)
		if err != nil {
			t.Fatal(err)
		}
		img, err := png.Decode(f)
		f.Close()
		if err != nil {
			t.Fatal(err)
		}
		b := img.Bounds()
		if b.Dx() != ShotWidth || b.Dy() != ShotHeight {
			t.Fatalf("shot %s size %dx%d, want %dx%d", path, b.Dx(), b.Dy(), ShotWidth, ShotHeight)
		}
		t.Logf("FSV screenshot %s: size=%dx%d mode=%s dirty=%v", path, b.Dx(), b.Dy(), mode, app.Snapshot().Dirty)
	}
}

func TestEditorCommandImportGraphFSV(t *testing.T) {
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	cmdDir := filepath.Clean(filepath.Join(filepath.Dir(file), ".."))
	cmd := exec.Command("go", "list", "-deps", ".")
	cmd.Dir = cmdDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("go list deps: %v\n%s", err, out)
	}
	deps := string(out)
	for _, banned := range []string{
		"/litd/sim",
		"/litd/render",
		"/litd/worldhost",
		"/litd/luabind",
	} {
		if strings.Contains(deps, banned) {
			t.Fatalf("editor imports private engine dependency %s\n%s", banned, deps)
		}
	}
	t.Logf("FSV import graph deps audited for cmd/litd-editor; banned private engine deps absent")
}

func newTestApp(t *testing.T) *App {
	t.Helper()
	table, err := locale.Load(os.DirFS(repoRoot(t, 3, "data")), "en")
	if err != nil {
		t.Fatal(err)
	}
	return New(table)
}

func repoRoot(t *testing.T, ups int, parts ...string) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dir := filepath.Dir(file)
	for i := 0; i < ups; i++ {
		dir = filepath.Dir(dir)
	}
	all := append([]string{dir}, parts...)
	return filepath.Join(all...)
}
