package shell

import (
	"encoding/json"
	"image/png"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
)

func TestInstallPlayableRuntimePersistsSourceAndArchiveFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if _, err := app.PlaceUnitCell("footman", 0, 1, 2, 0, 1000, false); err != nil {
		t.Fatal(err)
	}
	placementPath := filepath.Join(app.projectPath, "data", "placement", "editor.toml")
	if _, err := os.Stat(placementPath); !os.IsNotExist(err) {
		t.Fatalf("FSV before install: placement source should not exist yet, stat err=%v", err)
	}
	if err := app.InstallPlayableRuntime(); err != nil {
		t.Fatal(err)
	}
	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	placement, err := os.ReadFile(placementPath)
	if err != nil {
		t.Fatal(err)
	}
	units, err := os.ReadFile(filepath.Join(app.projectPath, "data", "units", "editor.toml"))
	if err != nil {
		t.Fatal(err)
	}
	script, err := os.ReadFile(filepath.Join(app.projectPath, "scripts", "main.lua"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(placement), "type = \"footman\"") || !strings.Contains(string(placement), "owner = 0") {
		t.Fatalf("FSV after install: placement TOML missing unit row:\n%s", placement)
	}
	if !strings.Contains(string(units), "sight-day = 900") || !strings.Contains(string(units), "sight-night = 900") {
		t.Fatalf("FSV after install: unit TOML missing sight radii needed by mapped-world acquisition:\n%s", units)
	}
	if string(script) != playtestMainLua {
		t.Fatalf("FSV after install: scripts/main.lua=%q", script)
	}

	archive := filepath.Join(t.TempDir(), "playable-runtime.litdworld")
	if err := app.SaveArchive(archive); err != nil {
		t.Fatal(err)
	}
	opened, err := worldarchive.Open(archive, EditorEngineVersion())
	if err != nil {
		t.Fatal(err)
	}
	manifestFiles := make([]string, 0, len(opened.Manifest.Files))
	for rel := range opened.Manifest.Files {
		manifestFiles = append(manifestFiles, rel)
	}
	opened.Close()
	for _, rel := range []string{
		"data/combat/damage-table.toml",
		"data/units/editor.toml",
		"data/placement/editor.toml",
		"data/maps/untitled-world/terrain.toml",
		"data/maps/untitled-world/pathing.txt",
		"scripts/main.lua",
	} {
		if !containsString(manifestFiles, rel) {
			t.Fatalf("FSV archive manifest missing %s in %v", rel, manifestFiles)
		}
	}
	if containsString(manifestFiles, "main.lua") {
		t.Fatalf("FSV archive should carry source-form scripts/main.lua, not root main.lua: %v", manifestFiles)
	}
	t.Logf("FSV playable runtime: source placement=%q units=%q script=%q archive=%s manifest=%v", string(placement), string(units), string(script), archive, manifestFiles)
}

func TestInstallPlayableRuntimeRefusesNoProjectFSV(t *testing.T) {
	app := newTestApp(t)
	err := app.InstallPlayableRuntime()
	t.Logf("FSV playable runtime no-project: err=%v status=%q", err, app.Snapshot().Status)
	if err == nil || !strings.Contains(err.Error(), "no project") {
		t.Fatalf("InstallPlayableRuntime without a project should fail closed, got %v", err)
	}
}

func TestEditorPlaytestRoundTripPreservesDirtyStateFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if _, err := app.PlaceUnitCell("archer", 1, 2, 2, 8192, 1250, false); err != nil {
		t.Fatal(err)
	}
	beforeHash := app.SimRelevantHash()
	before := app.Snapshot()
	root := t.TempDir()
	rec, err := app.Playtest(playtestTestOptions(t, root, filepath.Join(root, "playtest.png"), 80, 0))
	if err != nil {
		t.Fatalf("playtest: %v\nstdout=%s\nstderr=%s", err, rec.Stdout, rec.Stderr)
	}
	after := app.Snapshot()
	state := parseLitdState(t, rec.Stdout)
	t.Logf("FSV playtest happy: beforeHash=%s afterHash=%s dirty %v->%v tempBefore=%v tempAfter=%v stdout=%s",
		beforeHash, after.Playtest.StateHashAfter, before.Dirty, after.Dirty, rec.TempBefore, rec.TempAfter, rec.Stdout)
	if beforeHash != after.Playtest.StateHashAfter || !after.Dirty || !before.Dirty {
		t.Fatalf("playtest did not preserve dirty editor state: before dirty=%v after dirty=%v beforeHash=%s afterHash=%s", before.Dirty, after.Dirty, beforeHash, after.Playtest.StateHashAfter)
	}
	if !state.Order.Issued || len(state.Units) < 2 {
		t.Fatalf("playtest state did not show live ordered sim with placed units: %+v", state)
	}
	if !containsString(rec.ManifestFiles, "data/placement/editor.toml") || !containsString(rec.ManifestFiles, "main.lua") || !containsString(rec.ManifestFiles, "world.toml") {
		t.Fatalf("playtest archive manifest missing runtime/source entries: %v", rec.ManifestFiles)
	}
	assertRemoved(t, rec.TempDir, rec.TempAfter)
	assertPNG(t, rec.ShotPath)
}

