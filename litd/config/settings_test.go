package config

// #311 user-config persistence FSV. SoT = the TOML actually written to disk: we
// marshal settings, write a real file under t.TempDir(), read the bytes back,
// LoadSettings them, and assert the round-trip. X+X=Y: a config with preset=low
// and master=0.0 must read back EXACTLY PresetLow + 0.0 (muted, not the 1.0
// default) — proving the pointer-presence distinction. Fail-closed edges
// (corrupt, empty, out-of-range, unknown token) each fall back to defaults with
// a warning and never crash.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeConfig(t *testing.T, blob []byte) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "settings.toml")
	if err := os.WriteFile(path, blob, 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}
	return path
}

func TestSettingsRoundTripFSV(t *testing.T) {
	// Modify every field away from defaults — including a MUTED master (0.0) to
	// prove muted != missing.
	want := Settings{
		Graphics: PresetLow,
		Audio:    AudioVolumes{Master: 0, World: 0.25, UI: 0.5, Music: 0.75, Ambience: 1},
		Locale:   "xx",
		Keymap:   "classic",
	}
	blob, err := want.Marshal()
	if err != nil {
		t.Fatalf("Marshal: %v", err)
	}
	path := writeConfig(t, blob)

	// SoT: read the actual bytes on disk and inspect them.
	onDisk, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read config: %v", err)
	}
	t.Logf("FSV config on disk:\n%s", string(onDisk))
	// The TOML must literally carry the values (contradiction check vs a silent default).
	for _, frag := range []string{`preset = "low"`, `master = 0.0`, `world = 0.25`, `keymap = "classic"`, `locale = "xx"`} {
		if !strings.Contains(string(onDisk), frag) {
			t.Fatalf("config TOML missing %q; got:\n%s", frag, string(onDisk))
		}
	}

	got, warns := LoadSettings(onDisk)
	t.Logf("FSV loaded: %+v warns=%v", got, warns)
	if len(warns) != 0 {
		t.Fatalf("clean round-trip produced warnings: %v", warns)
	}
	if got != want {
		t.Fatalf("round-trip mismatch:\n got  %+v\n want %+v", got, want)
	}
	// X+X=Y spotlight: muted master survived as 0, not reset to the 1.0 default.
	if got.Audio.Master != 0 {
		t.Fatalf("muted master read back as %v, want 0 (muted != missing)", got.Audio.Master)
	}
}

func TestSettingsCorruptAndEmptyFSV(t *testing.T) {
	def := DefaultSettings()

	// (1) Corrupt: not valid TOML -> defaults + warning, then a clean rewrite.
	corrupt := []byte("this is = = not [valid toml \x00\xff")
	got, warns := LoadSettings(corrupt)
	t.Logf("FSV corrupt: settings=%+v warns=%v", got, warns)
	if got != def {
		t.Fatalf("corrupt config did not fall back to defaults: %+v", got)
	}
	if len(warns) == 0 || !strings.Contains(warns[0], "parse error") {
		t.Fatalf("corrupt config gave no parse warning: %v", warns)
	}
	// Rewrite clean and confirm the new file now loads with zero warnings.
	clean, err := got.Marshal()
	if err != nil {
		t.Fatalf("Marshal clean: %v", err)
	}
	path := writeConfig(t, clean)
	reread, _ := os.ReadFile(path)
	if _, w2 := LoadSettings(reread); len(w2) != 0 {
		t.Fatalf("rewritten config still warns: %v", w2)
	}
	t.Logf("FSV corrupt->clean rewrite:\n%s", string(reread))

	// (2) Empty/missing: defaults + a warning, never a crash.
	g2, w := LoadSettings(nil)
	t.Logf("FSV empty: settings=%+v warns=%v", g2, w)
	if g2 != def || len(w) == 0 {
		t.Fatalf("empty config: settings=%+v warns=%v, want defaults+warning", g2, w)
	}
}

func TestSettingsClampAndUnknownFSV(t *testing.T) {
	def := DefaultSettings()

	// Out-of-range volumes clamp to [0,1] with warnings; unknown preset/keymap
	// fall back with warnings; a valid field in the same blob still applies.
	blob := []byte(`
locale = "fr"
[graphics]
preset = "ultra"
[audio]
master = 5.0
world = -1.0
ui = 0.4
[input]
keymap = "dvorak"
`)
	got, warns := LoadSettings(blob)
	t.Logf("FSV clamp/unknown: settings=%+v\nwarns=%v", got, warns)

	if got.Audio.Master != 1 {
		t.Fatalf("master 5.0 clamped to %v, want 1", got.Audio.Master)
	}
	if got.Audio.World != 0 {
		t.Fatalf("world -1.0 clamped to %v, want 0", got.Audio.World)
	}
	if got.Audio.UI != 0.4 {
		t.Fatalf("ui 0.4 = %v, want 0.4 (valid value dropped)", got.Audio.UI)
	}
	// unknown preset/keymap -> defaults.
	if got.Graphics != def.Graphics {
		t.Fatalf("unknown preset gave %v, want default %v", got.Graphics, def.Graphics)
	}
	if got.Keymap != def.Keymap {
		t.Fatalf("unknown keymap gave %q, want default %q", got.Keymap, def.Keymap)
	}
	// a present locale still applies (validity is the locale loader's job).
	if got.Locale != "fr" {
		t.Fatalf("locale = %q, want fr", got.Locale)
	}
	// Expect exactly 4 warnings: master, world, preset, keymap.
	if len(warns) != 4 {
		t.Fatalf("warnings = %d, want 4 (master,world,preset,keymap): %v", len(warns), warns)
	}
}

func TestSettingsPartialFSV(t *testing.T) {
	// A partial config: only audio.music set. That one field applies; every
	// other field keeps its default (no warning — missing != invalid).
	def := DefaultSettings()
	blob := []byte("[audio]\nmusic = 0.1\n")
	got, warns := LoadSettings(blob)
	t.Logf("FSV partial: settings=%+v warns=%v", got, warns)
	if got.Audio.Music != 0.1 {
		t.Fatalf("music = %v, want 0.1", got.Audio.Music)
	}
	if got.Audio.Master != def.Audio.Master || got.Graphics != def.Graphics || got.Locale != def.Locale || got.Keymap != def.Keymap {
		t.Fatalf("partial config disturbed an unset field: %+v", got)
	}
	if len(warns) != 0 {
		t.Fatalf("partial (valid) config warned: %v", warns)
	}
}
