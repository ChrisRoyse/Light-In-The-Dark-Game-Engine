package hud

// #201 terminal-screen render-layout FSV. SoT = the TerminalScreenLayout the
// builder returns for synthetic known inputs (X+X=Y): a known result + known
// stats => known label set, formatted stat text, draw-call count, empty Issues.
// Headless — no GL, no window.

import "testing"

func testTerminalCanvas(t *testing.T) Canvas {
	t.Helper()
	c, err := NewCanvas(1280, 960, 1)
	if err != nil {
		t.Fatalf("canvas: %v", err)
	}
	return c
}

func findTerminalLabel(layout TerminalScreenLayout, name string) (MenuScreenLabel, bool) {
	for _, l := range layout.Labels {
		if l.Name == name {
			return l, true
		}
	}
	return MenuScreenLabel{}, false
}

func terminalStrings() TerminalScreenStrings {
	return TerminalScreenStrings{
		Title:         "Victory",
		DurationLabel: "Duration",
		TrainedLabel:  "Units Trained",
		LostLabel:     "Units Lost",
		ExitLabel:     "Exit to Menu",
	}
}

func TestTerminalScreenLayoutFSV(t *testing.T) {
	canvas := testTerminalCanvas(t)
	// X+X=Y: duration 42, trained 3, lost 1 => the three rows read exactly those.
	stats := TerminalStats{DurationTicks: 42, UnitsTrained: 3, UnitsLost: 1}
	layout := NewTerminalScreenLayout(canvas, "terminal", TerminalVictory, stats, terminalStrings())

	t.Logf("FSV terminal layout: id=%s result=%d widgets=%d labels=%d drawCalls=%d issues=%v",
		layout.ID, layout.Result, len(layout.Widgets), len(layout.Labels), layout.ExpectedDrawCalls, layout.Issues)

	if len(layout.Issues) != 0 {
		t.Fatalf("layout has validation issues, want none: %v", layout.Issues)
	}
	// 1 card panel + (title + 3 stat rows + exit) = 6 draw calls.
	if len(layout.Widgets) != 1 {
		t.Fatalf("widgets=%d, want 1 (terminal card)", len(layout.Widgets))
	}
	if len(layout.Labels) != 5 {
		t.Fatalf("labels=%d, want 5 (title + 3 stats + exit)", len(layout.Labels))
	}
	if layout.ExpectedDrawCalls != 6 {
		t.Fatalf("drawCalls=%d, want 6", layout.ExpectedDrawCalls)
	}
	// Exact resolved + formatted text (the dynamic numbers folded into the row).
	want := map[string]string{
		"terminal-title":  "Victory",
		"terminal-stat-0": "Duration: 42",
		"terminal-stat-1": "Units Trained: 3",
		"terminal-stat-2": "Units Lost: 1",
		"terminal-exit":   "Exit to Menu",
	}
	for name, wantText := range want {
		l, ok := findTerminalLabel(layout, name)
		if !ok || l.Text != wantText {
			t.Fatalf("label %s = %q (found=%v), want %q", name, l.Text, ok, wantText)
		}
	}
	// Headline is focused (tinted at draw); result is a win.
	title, _ := findTerminalLabel(layout, "terminal-title")
	if !title.Focused {
		t.Fatalf("title focused=%v, want true (tinted headline)", title.Focused)
	}
	if !layout.Result.Won() {
		t.Fatal("TerminalVictory.Won() = false, want true")
	}
}

func TestTerminalScreenEdgeCasesFSV(t *testing.T) {
	canvas := testTerminalCanvas(t)
	strs := terminalStrings()
	strs.Title = "Defeat"

	// (1) Defeat result with zero stats: a 0-0-0 match still renders cleanly,
	// rows read ": 0", result is not a win (fail-closed default).
	zero := NewTerminalScreenLayout(canvas, "defeat", TerminalDefeat, TerminalStats{}, strs)
	t.Logf("FSV defeat/zero: result=%d won=%v labels=%d issues=%v", zero.Result, zero.Result.Won(), len(zero.Labels), zero.Issues)
	if zero.Result.Won() {
		t.Fatal("TerminalDefeat.Won() = true, want false")
	}
	if len(zero.Issues) != 0 {
		t.Fatalf("zero-stat terminal should validate clean, got %v", zero.Issues)
	}
	for i, want := range []string{"Duration: 0", "Units Trained: 0", "Units Lost: 0"} {
		l, _ := findTerminalLabel(zero, TerminalRowName(i))
		if l.Text != want {
			t.Fatalf("zero row %d = %q, want %q", i, l.Text, want)
		}
	}
	tl, _ := findTerminalLabel(zero, "terminal-title")
	if tl.Text != "Defeat" {
		t.Fatalf("defeat title = %q, want %q", tl.Text, "Defeat")
	}

	// (2) Large stats: a long match (six-figure ticks, big counts) must still
	// keep every row inside the card — no silent overflow, 0 issues.
	big := NewTerminalScreenLayout(canvas, "big", TerminalVictory,
		TerminalStats{DurationTicks: 999999, UnitsTrained: 4321, UnitsLost: 1234}, strs)
	t.Logf("FSV big-stats: labels=%d drawCalls=%d issues=%v", len(big.Labels), big.ExpectedDrawCalls, big.Issues)
	if len(big.Issues) != 0 {
		t.Fatalf("big-stat terminal overflowed its card: %v", big.Issues)
	}
	row0, _ := findTerminalLabel(big, TerminalRowName(0))
	if row0.Text != "Duration: 999999" {
		t.Fatalf("big duration row = %q, want %q", row0.Text, "Duration: 999999")
	}

	// (3) Empty strings: a screen built before resolution still produces a valid
	// card (never panics), rows are just ": <n>".
	empty := NewTerminalScreenLayout(canvas, "empty", TerminalVictory, TerminalStats{DurationTicks: 5}, TerminalScreenStrings{})
	t.Logf("FSV empty-strings: labels=%d issues=%v", len(empty.Labels), empty.Issues)
	if len(empty.Issues) != 0 {
		t.Fatalf("empty-string terminal should validate clean, got %v", empty.Issues)
	}
	e0, _ := findTerminalLabel(empty, TerminalRowName(0))
	if e0.Text != ": 5" {
		t.Fatalf("empty-string row0 = %q, want %q", e0.Text, ": 5")
	}
}
