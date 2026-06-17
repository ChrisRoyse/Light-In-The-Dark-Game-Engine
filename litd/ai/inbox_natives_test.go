package ai_test

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// These tests verify the litd/ai command-stack facade against the AUTHORITATIVE
// source of truth: the sim-owned per-player AI command stack (litd/sim/ai.go).
// sim.World is the CommandStackSource; we cross-check every facade operation
// against the sim's own accessors, proving the facade holds no second copy
// (#379). The stack's LIFO/cap/save semantics themselves are owned and FSV'd by
// litd/sim/ai_test.go; here we verify delegation and the WC3 native behavior.

const aiPlayer = 3

func newAIWorld() *sim.World {
	w := sim.NewWorld(sim.Caps{Units: 16})
	w.AttachAI(aiPlayer, 1)
	return w
}

func drainTrace(cs ai.CommandStack) [][2]int {
	var tr [][2]int
	for cs.CommandsWaiting() != 0 {
		tr = append(tr, [2]int{cs.GetLastCommand(), cs.GetLastData()})
		cs.PopLastCommand()
	}
	return tr
}

// TestInboxNativesGoldenSemanticsFSV — push (1,10),(2,20),(3,30) via the
// map-side CommandAI; the natives read them top-down (LIFO): 3,2,1. Golden table.
func TestInboxNativesGoldenSemanticsFSV(t *testing.T) {
	w := newAIWorld()
	cs := ai.NewCommandStack(w, aiPlayer)
	ai.CommandAI(w, aiPlayer, 1, 10)
	ai.CommandAI(w, aiPlayer, 2, 20)
	ai.CommandAI(w, aiPlayer, 3, 30)
	// Cross-check against the sim SoT directly — the facade wrote to the sim.
	t.Logf("FSV CommandsWaiting=%d, sim AICommandCount=%d (must agree)", cs.CommandsWaiting(), w.AICommandCount(aiPlayer))
	if cs.CommandsWaiting() != 3 || w.AICommandCount(aiPlayer) != 3 {
		t.Fatalf("facade=%d sim=%d want 3/3", cs.CommandsWaiting(), w.AICommandCount(aiPlayer))
	}
	got := drainTrace(cs)
	golden := [][2]int{{3, 30}, {2, 20}, {1, 10}}
	t.Logf("FSV drain trace (GetLastCommand,GetLastData) = %v want golden %v", got, golden)
	if len(got) != 3 {
		t.Fatalf("trace len=%d want 3", len(got))
	}
	for i := range golden {
		if got[i] != golden[i] {
			t.Fatalf("trace[%d]=%v want %v (LIFO)", i, got[i], golden[i])
		}
	}
	// Drain emptied the sim stack too (no second copy).
	if w.AICommandCount(aiPlayer) != 0 {
		t.Fatalf("after facade drain, sim still holds %d (facade not delegating)", w.AICommandCount(aiPlayer))
	}
}

// TestInboxNativesDelegationFSV — the facade is a pure view: a push through the
// map-side CommandAI is observable via the sim accessors, and a pop through the
// native is observable in the sim. No second representation (#379 regression).
func TestInboxNativesDelegationFSV(t *testing.T) {
	w := newAIWorld()
	cs := ai.NewCommandStack(w, aiPlayer)
	ai.CommandAI(w, aiPlayer, 42, 99)
	// Facade read == sim read.
	fc, fd, fok := cs.LastPair()
	sc, sd, sok := w.LastAICommand(aiPlayer)
	t.Logf("FSV facade top=(%d,%d,%v) sim top=(%d,%d,%v)", fc, fd, fok, sc, sd, sok)
	if !fok || !sok || fc != int(sc) || fd != int(sd) {
		t.Fatalf("facade/sim top disagree: (%d,%d) vs (%d,%d)", fc, fd, sc, sd)
	}
	// Pop through the native; the sim observes it.
	cs.PopLastCommand()
	t.Logf("FSV after native pop: facade waiting=%d sim count=%d", cs.CommandsWaiting(), w.AICommandCount(aiPlayer))
	if cs.CommandsWaiting() != 0 || w.AICommandCount(aiPlayer) != 0 {
		t.Fatalf("native pop not reflected in sim: facade=%d sim=%d", cs.CommandsWaiting(), w.AICommandCount(aiPlayer))
	}
}

// TestInboxNativesPopEmptyFSV — empty-stack access is WC3-faithful and
// panic-free. Issue edge 1.
func TestInboxNativesPopEmptyFSV(t *testing.T) {
	w := newAIWorld()
	cs := ai.NewCommandStack(w, aiPlayer)
	t.Logf("FSV empty: waiting=%d GetLastCommand=%d GetLastData=%d", cs.CommandsWaiting(), cs.GetLastCommand(), cs.GetLastData())
	if cs.CommandsWaiting() != 0 || cs.GetLastCommand() != 0 || cs.GetLastData() != 0 {
		t.Fatalf("empty waiting/cmd/data = %d/%d/%d want 0/0/0", cs.CommandsWaiting(), cs.GetLastCommand(), cs.GetLastData())
	}
	cs.PopLastCommand() // no panic, no negative
	cs.PopLastCommand()
	if _, _, ok := cs.LastPair(); ok {
		t.Fatal("LastPair on empty returned ok")
	}
	if cs.CommandsWaiting() != 0 {
		t.Fatalf("empty pop made waiting=%d want 0", cs.CommandsWaiting())
	}
	t.Logf("FSV PopLastCommand on empty is a no-op")
}

