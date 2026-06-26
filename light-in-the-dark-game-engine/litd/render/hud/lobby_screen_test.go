package hud

// #80 lobby-screen render-layout FSV. SoT = the LobbyScreenLayout for known slot
// rows (X+X=Y): known names+statuses => known resolved row text, focus, and empty
// Issues. Headless, real en locale table.

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

func lobbyStrings(t *testing.T) LobbyScreenStrings {
	t.Helper()
	table, err := locale.Load(os.DirFS("../../../data"), "en")
	if err != nil {
		t.Fatalf("load en locale: %v", err)
	}
	return LobbyScreenStringsFromLocale(table)
}

func findLobbyLabel(layout LobbyScreenLayout, name string) (MenuScreenLabel, bool) {
	for _, l := range layout.Labels {
		if l.Name == name {
			return l, true
		}
	}
	return MenuScreenLabel{}, false
}

func TestLobbyScreenLayoutFSV(t *testing.T) {
	canvas, err := NewCanvas(1280, 960, 1)
	if err != nil {
		t.Fatal(err)
	}
	strs := lobbyStrings(t)

	// A 4-slot lobby: host, a ready client, a waiting client, and an open slot.
	// Not all ready => canStart false => Start row shows the waiting suffix.
	slots := []LobbyScreenSlot{
		{Name: "Aldric", Status: LobbySlotStatusHost},
		{Name: "Bryn", Status: LobbySlotStatusReady},
		{Name: "Cael", Status: LobbySlotStatusWaiting},
		{Status: LobbySlotStatusOpen},
	}
	t.Logf("FSV lobby BEFORE: %d slots, canStart=false, focus=1", len(slots))
	layout := NewLobbyScreenLayout(canvas, slots, false, 1, strs)

	if len(layout.Issues) != 0 {
		t.Fatalf("layout issues, want none: %v", layout.Issues)
	}
	// 1 panel + title + 4 slot rows + start = 1 widget + 6 labels.
	if layout.ExpectedDrawCalls != 7 || len(layout.Labels) != 6 {
		t.Fatalf("drawCalls=%d labels=%d, want 7 and 6", layout.ExpectedDrawCalls, len(layout.Labels))
	}
	want := map[string]string{
		"lobby-title":   "Lobby",
		LobbyRowName(0): "1. Aldric — Host",
		LobbyRowName(1): "2. Bryn — Ready",
		LobbyRowName(2): "3. Cael — Waiting",
		LobbyRowName(3): "4. Open",
		LobbyStartName:  "Start Game (waiting…)",
	}
	for name, wt := range want {
		l, ok := findLobbyLabel(layout, name)
		if !ok || l.Text != wt {
			t.Fatalf("row %s = %q (found=%v), want %q", name, l.Text, ok, wt)
		}
		t.Logf("FSV AFTER %-14s = %q", name, l.Text)
	}
	// Focus is on slot row 1 (Bryn); the Start row is not focused.
	if l, _ := findLobbyLabel(layout, LobbyRowName(1)); !l.Focused {
		t.Fatal("slot row 1 should be focused")
	}
	if l, _ := findLobbyLabel(layout, LobbyStartName); l.Focused {
		t.Fatal("start row must not be focused when focus=1")
	}
}

func TestLobbyScreenStartReadyAndEdgesFSV(t *testing.T) {
	canvas, _ := NewCanvas(1280, 960, 1)
	strs := lobbyStrings(t)

	// All occupied slots ready => canStart true => Start row has no waiting suffix.
	ready := []LobbyScreenSlot{
		{Name: "Aldric", Status: LobbySlotStatusHost},
		{Name: "Bryn", Status: LobbySlotStatusReady},
	}
	// focus past the end clamps to the Start row (index len(slots)).
	layout := NewLobbyScreenLayout(canvas, ready, true, 99, strs)
	if l, _ := findLobbyLabel(layout, LobbyStartName); l.Text != "Start Game" {
		t.Fatalf("ready start row = %q, want \"Start Game\"", l.Text)
	}
	if l, _ := findLobbyLabel(layout, LobbyStartName); !l.Focused {
		t.Fatal("focus=99 must clamp to the Start row")
	}
	if layout.Focused != len(ready) {
		t.Fatalf("clamped focus=%d, want %d", layout.Focused, len(ready))
	}
	t.Logf("FSV ready: start row %q focused (focus clamped 99->%d)", "Start Game", layout.Focused)

	// Edge: negative focus clamps to 0 (the host row).
	neg := NewLobbyScreenLayout(canvas, ready, true, -5, strs)
	if neg.Focused != 0 {
		t.Fatalf("negative focus=%d, want 0", neg.Focused)
	}

	// Edge: a full 8-slot lobby still validates cleanly at a couple of resolutions.
	full := make([]LobbyScreenSlot, 8)
	full[0] = LobbyScreenSlot{Name: "Host", Status: LobbySlotStatusHost}
	for i := 1; i < 8; i++ {
		full[i] = LobbyScreenSlot{Name: "P" + string(rune('0'+i)), Status: LobbySlotStatusReady}
	}
	for _, res := range [][2]int{{1280, 960}, {1920, 1080}} {
		c, _ := NewCanvas(res[0], res[1], 1)
		fl := NewLobbyScreenLayout(c, full, true, 0, strs)
		if len(fl.Issues) != 0 {
			t.Fatalf("full lobby at %dx%d issues: %v", res[0], res[1], fl.Issues)
		}
		if len(fl.Labels) != 10 { // title + 8 slots + start
			t.Fatalf("full lobby labels=%d, want 10", len(fl.Labels))
		}
	}
	t.Log("FSV full 8-slot lobby validates clean at 1280x960 and 1920x1080")
}
