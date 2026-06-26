package menu

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
)

// fakeSink records what the controller pushed to the runtime — the SoT for the
// "settings actually reach render/audio/input" half of #311.
type fakeSink struct {
	graphics        config.GraphicsPreset
	audio           config.AudioVolumes
	locale          string
	keymap          string
	applyCount      int
	restartRequired bool
}

func (f *fakeSink) ApplyGraphics(p config.GraphicsPreset) bool {
	f.graphics = p
	f.applyCount++
	return f.restartRequired
}
func (f *fakeSink) ApplyAudio(v config.AudioVolumes) { f.audio = v }
func (f *fakeSink) ApplyLocale(l string)             { f.locale = l }
func (f *fakeSink) ApplyKeymap(k string)             { f.keymap = k }

// #311 slice 3 — the settings controller. SoT (two of them): the on-disk TOML the
// controller persists, and the runtime state the sink received. A menu click is
// load → Apply → save → re-apply; this proves all four legs with known inputs.
func TestSettingsControllerRoutePersistsAndAppliesFSV(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "settings.toml")
	sink := &fakeSink{}
	save := func(s config.Settings) error { return config.SaveTo(path, s) }

	// BEFORE: defaults; the constructor must apply them to the sink immediately.
	c := NewSettingsController(config.DefaultSettings(), []string{"en", "fr"}, sink, save)
	t.Logf("FSV controller BEFORE settings=%+v sink=%+v", c.Settings(), sink)
	if c.Settings().Graphics != config.PresetHigh || sink.graphics != config.PresetHigh {
		t.Fatalf("initial graphics not applied: settings=%v sink=%v", c.Settings().Graphics, sink.graphics)
	}
	if sink.audio != config.DefaultSettings().Audio {
		t.Fatalf("initial audio not applied: %+v", sink.audio)
	}

	// ACTION 1: toggle graphics high→low. SoT delta: settings, sink, and the TOML.
	handled, err := c.Route(string(config.ToggleGraphics))
	if err != nil || !handled {
		t.Fatalf("graphics toggle handled=%v err=%v", handled, err)
	}
	if c.Settings().Graphics != config.PresetLow || sink.graphics != config.PresetLow {
		t.Fatalf("graphics not low after toggle: settings=%v sink=%v", c.Settings().Graphics, sink.graphics)
	}
	// Read the persisted file back — return values are claims, the file is the verdict.
	onDisk, _, err := config.LoadFrom(path)
	if err != nil {
		t.Fatal(err)
	}
	blob, _ := os.ReadFile(path)
	t.Logf("FSV controller AFTER graphics toggle: onDisk=%+v\nTOML:\n%s", onDisk, blob)
	if onDisk.Graphics != config.PresetLow {
		t.Fatalf("persisted graphics = %v, want low", onDisk.Graphics)
	}

	// ACTION 2: nudge master volume down one step (1.0 → 0.9). Reducer + sink + disk.
	if _, err := c.Route("settings.audio.master.down"); err != nil {
		t.Fatal(err)
	}
	if got := c.Settings().Audio.Master; got != 0.9 {
		t.Fatalf("master volume = %.3f, want 0.9", got)
	}
	if sink.audio.Master != 0.9 {
		t.Fatalf("sink master volume = %.3f, want 0.9", sink.audio.Master)
	}
	onDisk2, _, _ := config.LoadFrom(path)
	t.Logf("FSV controller AFTER master down: disk master=%.3f sink master=%.3f", onDisk2.Audio.Master, sink.audio.Master)
	if onDisk2.Audio.Master != 0.9 {
		t.Fatalf("persisted master = %.3f, want 0.9", onDisk2.Audio.Master)
	}

	// ACTION 3: cycle locale en→fr; HUD-string sink must receive the new tag.
	if _, err := c.Route(string(config.CycleLocale)); err != nil {
		t.Fatal(err)
	}
	if c.Settings().Locale != "fr" || sink.locale != "fr" {
		t.Fatalf("locale not fr: settings=%q sink=%q", c.Settings().Locale, sink.locale)
	}
}

// Edge cases: unrecognized command is a no-op (no write, no runtime perturbation);
// the Back command is NOT a settings mutation; restart-required is surfaced.
func TestSettingsControllerEdgesFSV(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "settings.toml")
	saves := 0
	sink := &fakeSink{restartRequired: true}
	c := NewSettingsController(config.DefaultSettings(), []string{"en"}, sink, func(s config.Settings) error {
		saves++
		return config.SaveTo(path, s)
	})
	baseApply := sink.applyCount // from the constructor's initial apply

	// Edge 1: stray/unknown command → not handled, no save, no extra sink apply, no file.
	for _, cmd := range []string{"settings.bogus", BackCommand, "", "settings.audio.master.sideways"} {
		handled, err := c.Route(cmd)
		if handled || err != nil {
			t.Fatalf("command %q: handled=%v err=%v, want no-op", cmd, handled, err)
		}
	}
	if saves != 0 {
		t.Fatalf("unknown commands persisted %d times, want 0", saves)
	}
	if sink.applyCount != baseApply {
		t.Fatalf("unknown commands re-applied to sink (%d != %d)", sink.applyCount, baseApply)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("config file written for no-op commands: %v", err)
	}

	// Edge 2: single installed locale → CycleLocale wraps to itself but is handled.
	handled, _ := c.Route(string(config.CycleLocale))
	if !handled || c.Settings().Locale != "en" {
		t.Fatalf("single-locale cycle: handled=%v locale=%q", handled, c.Settings().Locale)
	}

	// Edge 3: a graphics change reports restart-required honestly (sink said true).
	if _, err := c.Route(string(config.ToggleGraphics)); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV controller restartRequired AFTER graphics toggle = %v", c.RestartRequired())
	if !c.RestartRequired() {
		t.Fatal("restart-required not surfaced when the sink reported it")
	}
}
