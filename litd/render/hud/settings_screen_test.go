package hud

// #311 settings-screen render-layout FSV. SoT = the SettingsScreenLayout the
// builder returns for a known config.Settings (X+X=Y): known settings => known
// resolved value rows, formatted text, draw-call count, empty Issues. Headless —
// no GL, no window. Uses the REAL en locale table so the chrome resolution is
// proven end to end.

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
)

func settingsCanvas(t *testing.T) Canvas {
	t.Helper()
	c, err := NewCanvas(1280, 960, 1)
	if err != nil {
		t.Fatalf("canvas: %v", err)
	}
	return c
}

func settingsStrings(t *testing.T) SettingsScreenStrings {
	t.Helper()
	table, err := locale.Load(os.DirFS("../../../data"), "en")
	if err != nil {
		t.Fatalf("load en locale: %v", err)
	}
	return SettingsScreenStringsFromLocale(table)
}

func findSettingsLabel(layout SettingsScreenLayout, name string) (MenuScreenLabel, bool) {
	for _, l := range layout.Labels {
		if l.Name == name {
			return l, true
		}
	}
	return MenuScreenLabel{}, false
}

func TestSettingsScreenLayoutFSV(t *testing.T) {
	canvas := settingsCanvas(t)
	strs := settingsStrings(t)

	// BEFORE: a known non-default settings. X+X=Y: each value row must read it back.
	s := config.Settings{
		Graphics: config.PresetLow,
		Audio:    config.AudioVolumes{Master: 0.8, World: 1.0, UI: 0.0, Music: 0.3, Ambience: 0.5},
		Locale:   "fr",
		Keymap:   "classic",
	}
	t.Logf("FSV settings BEFORE: %+v", s)

	layout := NewSettingsScreenLayout(canvas, s, strs, 2) // focus row 2 (World)

	if len(layout.Issues) != 0 {
		t.Fatalf("layout has validation issues, want none: %v", layout.Issues)
	}
	// 1 panel widget + 10 labels (title + 8 value rows + back).
	if layout.ExpectedDrawCalls != 11 {
		t.Fatalf("drawCalls=%d, want 11", layout.ExpectedDrawCalls)
	}
	if len(layout.Labels) != 10 {
		t.Fatalf("labels=%d, want 10", len(layout.Labels))
	}

	// SoT: every value row reads back the live setting, localized + formatted.
	want := map[string]string{
		SettingsRowName(0): "Graphics: Low",
		SettingsRowName(1): "Master: 0.8",
		SettingsRowName(2): "World: 1.0",
		SettingsRowName(3): "Interface: 0.0",
		SettingsRowName(4): "Music: 0.3",
		SettingsRowName(5): "Ambience: 0.5",
		SettingsRowName(6): "Language: fr",
		SettingsRowName(7): "Hotkeys: Classic",
		SettingsBackName:   "Back",
	}
	for name, wantText := range want {
		l, ok := findSettingsLabel(layout, name)
		if !ok {
			t.Fatalf("row %s missing", name)
		}
		t.Logf("FSV AFTER row %-15s = %q", name, l.Text)
		if l.Text != wantText {
			t.Fatalf("row %s = %q, want %q", name, l.Text, wantText)
		}
	}

	// Focus highlights exactly row 2 (World), nothing else.
	for i := 0; i < SettingsValueRows; i++ {
		l, _ := findSettingsLabel(layout, SettingsRowName(i))
		if (i == 2) != l.Focused {
			t.Fatalf("row %d focused=%v, want %v", i, l.Focused, i == 2)
		}
	}
	if back, _ := findSettingsLabel(layout, SettingsBackName); back.Focused {
		t.Fatal("Back should not be focused when focus=2")
	}
}

// Defaults render the high-preset / grid-keymap / full-volume baseline; a muted
// group reads "0.0" not "0"; focus clamps at both ends.
func TestSettingsScreenDefaultsAndEdgesFSV(t *testing.T) {
	canvas := settingsCanvas(t)
	strs := settingsStrings(t)

	// Edge 1 — defaults: High / Grid / Master 1.0 / Music 0.8.
	def := config.DefaultSettings()
	t.Logf("FSV defaults BEFORE: %+v", def)
	layout := NewSettingsScreenLayout(canvas, def, strs, 0)
	want := map[string]string{
		SettingsRowName(0): "Graphics: High",
		SettingsRowName(1): "Master: 1.0",
		SettingsRowName(4): "Music: 0.8",
		SettingsRowName(6): "Language: en",
		SettingsRowName(7): "Hotkeys: Grid",
	}
	for name, wt := range want {
		l, ok := findSettingsLabel(layout, name)
		if !ok || l.Text != wt {
			t.Fatalf("default row %s = %q (found=%v), want %q", name, l.Text, ok, wt)
		}
		t.Logf("FSV default AFTER %-15s = %q", name, l.Text)
	}
	if len(layout.Issues) != 0 {
		t.Fatalf("default layout issues: %v", layout.Issues)
	}

	// Edge 2 — muted master reads "0.0", distinguishable from a default-omitted field.
	muted := config.DefaultSettings()
	muted.Audio.Master = 0.0
	ml := NewSettingsScreenLayout(canvas, muted, strs, 0)
	if l, _ := findSettingsLabel(ml, SettingsRowName(1)); l.Text != "Master: 0.0" {
		t.Fatalf("muted master = %q, want \"Master: 0.0\"", l.Text)
	}

	// Edge 3 — focus clamp: 99 -> Back (index SettingsValueRows), -1 -> row 0.
	hi := NewSettingsScreenLayout(canvas, def, strs, 99)
	if hi.Focused != SettingsValueRows {
		t.Fatalf("focus 99 clamped to %d, want %d (Back)", hi.Focused, SettingsValueRows)
	}
	if back, _ := findSettingsLabel(hi, SettingsBackName); !back.Focused {
		t.Fatal("focus 99 must highlight Back")
	}
	lo := NewSettingsScreenLayout(canvas, def, strs, -1)
	if lo.Focused != 0 {
		t.Fatalf("focus -1 clamped to %d, want 0", lo.Focused)
	}
	t.Logf("FSV focus clamp: 99->%d (Back) -1->%d", hi.Focused, lo.Focused)

	// Edge 4 — a second supported canvas size still validates clean (no offscreen).
	wide, err := NewCanvas(1920, 1080, 1)
	if err != nil {
		t.Fatalf("wide canvas: %v", err)
	}
	if w := NewSettingsScreenLayout(wide, def, strs, 0); len(w.Issues) != 0 {
		t.Fatalf("1920x1080 layout issues: %v", w.Issues)
	}
}
