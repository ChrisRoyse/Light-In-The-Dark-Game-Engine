package shell

import (
	"archive/zip"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func TestArchiveSaveLoadSaveRoundTripFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		t.Fatal(err)
	}
	players := sourceform.Players{Min: 1, Max: 8, Suggested: 2}
	if err := app.SetMapMetadata("Archive Round Trip FSV", "save load save", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 5); err != nil {
		t.Fatal(err)
	}
	archiveA := filepath.Join(root, "a.litdworld")
	if err := app.SaveArchive(archiveA); err != nil {
		t.Fatalf("save archive A: %v", err)
	}
	hashesA := archiveMemberHashList(t, archiveA)

	loaded := newTestApp(t)
	if err := loaded.OpenArchive(archiveA, filepath.Join(root, "work")); err != nil {
		t.Fatalf("open archive A: %v", err)
	}
	loadSnap := loaded.Snapshot()
	if loadSnap.ArchivePath != archiveA || loadSnap.ArchiveReadOnly || loadSnap.World.HeightCell != 5 {
		t.Fatalf("loaded snapshot mismatch: %+v", loadSnap)
	}
	archiveB := filepath.Join(root, "b.litdworld")
	if err := loaded.SaveArchive(archiveB); err != nil {
		t.Fatalf("save archive B: %v", err)
	}
	hashesB := archiveMemberHashList(t, archiveB)
	t.Logf("FSV archive A member hashes:\n%s", strings.Join(hashesA, "\n"))
	t.Logf("FSV archive B member hashes:\n%s", strings.Join(hashesB, "\n"))
	if !reflect.DeepEqual(hashesA, hashesB) {
		t.Fatalf("archive member hashes differ after save-load-save")
	}
}

func TestArchiveOpenRefusesCorruptedMemberFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		t.Fatal(err)
	}
	archivePath := filepath.Join(root, "clean.litdworld")
	if err := app.SaveArchive(archivePath); err != nil {
		t.Fatal(err)
	}
	corrupt := filepath.Join(root, "corrupt.litdworld")
	rewriteArchiveEntry(t, archivePath, corrupt, "world.toml", func(b []byte) []byte {
		if len(b) > 0 {
			b[0] ^= 0x01
		}
		return b
	})

	before := app.Snapshot()
	err := app.OpenArchive(corrupt, filepath.Join(root, "corrupt-work"))
	after := app.Snapshot()
	t.Logf("FSV corrupt open before: path=%s dirty=%v", before.ProjectPath, before.Dirty)
	t.Logf("FSV corrupt open after: path=%s dirty=%v err=%v status=%q", after.ProjectPath, after.Dirty, err, after.Status)
	if err == nil || !strings.Contains(err.Error(), "world.toml") || !strings.Contains(err.Error(), "content hash") {
		t.Fatalf("corrupt archive should name world.toml hash mismatch, got %v", err)
	}
	if after.ProjectPath != before.ProjectPath || after.Dirty != before.Dirty {
		t.Fatalf("corrupt open changed editor state: before=%+v after=%+v", before, after)
	}
}

func TestArchiveOpenRefusesEngineRangeFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		t.Fatal(err)
	}
	players := sourceform.Players{Min: 1, Max: 8, Suggested: 2}
	if err := app.SetMapMetadata("Future Engine FSV", "range edge", ">=99.0.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		t.Fatal(err)
	}
	archivePath := filepath.Join(root, "future.litdworld")
	if err := app.SaveArchive(archivePath); err != nil {
		t.Fatal(err)
	}
	err := app.OpenArchive(archivePath, filepath.Join(root, "future-work"))
	t.Logf("FSV engine-range refusal: editor=%s err=%v", EditorEngineVersion(), err)
	if err == nil || !strings.Contains(err.Error(), ">=99.0.0") || !strings.Contains(err.Error(), "does not satisfy") {
		t.Fatalf("future engine range should be refused, got %v", err)
	}
}

func TestArchiveSaveReadOnlyPathPreservesDirtyStateFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		t.Fatal(err)
	}
	if err := app.EditTerrainHeight(1, 1, 9); err != nil {
		t.Fatal(err)
	}
	roDir := filepath.Join(root, "readonly")
	if err := os.Mkdir(roDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(roDir, 0o555); err != nil {
		t.Fatal(err)
	}
	defer os.Chmod(roDir, 0o755)

	before := app.Snapshot()
	err := app.SaveArchive(filepath.Join(roDir, "blocked.litdworld"))
	after := app.Snapshot()
	t.Logf("FSV read-only save before: dirty=%v height[1,1]=%d", before.Dirty, before.World.HeightCell)
	t.Logf("FSV read-only save after: dirty=%v height[1,1]=%d err=%v status=%q", after.Dirty, after.World.HeightCell, err, after.Status)
	if err == nil || !strings.Contains(err.Error(), "read-only") {
		t.Fatalf("read-only archive path should be refused, got %v", err)
	}
	if !after.Dirty || after.World.HeightCell != before.World.HeightCell {
		t.Fatalf("read-only save did not preserve dirty in-memory map: before=%+v after=%+v", before, after)
	}
}

func TestOpenShippedM6ArchiveReadOnlyProjectionFSV(t *testing.T) {
	app := newTestApp(t)
	archivePath := repoRoot(t, 3, "worlds", "firstflame.litdworld")
	if err := app.OpenProject(archivePath); err != nil {
		t.Fatalf("open shipped archive: %v", err)
	}
	snap := app.Snapshot()
	t.Logf("FSV shipped M6 archive snapshot: archive=%s readOnly=%v name=%q size=%dx%d starts=%+v status=%q",
		snap.ArchivePath, snap.ArchiveReadOnly, snap.World.Name, snap.World.Width, snap.World.Height, snap.World.Starts, snap.Status)
	if snap.ArchivePath != archivePath || !snap.ArchiveReadOnly || snap.World.Name != "First Flame" || snap.World.Width != 64 || snap.World.Height != 64 || len(snap.World.Starts) != 2 {
		t.Fatalf("shipped archive projection mismatch: %+v", snap)
	}
	if err := app.Save(); err == nil || !strings.Contains(err.Error(), "read-only") {
		t.Fatalf("read-only shipped archive save should be refused, got %v", err)
	}
}

func archiveMemberHashList(t *testing.T, archivePath string) []string {
	t.Helper()
	opened, err := worldarchive.Open(archivePath, "")
	if err != nil {
		t.Fatalf("open %s: %v", archivePath, err)
	}
	defer opened.Close()
	lines := make([]string, 0, len(opened.Manifest.Files))
	for rel, entry := range opened.Manifest.Files {
		lines = append(lines, fmt.Sprintf("%s %s %d", rel, entry.Hash, entry.Size))
	}
	sort.Strings(lines)
	return lines
}

func rewriteArchiveEntry(t *testing.T, src, dst, target string, mutate func([]byte) []byte) {
	t.Helper()
	zr, err := zip.OpenReader(src)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	out, err := os.Create(dst)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(out)
	rewrote := false
	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			t.Fatal(err)
		}
		body, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			t.Fatal(err)
		}
		w, err := zw.Create(f.Name)
		if err != nil {
			t.Fatal(err)
		}
		if f.Name == target {
			body = mutate(body)
			rewrote = true
		}
		if _, err := w.Write(body); err != nil {
			t.Fatal(err)
		}
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := out.Close(); err != nil {
		t.Fatal(err)
	}
	if !rewrote {
		t.Fatalf("archive entry %q not found", target)
	}
}
