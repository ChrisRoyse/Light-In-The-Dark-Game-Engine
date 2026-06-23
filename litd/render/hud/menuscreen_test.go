package hud

// #211 main-menu render-layout FSV. SoT = the MenuScreenLayout the builder
// returns for synthetic known inputs (the X+X=Y discipline): a known canvas +
// known buttons => known label set, focus highlight, draw-call count, and an
// empty Issues slice (geometry valid). Headless — no GL, no window.

import (
	"strings"
	"testing"
)

func testMenuCanvas(t *testing.T) Canvas {
	t.Helper()
	c, err := NewCanvas(1280, 960, 1)
	if err != nil {
		t.Fatalf("canvas: %v", err)
	}
	return c
}

func findMenuLabel(layout MenuScreenLayout, name string) (MenuScreenLabel, bool) {
	for _, l := range layout.Labels {
		if l.Name == name {
			return l, true
		}
	}
	return MenuScreenLabel{}, false
}

func TestMenuScreenLayoutFSV(t *testing.T) {
	canvas := testMenuCanvas(t)
	strs := MenuScreenStrings{Title: "Light in the Dark", Subtitle: "First Flame", Version: "v0.1"}
	buttons := []MenuButton{
		{ID: "skirmish", Label: "New Skirmish"},
		{ID: "load", Label: "Load Game"},
		{ID: "quit", Label: "Quit"},
	}
	layout := NewMenuScreenLayout(canvas, "main-menu", strs, buttons, 1)

	t.Logf("FSV menu layout: id=%s widgets=%d labels=%d drawCalls=%d focused=%d issues=%v",
		layout.ID, len(layout.Widgets), len(layout.Labels), layout.ExpectedDrawCalls, layout.Focused, layout.Issues)

	// Geometry valid: backdrop + panel on-canvas, no label off-parent or overlapping.
	if len(layout.Issues) != 0 {
		t.Fatalf("layout has validation issues, want none: %v", layout.Issues)
	}
	// 1 card panel + (title+subtitle+version) + 3 buttons = 7 draw calls.
	if len(layout.Widgets) != 1 {
		t.Fatalf("widgets=%d, want 1 (menu card)", len(layout.Widgets))
	}
	if len(layout.Labels) != 6 {
		t.Fatalf("labels=%d, want 6 (3 chrome + 3 buttons)", len(layout.Labels))
	}
	if layout.ExpectedDrawCalls != 7 {
		t.Fatalf("drawCalls=%d, want 7", layout.ExpectedDrawCalls)
	}
	// Chrome strings resolved (D-17: real resolved text, not keys).
	for name, want := range map[string]string{
		"menu-title":    "Light in the Dark",
		"menu-subtitle": "First Flame",
		"menu-version":  "v0.1",
	} {
		l, ok := findMenuLabel(layout, name)
		if !ok || l.Text != want {
			t.Fatalf("label %s = %q (found=%v), want %q", name, l.Text, ok, want)
		}
	}
	// Focus highlight: index 1 (Load Game) marked + ">" prefix; others padded, not focused.
	f, _ := findMenuLabel(layout, "menu-button-1")
	if !f.Focused || !strings.HasPrefix(f.Text, menuFocusMarker) || !strings.Contains(f.Text, "Load Game") {
		t.Fatalf("focused button-1 wrong: focused=%v text=%q", f.Focused, f.Text)
	}
	for _, name := range []string{"menu-button-0", "menu-button-2"} {
		b, _ := findMenuLabel(layout, name)
		if b.Focused || strings.HasPrefix(b.Text, menuFocusMarker) {
			t.Fatalf("unfocused %s wrongly highlighted: focused=%v text=%q", name, b.Focused, b.Text)
		}
	}
	if layout.Focused != 1 {
		t.Fatalf("Focused=%d, want 1", layout.Focused)
	}
}

func TestMenuScreenFocusNavFSV(t *testing.T) {
	// n=3: wrap forward 0->1->2->0, backward 0->2->1->0; from detached -1 lands on 0.
	cases := []struct {
		name               string
		focused, n         int
		wantNext, wantPrev int
	}{
		{"start", 0, 3, 1, 2},
		{"mid", 1, 3, 2, 0},
		{"end-wrap", 2, 3, 0, 1},
		{"detached", -1, 3, 0, 2},
		{"empty", 0, 0, -1, -1},
		{"single", 0, 1, 0, 0},
	}
	for _, c := range cases {
		gotN := MenuFocusNext(c.focused, c.n)
		gotP := MenuFocusPrev(c.focused, c.n)
		t.Logf("FSV nav %-9s focused=%d n=%d -> next=%d prev=%d", c.name, c.focused, c.n, gotN, gotP)
		if gotN != c.wantNext || gotP != c.wantPrev {
			t.Fatalf("nav %s: next=%d (want %d) prev=%d (want %d)", c.name, gotN, c.wantNext, gotP, c.wantPrev)
		}
	}
}

func TestMenuScreenEdgeCasesFSV(t *testing.T) {
	canvas := testMenuCanvas(t)
	strs := MenuScreenStrings{Title: "T"}

	// (1) Empty buttons: title-only screen is valid; focus detaches to -1; no button labels.
	empty := NewMenuScreenLayout(canvas, "empty", strs, nil, 0)
	t.Logf("FSV empty-buttons: focused=%d labels=%d issues=%v", empty.Focused, len(empty.Labels), empty.Issues)
	if empty.Focused != -1 {
		t.Fatalf("empty menu focused=%d, want -1", empty.Focused)
	}
	if len(empty.Issues) != 0 {
		t.Fatalf("title-only menu should validate clean, got %v", empty.Issues)
	}
	for _, l := range empty.Labels {
		if strings.HasPrefix(l.Name, "menu-button-") {
			t.Fatalf("empty menu produced button label %q", l.Name)
		}
	}

	// (2) Out-of-range focus clamps, never panics or indexes OOB.
	three := []MenuButton{{ID: "a", Label: "A"}, {ID: "b", Label: "B"}, {ID: "c", Label: "C"}}
	hi := NewMenuScreenLayout(canvas, "hi", strs, three, 99)
	lo := NewMenuScreenLayout(canvas, "lo", strs, three, -5)
	t.Logf("FSV clamp: focus99->%d focus-5->%d", hi.Focused, lo.Focused)
	if hi.Focused != 2 {
		t.Fatalf("focus 99 clamped to %d, want 2", hi.Focused)
	}
	if lo.Focused != 0 {
		t.Fatalf("focus -5 clamped to %d, want 0", lo.Focused)
	}

	// (3) Max stack (10 buttons): all entries stay inside the panel — no silent
	// overflow (the panel grows with the count). Validation must report 0 issues.
	many := make([]MenuButton, 10)
	for i := range many {
		many[i] = MenuButton{ID: "b" + string(rune('0'+i)), Label: "Entry " + string(rune('0'+i))}
	}
	big := NewMenuScreenLayout(canvas, "big", strs, many, 0)
	t.Logf("FSV 10-button stack: panelH-driven labels=%d drawCalls=%d issues=%v", len(big.Labels), big.ExpectedDrawCalls, big.Issues)
	if len(big.Issues) != 0 {
		t.Fatalf("10-button menu overflowed its panel: %v", big.Issues)
	}
	btnCount := 0
	for _, l := range big.Labels {
		if strings.HasPrefix(l.Name, "menu-button-") {
			btnCount++
		}
	}
	if btnCount != 10 {
		t.Fatalf("10-button menu has %d button labels, want 10", btnCount)
	}
}
