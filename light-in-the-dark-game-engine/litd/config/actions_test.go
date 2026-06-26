package config

// #311 settings-reducer FSV. SoT = the Settings value Apply returns for a known
// (settings, command) pair. X+X=Y: from master=1.0, ten "down" steps must land
// EXACTLY on 0.0 with no float drift (the 0.1-grid snap), and one more "down"
// stays 0.0 (clamp). Every control's cycle is checked, plus the fail-closed path:
// an unknown command returns the input byte-identical and handled=false.

import "testing"

func TestApplyGraphicsAndKeymapToggleFSV(t *testing.T) {
	s := DefaultSettings() // PresetHigh, keymap grid
	s, ok := Apply(s, string(ToggleGraphics), nil)
	if !ok || s.Graphics != PresetLow {
		t.Fatalf("toggle graphics: ok=%v preset=%v, want true/low", ok, s.Graphics)
	}
	s, _ = Apply(s, string(ToggleGraphics), nil)
	if s.Graphics != PresetHigh {
		t.Fatalf("graphics did not toggle back to high: %v", s.Graphics)
	}
	s, ok = Apply(s, string(ToggleKeymap), nil)
	if !ok || s.Keymap != "classic" {
		t.Fatalf("toggle keymap: ok=%v keymap=%q, want true/classic", ok, s.Keymap)
	}
	s, _ = Apply(s, string(ToggleKeymap), nil)
	if s.Keymap != "grid" {
		t.Fatalf("keymap did not toggle back to grid: %q", s.Keymap)
	}
	t.Logf("FSV toggles ok: graphics=%v keymap=%q", s.Graphics, s.Keymap)
}

func TestApplyVolumeStepGridAndClampFSV(t *testing.T) {
	s := DefaultSettings() // master = 1.0
	// Ten down-steps must land exactly on 0.0 (no float drift off the 0.1 grid).
	for i := 0; i < 10; i++ {
		var ok bool
		s, ok = Apply(s, "settings.audio.master.down", nil)
		if !ok {
			t.Fatalf("master.down step %d not handled", i)
		}
	}
	t.Logf("FSV master after 10 down-steps = %v", s.Audio.Master)
	if s.Audio.Master != 0 {
		t.Fatalf("master after 10 down = %v, want exactly 0 (grid snap)", s.Audio.Master)
	}
	// One more down stays clamped at 0.
	s, _ = Apply(s, "settings.audio.master.down", nil)
	if s.Audio.Master != 0 {
		t.Fatalf("master below 0: %v, want 0 (clamp)", s.Audio.Master)
	}
	// Up from default 1.0 stays clamped at 1.
	up := DefaultSettings()
	up, _ = Apply(up, "settings.audio.master.up", nil)
	if up.Audio.Master != 1 {
		t.Fatalf("master above 1: %v, want 1 (clamp)", up.Audio.Master)
	}
	// A mid-range step lands on the grid: world 1.0 -> 0.9 -> 0.8.
	w := DefaultSettings()
	w, _ = Apply(w, "settings.audio.world.down", nil)
	w, _ = Apply(w, "settings.audio.world.down", nil)
	if w.Audio.World != 0.8 {
		t.Fatalf("world after 2 down = %v, want 0.8", w.Audio.World)
	}
	// Each group is independent: stepping world left master untouched.
	if w.Audio.Master != 1 {
		t.Fatalf("world step disturbed master: %v, want 1", w.Audio.Master)
	}
}

func TestApplyLocaleCycleFSV(t *testing.T) {
	installed := []string{"en", "xx"}
	s := DefaultSettings() // locale "en"
	s, ok := Apply(s, string(CycleLocale), installed)
	if !ok || s.Locale != "xx" {
		t.Fatalf("cycle locale: ok=%v locale=%q, want true/xx", ok, s.Locale)
	}
	s, _ = Apply(s, string(CycleLocale), installed) // wraps back to en
	if s.Locale != "en" {
		t.Fatalf("locale did not wrap to en: %q", s.Locale)
	}
	t.Logf("FSV locale cycle en->xx->en ok")
	// A stale locale not in the installed set recovers to the first entry.
	stale := Settings{Locale: "zz"}
	stale, ok = Apply(stale, string(CycleLocale), installed)
	if !ok || stale.Locale != "en" {
		t.Fatalf("stale locale recovery: ok=%v locale=%q, want true/en", ok, stale.Locale)
	}
	// No installed locales -> not handled, unchanged.
	none := DefaultSettings()
	got, ok := Apply(none, string(CycleLocale), nil)
	if ok || got != none {
		t.Fatalf("cycle with no locales: ok=%v changed=%v, want false/unchanged", ok, got != none)
	}
}

func TestApplyUnknownIsNoOpFSV(t *testing.T) {
	s := DefaultSettings()
	for _, bad := range []string{"", "settings.bogus", "settings.audio.master", "settings.audio.master.sideways", "settings.audio.bogus.up", "settings.audio..up"} {
		got, ok := Apply(s, bad, []string{"en"})
		if ok {
			t.Fatalf("unknown action %q reported handled", bad)
		}
		if got != s {
			t.Fatalf("unknown action %q mutated settings: %+v", bad, got)
		}
	}
	t.Logf("FSV all unknown actions are fail-closed no-ops")
}
