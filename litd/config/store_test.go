package config

// #311 disk-persistence FSV. SoT = the actual settings.toml on disk under a temp
// config dir: SaveTo writes it, we read the raw bytes back, LoadFrom re-parses, and
// we assert the round-trip + every fail-closed edge against the real file. X+X=Y:
// SaveTo(non-default) then LoadFrom must return EXACTLY those settings, and the
// bytes on disk must literally carry them. Edges: missing file → defaults+note (no
// error); corrupt file → defaults+parse-warning (no error) then a clean Save makes
// the reload warning-free; the write is atomic (no .tmp left behind).

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestStoreRoundTripFSV(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, fileName)
	want := Settings{
		Graphics: PresetLow,
		Audio:    AudioVolumes{Master: 0, World: 0.25, UI: 0.5, Music: 0.75, Ambience: 1},
		Locale:   "xx",
		Keymap:   "classic",
	}
	if err := SaveTo(path, want); err != nil {
		t.Fatalf("SaveTo: %v", err)
	}

	// SoT: inspect the actual file the game would reload.
	onDisk, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read written config: %v", err)
	}
	t.Logf("FSV settings.toml on disk:\n%s", string(onDisk))
	for _, frag := range []string{`preset = "low"`, `master = 0.0`, `world = 0.25`, `keymap = "classic"`, `locale = "xx"`} {
		if !strings.Contains(string(onDisk), frag) {
			t.Fatalf("written TOML missing %q; got:\n%s", frag, string(onDisk))
		}
	}

	got, warns, err := LoadFrom(path)
	t.Logf("FSV LoadFrom: %+v warns=%v err=%v", got, warns, err)
	if err != nil {
		t.Fatalf("LoadFrom of a file we just wrote errored: %v", err)
	}
	if len(warns) != 0 {
		t.Fatalf("clean round-trip warned: %v", warns)
	}
	if got != want {
		t.Fatalf("round-trip mismatch:\n got  %+v\n want %+v", got, want)
	}
	// Atomic write must leave no temp sibling.
	if _, err := os.Stat(path + ".tmp"); !os.IsNotExist(err) {
		t.Fatalf("temp file %s.tmp survived the atomic write (err=%v)", path, err)
	}
}

func TestStoreMissingFileFSV(t *testing.T) {
	// A path that does not exist (first run) → defaults + a note, never an error.
	path := filepath.Join(t.TempDir(), "does-not-exist.toml")
	got, warns, err := LoadFrom(path)
	t.Logf("FSV missing: settings=%+v warns=%v err=%v", got, warns, err)
	if err != nil {
		t.Fatalf("missing file returned error %v, want nil (fail-closed)", err)
	}
	if got != DefaultSettings() {
		t.Fatalf("missing file did not yield defaults: %+v", got)
	}
	if len(warns) == 0 {
		t.Fatal("missing file gave no warning note")
	}
}

func TestStoreCorruptThenCleanFSV(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, fileName)
	// Write a genuinely corrupt file, then load: defaults + parse warning, no error.
	if err := os.WriteFile(path, []byte("this is = = not [valid toml \x00\xff"), 0o644); err != nil {
		t.Fatalf("seed corrupt: %v", err)
	}
	got, warns, err := LoadFrom(path)
	t.Logf("FSV corrupt: settings=%+v warns=%v err=%v", got, warns, err)
	if err != nil {
		t.Fatalf("corrupt file returned error %v, want nil (fail-closed)", err)
	}
	if got != DefaultSettings() {
		t.Fatalf("corrupt file did not fall back to defaults: %+v", got)
	}
	if len(warns) == 0 || !strings.Contains(warns[0], "parse error") {
		t.Fatalf("corrupt file gave no parse warning: %v", warns)
	}
	// Now persist the recovered defaults (the "rewrite clean" the spec calls for)
	// and confirm the reload is warning-free — the corruption is healed on disk.
	if err := SaveTo(path, got); err != nil {
		t.Fatalf("rewrite clean: %v", err)
	}
	reread, w2, err := LoadFrom(path)
	t.Logf("FSV healed reload: settings=%+v warns=%v", reread, w2)
	if err != nil || len(w2) != 0 {
		t.Fatalf("healed config still warns/errs: warns=%v err=%v", w2, err)
	}
	if reread != DefaultSettings() {
		t.Fatalf("healed config != defaults: %+v", reread)
	}
}

func TestStoreCreatesDirFSV(t *testing.T) {
	// SaveTo must create a missing parent dir (first-ever run, no litd/ yet).
	nested := filepath.Join(t.TempDir(), "litd", fileName)
	if err := SaveTo(nested, DefaultSettings()); err != nil {
		t.Fatalf("SaveTo into a missing dir: %v", err)
	}
	if _, err := os.Stat(nested); err != nil {
		t.Fatalf("SaveTo did not create the file/dir: %v", err)
	}
	t.Logf("FSV created %s", nested)
}
