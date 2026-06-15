package litd

// #245 UI text-message public-API FSV. The text-message surface (Print/
// ClearMessages) is the one shippable UI primitive (the frame/dialog/board/
// quest system is deferred-v2 render). It is presentation-only and must be
// sim-inert: SoTs are (a) the state hash, byte-identical before/after any UI
// script, and (b) a recording sink installed via Game.OnUI that captures the
// resolved per-recipient events (so the no-op + fan-out paths are observable).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func uiWorld(t *testing.T) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	return w, newGame(w)
}

// TestUIPrintFanOutAndOptionsFSV — Print fans a multi-recipient call out to one
// resolved event per valid player, carrying the timed/positioned options;
// invalid handles are skipped.
func TestUIPrintFanOutAndOptionsFSV(t *testing.T) {
	w, g := uiWorld(t)
	var rec []UIMessageEvent
	g.OnUI(func(ev UIMessageEvent) { rec = append(rec, ev) })

	before := hashTop(w)
	to := []Player{g.Player(0), g.Player(2), {}} // zero-value handle must be skipped
	g.Print(to, "X+X=Y", For(7.5), At(Vec2{X: 0.25, Y: 0.5}))
	g.ClearMessages([]Player{g.Player(0)})
	after := hashTop(w)

	t.Logf("FSV UI sim-inert: hash before=%016x after=%016x", before, after)
	if before != after {
		t.Fatalf("UI message mutated sim state: %016x -> %016x", before, after)
	}

	// SoT: the recorded per-recipient events.
	t.Logf("FSV recorded %d events: %+v", len(rec), rec)
	if len(rec) != 3 { // 2 valid Print recipients + 1 clear
		t.Fatalf("recorded %d events, want 3 (zero-value player must be skipped)", len(rec))
	}
	if rec[0].Kind != UIPrint || rec[0].Player != 0 || rec[0].Text != "X+X=Y" {
		t.Fatalf("event 0 wrong: %+v", rec[0])
	}
	if rec[0].Duration != 7.5 || !rec[0].HasPos || rec[0].Pos.X != 0.25 || rec[0].Pos.Y != 0.5 {
		t.Fatalf("event 0 options not resolved: %+v", rec[0])
	}
	if rec[1].Player != 2 || rec[1].Text != "X+X=Y" {
		t.Fatalf("event 1 wrong recipient: %+v", rec[1])
	}
	if rec[2].Kind != UIClear || rec[2].Player != 0 || rec[2].Text != "" {
		t.Fatalf("clear event wrong: %+v", rec[2])
	}
}

// TestUIPrintEmptyAndHeadlessFSV — edge cases: an empty/nil recipient list is a
// no-op (no "to all" footgun), and with no sink installed every verb is a
// silent no-op (no panic, hash unchanged).
func TestUIPrintEmptyAndHeadlessFSV(t *testing.T) {
	w, g := uiWorld(t)
	if _, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0); !ok {
		t.Fatal("CreateUnit failed")
	}

	// (1) empty recipient list with a sink → zero events.
	var rec []UIMessageEvent
	g.OnUI(func(ev UIMessageEvent) { rec = append(rec, ev) })
	before := hashTop(w)
	g.Print(nil, "nobody")
	g.Print([]Player{}, "nobody")
	g.ClearMessages(nil)
	t.Logf("FSV empty recipients: events=%d (want 0)", len(rec))
	if len(rec) != 0 {
		t.Fatalf("empty recipient list leaked %d events", len(rec))
	}

	// (2) headless (no sink) → silent no-op, hash unchanged.
	g.OnUI(nil)
	g.Print([]Player{g.Player(0)}, "headless")
	g.ClearMessages([]Player{g.Player(0)})
	after := hashTop(w)
	t.Logf("FSV headless: hash before=%016x after=%016x", before, after)
	if before != after {
		t.Fatal("headless UI message touched the sim")
	}

	// (3) zero-value Game-bound nil receiver path is safe.
	var zero *Game
	zero.Print([]Player{g.Player(0)}, "safe") // must not panic
}
