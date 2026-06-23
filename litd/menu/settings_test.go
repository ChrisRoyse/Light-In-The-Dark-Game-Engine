package menu

// #311 slice 3 FSV. SoT = (a) the UIScreenEvent the api surface actually emits when
// the settings screen is Shown — captured off a real game's OnUIScreen sink, not the
// builder's return value; (b) the real data/locale tables resolving every key the
// screen carries; (c) the hud layout the render side derives. X+X=Y: each toggle/
// cycle button's command, fed back through config.Apply, must actually mutate the
// settings — proving the menu is wired to the reducer, not just labelled like it.

import (
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func TestSettingsScreenShownAndCapturedFSV(t *testing.T) {
	host, err := worldhost.Load("../../worlds/dev-sandbox", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load dev-sandbox: %v", err)
	}
	defer host.Close()
	g := host.Game

	// SoT: what the api surface actually emits, not what the builder returns.
	var captured api.UIScreen
	var shown bool
	g.OnUIScreen(func(ev api.UIScreenEvent) {
		if ev.Kind == api.UIScreenShow {
			captured = ev.Screen
			shown = true
		}
	})
	if !g.UI().Show(SettingsScreen()) {
		t.Fatal("g.UI().Show(SettingsScreen()) rejected a valid spec")
	}
	if !shown {
		t.Fatal("OnUIScreen sink never saw the settings screen")
	}
	t.Logf("FSV captured screen: id=%q title=%q buttons=%d", captured.ID, captured.TitleKey, len(captured.Buttons))
	if captured.ID != SettingsScreenID {
		t.Fatalf("captured id=%q want %q", captured.ID, SettingsScreenID)
	}
	if len(captured.Buttons) != 4 {
		t.Fatalf("captured %d buttons, want 4", len(captured.Buttons))
	}
}

func TestSettingsButtonsWiredToReducerFSV(t *testing.T) {
	// X+X=Y: feed each control button's command back through config.Apply; the
	// toggle/cycle controls MUST change the settings (menu -> reducer wiring), while
	// Back is controller-handled (reducer leaves it untouched, handled=false).
	scr := SettingsScreen()
	base := config.DefaultSettings()
	installed := []string{"en", "xx"}
	wired := map[string]bool{
		"graphics": true, "keymap": true, "locale": true, "back": false,
	}
	for _, b := range scr.Buttons {
		got, handled := config.Apply(base, b.Command, installed)
		changed := got != base
		t.Logf("FSV button %q cmd=%q -> handled=%v changed=%v", b.ID, b.Command, handled, changed)
		if want := wired[b.ID]; handled != want {
			t.Fatalf("button %q (cmd %q): reducer handled=%v, want %v", b.ID, b.Command, handled, want)
		}
		if wired[b.ID] && !changed {
			t.Fatalf("button %q (cmd %q): reducer handled it but settings unchanged", b.ID, b.Command)
		}
		if b.ID == "back" && changed {
			t.Fatalf("Back mutated settings (cmd %q)", b.Command)
		}
	}
}

func TestSettingsLocaleCoverageFSV(t *testing.T) {
	// Every key the screen carries must resolve in BOTH shipped tables, and the xx
	// pseudolocale must differ from en (proves the strings are real, not stubbed to
	// the key). Loading via locale.Load also re-runs the exact-set validation.
	scr := SettingsScreen()
	keys := []string{scr.TitleKey}
	for _, b := range scr.Buttons {
		keys = append(keys, b.LabelKey)
	}
	en, err := locale.Load(os.DirFS("../../data"), "en")
	if err != nil {
		t.Fatalf("load en: %v", err)
	}
	xx, err := locale.Load(os.DirFS("../../data"), "xx")
	if err != nil {
		t.Fatalf("load xx: %v", err)
	}
	for _, k := range keys {
		ev := en.Must(locale.Key(k))
		xv := xx.Must(locale.Key(k))
		t.Logf("FSV key %q en=%q xx=%q", k, ev, xv)
		if ev == "" || xv == "" {
			t.Fatalf("key %q resolved empty (en=%q xx=%q)", k, ev, xv)
		}
		if ev == xv {
			t.Fatalf("key %q identical across locales (%q) — not actually localized", k, ev)
		}
	}
}

func TestSettingsLayoutValidFSV(t *testing.T) {
	// The settings screen reuses the main-menu layout; assert it lays out cleanly
	// (no validation issues, all 4 buttons present) at a real resolution.
	scr := SettingsScreen()
	en, err := locale.Load(os.DirFS("../../data"), "en")
	if err != nil {
		t.Fatalf("load en: %v", err)
	}
	canvas, err := lithud.NewCanvas(1920, 1080, 1.0)
	if err != nil {
		t.Fatalf("canvas: %v", err)
	}
	buttons := make([]lithud.MenuButton, 0, len(scr.Buttons))
	for _, b := range scr.Buttons {
		buttons = append(buttons, lithud.MenuButton{ID: b.ID, Label: en.Must(locale.Key(b.LabelKey))})
	}
	strs := lithud.MenuScreenStrings{Title: en.Must(locale.SettingsTitle)}
	layout := lithud.NewMenuScreenLayout(canvas, scr.ID, strs, buttons, 0)
	t.Logf("FSV layout: id=%q widgets=%d labels=%d buttons=%d issues=%v",
		layout.ID, len(layout.Widgets), len(layout.Labels), len(layout.Buttons), layout.Issues)
	if len(layout.Issues) != 0 {
		t.Fatalf("settings layout has issues: %v", layout.Issues)
	}
	if len(layout.Buttons) != 4 {
		t.Fatalf("layout rendered %d buttons, want 4", len(layout.Buttons))
	}
}