func TestEditorPlaytestRefusesZeroStartsFSV(t *testing.T) {
	app := newCommandTestApp(t)
	app.world.Terrain.StartLocations = nil
	beforeHash := app.SimRelevantHash()
	rec, err := app.Playtest(playtestTestOptions(t, t.TempDir(), "", 1, 0))
	afterHash := app.SimRelevantHash()
	t.Logf("FSV playtest zero starts: beforeHash=%s afterHash=%s err=%v temp=%q status=%q", beforeHash, afterHash, err, rec.TempDir, app.Snapshot().Status)
	if err == nil || !strings.Contains(err.Error(), "start location") {
		t.Fatalf("zero-start playtest should be blocked before launch, got %v", err)
	}
	if rec.TempDir != "" || beforeHash != afterHash {
		t.Fatalf("zero-start refusal should not create temp files or alter state: rec=%+v before=%s after=%s", rec, beforeHash, afterHash)
	}
}

func TestEditorPlaytestRepeatAndKilledSessionFSV(t *testing.T) {
	app := newCommandTestApp(t)
	root := t.TempDir()
	first, err := app.Playtest(playtestTestOptions(t, root, filepath.Join(root, "first.png"), 2, 0))
	if err != nil {
		t.Fatalf("first playtest: %v\nstdout=%s\nstderr=%s", err, first.Stdout, first.Stderr)
	}
	second, err := app.Playtest(playtestTestOptions(t, root, filepath.Join(root, "second.png"), 2, 0))
	if err != nil {
		t.Fatalf("second playtest: %v\nstdout=%s\nstderr=%s", err, second.Stdout, second.Stderr)
	}
	t.Logf("FSV playtest repeat: firstTemp=%s secondTemp=%s firstAfter=%v secondAfter=%v", first.TempDir, second.TempDir, first.TempAfter, second.TempAfter)
	if first.TempDir == second.TempDir {
		t.Fatalf("repeat playtest reused temp dir: %s", first.TempDir)
	}
	assertRemoved(t, first.TempDir, first.TempAfter)
	assertRemoved(t, second.TempDir, second.TempAfter)

	beforeKill := app.SimRelevantHash()
	killed, err := app.Playtest(playtestTestOptions(t, root, filepath.Join(root, "killed.png"), 1_000_000, 50*time.Millisecond))
	afterKill := app.SimRelevantHash()
	t.Logf("FSV playtest killed: err=%v killed=%v exit=%d beforeHash=%s afterHash=%s tempAfter=%v stderr=%s",
		err, killed.Killed, killed.ExitCode, beforeKill, afterKill, killed.TempAfter, killed.Stderr)
	if err == nil || !killed.Killed {
		t.Fatalf("killed playtest should return killed process error, got err=%v rec=%+v", err, killed)
	}
	if beforeKill != afterKill {
		t.Fatalf("killed playtest changed editor state: before=%s after=%s", beforeKill, afterKill)
	}
	assertRemoved(t, killed.TempDir, killed.TempAfter)
}

func playtestTestOptions(t *testing.T, tempRoot, shot string, ticks int, killAfter time.Duration) PlaytestOptions {
	t.Helper()
	return PlaytestOptions{
		TempRoot:  tempRoot,
		Dir:       repoRoot(t, 3),
		ShotPath:  shot,
		Timeout:   45 * time.Second,
		KillAfter: killAfter,
		Command: []string{
			"go", "run", "./cmd/litd",
			"-archive", "{{archive}}",
			"-autotest",
			"-autotest-order",
			"-ticks", strconv.Itoa(ticks),
			"-shot", "{{shot}}",
		},
	}
}

type litdState struct {
	Order struct {
		Issued bool `json:"issued"`
	} `json:"order"`
	Units []struct {
		X float64 `json:"x"`
		Y float64 `json:"y"`
	} `json:"units"`
}

func parseLitdState(t *testing.T, stdout string) litdState {
	t.Helper()
	for _, line := range strings.Split(stdout, "\n") {
		if !strings.HasPrefix(line, "state: ") {
			continue
		}
		var state litdState
		if err := json.Unmarshal([]byte(strings.TrimPrefix(line, "state: ")), &state); err != nil {
			t.Fatal(err)
		}
		return state
	}
	t.Fatalf("no state line in stdout:\n%s", stdout)
	return litdState{}
}

func assertPNG(t *testing.T, path string) {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	img, err := png.Decode(f)
	if err != nil {
		t.Fatal(err)
	}
	b := img.Bounds()
	if b.Dx() != 640 || b.Dy() != 360 {
		t.Fatalf("playtest screenshot size=%dx%d, want 640x360", b.Dx(), b.Dy())
	}
}

func assertRemoved(t *testing.T, tempDir string, parentListing []string) {
	t.Helper()
	if tempDir == "" {
		t.Fatal("empty temp dir")
	}
	if _, err := os.Stat(tempDir); !os.IsNotExist(err) {
		t.Fatalf("temp dir %s still exists after playtest: err=%v", tempDir, err)
	}
	if containsString(parentListing, filepath.Base(tempDir)) {
		t.Fatalf("temp dir %s still appears in parent listing %v", tempDir, parentListing)
	}
}

func containsString(values []string, needle string) bool {
	for _, value := range values {
		if value == needle {
			return true
		}
	}
	return false
}
