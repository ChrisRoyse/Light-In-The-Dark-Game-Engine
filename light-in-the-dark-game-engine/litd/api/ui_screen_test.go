package litd

// #526 g.UI() screen-surface FSV. Like the text-message surface, the screen
// builder is presentation-only and must be sim-inert: SoTs are (a) the state
// hash, byte-identical before/after any screen script, and (b) a recording sink
// installed via Game.OnUIScreen that captures the resolved UIScreenEvents (so
// the validate / fan-out / headless-no-op paths are observable). Synthetic
// known specs => known events.

import "testing"

func TestUIScreenShowHideFSV(t *testing.T) {
	w, g := uiWorld(t)
	var rec []UIScreenEvent
	g.OnUIScreen(func(ev UIScreenEvent) { rec = append(rec, ev) })

	before := hashTop(w)
	menu := UIScreen{
		ID:          "main-menu",
		TitleKey:    "menu.title",
		SubtitleKey: "menu.subtitle",
		Buttons: []UIButton{
			{ID: "skirmish", LabelKey: "menu.new_skirmish", Command: "flow.skirmish"},
			{ID: "load", LabelKey: "menu.load"},
			{ID: "quit", LabelKey: "menu.quit", Command: "flow.quit"},
		},
	}
	okShow := g.UI().Show(menu)
	okHide := g.UI().Hide("main-menu")
	after := hashTop(w)

	t.Logf("FSV UI screen sim-inert: hash before=%016x after=%016x show=%v hide=%v events=%d", before, after, okShow, okHide, len(rec))
	if before != after {
		t.Fatalf("UI screen mutated sim state: %016x -> %016x", before, after)
	}
	if !okShow || !okHide {
		t.Fatalf("valid show/hide rejected: show=%v hide=%v", okShow, okHide)
	}
	if len(rec) != 2 {
		t.Fatalf("expected 2 screen events (show+hide), got %d: %+v", len(rec), rec)
	}
	if rec[0].Kind != UIScreenShow || rec[0].Screen.ID != "main-menu" ||
		rec[0].Screen.TitleKey != "menu.title" || rec[0].Screen.SubtitleKey != "menu.subtitle" ||
		len(rec[0].Screen.Buttons) != 3 || rec[0].Screen.Buttons[0].Command != "flow.skirmish" ||
		rec[0].Screen.Buttons[2].ID != "quit" {
		t.Fatalf("show event wrong: %+v", rec[0])
	}
	if rec[1].Kind != UIScreenHide || rec[1].Screen.ID != "main-menu" {
		t.Fatalf("hide event wrong: %+v", rec[1])
	}
}

func TestUIScreenRejectsInvalidFSV(t *testing.T) {
	_, g := uiWorld(t)
	var rec []UIScreenEvent
	g.OnUIScreen(func(ev UIScreenEvent) { rec = append(rec, ev) })

	cases := []struct {
		name string
		s    UIScreen
	}{
		{"empty id", UIScreen{TitleKey: "t"}},
		{"empty title", UIScreen{ID: "s"}},
		{"button empty id", UIScreen{ID: "s", TitleKey: "t", Buttons: []UIButton{{LabelKey: "l"}}}},
		{"button empty label", UIScreen{ID: "s", TitleKey: "t", Buttons: []UIButton{{ID: "b"}}}},
		{"duplicate button id", UIScreen{ID: "s", TitleKey: "t", Buttons: []UIButton{{ID: "b", LabelKey: "l"}, {ID: "b", LabelKey: "l2"}}}},
	}
	for _, c := range cases {
		if ok := g.UI().Show(c.s); ok {
			t.Fatalf("invalid spec %q was accepted", c.name)
		}
	}
	if g.UI().Hide("") {
		t.Fatal("empty-id hide was accepted")
	}
	t.Logf("FSV invalid specs rejected: %d cases, events emitted=%d", len(cases), len(rec))
	if len(rec) != 0 {
		t.Fatalf("invalid specs emitted %d events, want 0: %+v", len(rec), rec)
	}
}

func TestUIScreenHeadlessFSV(t *testing.T) {
	_, g := uiWorld(t)
	// No sink: a valid spec still validates (returns true) but emits nothing,
	// and must not panic.
	if !g.UI().Show(UIScreen{ID: "x", TitleKey: "t"}) {
		t.Fatal("headless valid show should still report accepted")
	}
	if !g.UI().Hide("x") {
		t.Fatal("headless valid hide should still report accepted")
	}
	// Installing then clearing the sink restores no-op.
	var rec []UIScreenEvent
	g.OnUIScreen(func(ev UIScreenEvent) { rec = append(rec, ev) })
	g.OnUIScreen(nil)
	g.UI().Show(UIScreen{ID: "y", TitleKey: "t"})
	t.Logf("FSV headless: events after sink cleared=%d", len(rec))
	if len(rec) != 0 {
		t.Fatalf("cleared sink still received %d events", len(rec))
	}
}
