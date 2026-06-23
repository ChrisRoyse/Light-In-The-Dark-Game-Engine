package hud

// #71 stall-overlay render-layout FSV. SoT = the StallScreenLayout for known
// inputs (X+X=Y): known laggards + known countdown => known resolved rows,
// formatted text, empty Issues. Headless. Uses the real en locale table.

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

func stallStrings(t *testing.T) StallScreenStrings {
	t.Helper()
	table, err := locale.Load(os.DirFS("../../../data"), "en")
	if err != nil {
		t.Fatalf("load en locale: %v", err)
	}
	return StallScreenStringsFromLocale(table)
}

func findStallLabel(layout StallScreenLayout, name string) (MenuScreenLabel, bool) {
	for _, l := range layout.Labels {
		if l.Name == name {
			return l, true
		}
	}
	return MenuScreenLabel{}, false
}

func TestStallScreenLayoutFSV(t *testing.T) {
	canvas, err := NewCanvas(1280, 960, 1)
	if err != nil {
		t.Fatal(err)
	}
	strs := stallStrings(t)

	// BEFORE: two laggards, 17 s left. SoT: one row per laggard + a countdown row.
	laggards := []string{"Ser Caldus", "Mira Vale"}
	t.Logf("FSV stall BEFORE laggards=%v seconds=17", laggards)
	layout := NewStallScreenLayout(canvas, laggards, 17, strs)

	if len(layout.Issues) != 0 {
		t.Fatalf("layout issues, want none: %v", layout.Issues)
	}
	// 1 panel + title + 2 laggard rows + countdown = 1 + 4 labels.
	if layout.ExpectedDrawCalls != 5 || len(layout.Labels) != 4 {
		t.Fatalf("drawCalls=%d labels=%d, want 5 and 4", layout.ExpectedDrawCalls, len(layout.Labels))
	}
	want := map[string]string{
		"stall-title":      "Connection Stalled",
		StallRowName(0):    "Waiting for Ser Caldus",
		StallRowName(1):    "Waiting for Mira Vale",
		StallCountdownName: "Dropping in 17s",
	}
	for name, wt := range want {
		l, ok := findStallLabel(layout, name)
		if !ok || l.Text != wt {
			t.Fatalf("row %s = %q (found=%v), want %q", name, l.Text, ok, wt)
		}
		t.Logf("FSV AFTER %-16s = %q", name, l.Text)
	}
}

// Edge cases: a single laggard; an empty laggard list still yields a valid card;
// a negative countdown clamps to 0.
func TestStallScreenEdgesFSV(t *testing.T) {
	canvas, _ := NewCanvas(1280, 960, 1)
	strs := stallStrings(t)

	// Edge 1 — single laggard, countdown 0 (about to drop).
	one := NewStallScreenLayout(canvas, []string{"Player 3"}, 0, strs)
	if l, _ := findStallLabel(one, StallRowName(0)); l.Text != "Waiting for Player 3" {
		t.Fatalf("single laggard row = %q", l.Text)
	}
	if l, _ := findStallLabel(one, StallCountdownName); l.Text != "Dropping in 0s" {
		t.Fatalf("countdown = %q, want \"Dropping in 0s\"", l.Text)
	}
	if len(one.Issues) != 0 {
		t.Fatalf("single-laggard issues: %v", one.Issues)
	}

	// Edge 2 — empty laggard list (gate momentarily blocked, name not yet resolved):
	// still a valid card with just title + countdown, no laggard rows.
	empty := NewStallScreenLayout(canvas, nil, 30, strs)
	t.Logf("FSV empty-laggards labels=%d issues=%v", len(empty.Labels), empty.Issues)
	if len(empty.Issues) != 0 {
		t.Fatalf("empty-laggard issues: %v", empty.Issues)
	}
	if _, ok := findStallLabel(empty, StallRowName(0)); ok {
		t.Fatal("empty laggards must produce no laggard row")
	}
	if l, _ := findStallLabel(empty, StallCountdownName); l.Text != "Dropping in 30s" {
		t.Fatalf("empty countdown = %q", l.Text)
	}

	// Edge 3 — negative seconds clamps to 0 (never renders "-5s").
	neg := NewStallScreenLayout(canvas, []string{"X"}, -5, strs)
	if l, _ := findStallLabel(neg, StallCountdownName); l.Text != "Dropping in 0s" {
		t.Fatalf("negative countdown = %q, want clamp to 0s", l.Text)
	}
	if neg.SecondsRemaining != 0 {
		t.Fatalf("SecondsRemaining=%d, want 0 after clamp", neg.SecondsRemaining)
	}
}