// TestInboxNativesThreeSameTickFSV — the map script sends 3 commands in one
// tick via CommandAI; all three are visible on the SAME tick, top = last sent
// (LIFO). Issue edge 2 (AI-visible order + delivery tick).
func TestInboxNativesThreeSameTickFSV(t *testing.T) {
	w := newAIWorld()
	cs := ai.NewCommandStack(w, aiPlayer)
	// "tick T": three CommandAI before the AI sub-phase reads.
	ai.CommandAI(w, aiPlayer, 5, 50)
	ai.CommandAI(w, aiPlayer, 6, 60)
	ai.CommandAI(w, aiPlayer, 7, 70)
	t.Logf("FSV after 3 CommandAI on tick T: CommandsWaiting=%d (same-tick delivery)", cs.CommandsWaiting())
	if cs.CommandsWaiting() != 3 {
		t.Fatalf("waiting=%d want 3 (not delivered same tick)", cs.CommandsWaiting())
	}
	c, d, _ := cs.LastPair()
	t.Logf("FSV AI sees top=(%d,%d) want (7,70); full drain=%v", c, d, drainTrace(cs))
	if c != 7 || d != 70 {
		t.Fatalf("top=(%d,%d) want (7,70)", c, d)
	}
}

// TestInboxNativesSaveRestoreFSV — undrained commands survive a sim save/load;
// the facade on the reloaded world sees the identical stack. Issue edge 3 (the
// stack is sim state, so the sim save is the authoritative round-trip).
func TestInboxNativesSaveRestoreFSV(t *testing.T) {
	w := newAIWorld()
	// Known input: push (1,100)..(5,500); known LIFO drain: 5,4,3,2,1.
	for i := 1; i <= 5; i++ {
		ai.CommandAI(w, aiPlayer, i, i*100)
	}
	want := [][2]int{{5, 500}, {4, 400}, {3, 300}, {2, 200}, {1, 100}}
	t.Logf("FSV before save: waiting=%d (pushed 1..5)", ai.NewCommandStack(w, aiPlayer).CommandsWaiting())

	var buf bytes.Buffer
	const fp = 0x276276
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := sim.NewWorld(sim.Caps{Units: 16})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	cs2 := ai.NewCommandStack(w2, aiPlayer)
	t.Logf("FSV after restore: waiting=%d (want 5)", cs2.CommandsWaiting())
	if cs2.CommandsWaiting() != 5 {
		t.Fatalf("restored waiting=%d want 5", cs2.CommandsWaiting())
	}
	got := drainTrace(cs2)
	t.Logf("FSV restored drain trace=%v want %v (LIFO)", got, want)
	if len(got) != len(want) {
		t.Fatalf("restored trace len %d != %d", len(got), len(want))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("restored trace[%d]=%v != %v", i, got[i], want[i])
		}
	}
	t.Logf("FSV 5 undrained commands survived the sim save/load identically")
}

// TestInboxNativesDeterminismFSV — two runs of the same interleaved send/pop
// script over fresh worlds produce identical observation traces. Issue edge 4.
func TestInboxNativesDeterminismFSV(t *testing.T) {
	script := func() [][2]int {
		w := newAIWorld()
		cs := ai.NewCommandStack(w, aiPlayer)
		var tr [][2]int
		record := func() {
			if c, d, ok := cs.LastPair(); ok {
				tr = append(tr, [2]int{c, d})
			} else {
				tr = append(tr, [2]int{-1, -1})
			}
		}
		ai.CommandAI(w, aiPlayer, 1, 1)
		ai.CommandAI(w, aiPlayer, 2, 2)
		record()           // top (2,2)
		cs.PopLastCommand() // remove 2
		ai.CommandAI(w, aiPlayer, 3, 3)
		record()           // top (3,3)
		cs.PopLastCommand() // remove 3
		record()           // top (1,1)
		cs.PopLastCommand() // remove 1
		record()           // empty → (-1,-1)
		return tr
	}
	a := script()
	b := script()
	t.Logf("FSV run A=%v", a)
	t.Logf("FSV run B=%v", b)
	want := [][2]int{{2, 2}, {3, 3}, {1, 1}, {-1, -1}}
	if len(a) != len(want) {
		t.Fatalf("trace len=%d want %d", len(a), len(want))
	}
	for i := range want {
		if a[i] != want[i] || b[i] != want[i] {
			t.Fatalf("trace[%d] A=%v B=%v want %v", i, a[i], b[i], want[i])
		}
	}
	t.Logf("FSV interleaved send/pop deterministic across 2 runs, matches golden %v", want)
}
