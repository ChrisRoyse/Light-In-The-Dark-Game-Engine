package ai_test

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
)

// drainTrace pops the whole stack via the WC3 natives, recording the
// (command,data) it sees at each step — the exact sequence a ported AI script
// would observe with GetLastCommand/GetLastData/PopLastCommand.
func drainTrace(b *ai.Inbox) [][2]int {
	var tr [][2]int
	for b.CommandsWaiting() != 0 {
		tr = append(tr, [2]int{b.GetLastCommand(), b.GetLastData()})
		b.PopLastCommand()
	}
	return tr
}

// TestInboxNativesGoldenSemanticsFSV — push (1,10),(2,20),(3,30); the natives
// must read them top-down (LIFO): 3,2,1. Golden semantics table.
func TestInboxNativesGoldenSemanticsFSV(t *testing.T) {
	b := ai.NewCommandStack()
	b.Push(ai.InboxMsg{Command: 1, Data: 10})
	b.Push(ai.InboxMsg{Command: 2, Data: 20})
	b.Push(ai.InboxMsg{Command: 3, Data: 30})
	t.Logf("FSV pushed (1,10),(2,20),(3,30); CommandsWaiting=%d cap=%d", b.CommandsWaiting(), b.Cap())
	if b.CommandsWaiting() != 3 || b.Cap() != 12 {
		t.Fatalf("waiting=%d cap=%d want 3/12", b.CommandsWaiting(), b.Cap())
	}
	got := drainTrace(b)
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
}

// TestInboxNativesPopEmptyFSV — Pop/Get on an empty stack are WC3-faithful: 0
// and no-op, never a panic. Issue edge 1.
func TestInboxNativesPopEmptyFSV(t *testing.T) {
	b := ai.NewCommandStack()
	t.Logf("FSV empty stack: waiting=%d GetLastCommand=%d GetLastData=%d", b.CommandsWaiting(), b.GetLastCommand(), b.GetLastData())
	if b.CommandsWaiting() != 0 || b.GetLastCommand() != 0 || b.GetLastData() != 0 {
		t.Fatalf("empty stack waiting/cmd/data = %d/%d/%d want 0/0/0", b.CommandsWaiting(), b.GetLastCommand(), b.GetLastData())
	}
	b.PopLastCommand() // must not panic, must not go negative
	b.PopLastCommand()
	if _, _, ok := b.LastPair(); ok {
		t.Fatal("LastPair on empty returned ok")
	}
	t.Logf("FSV PopLastCommand on empty is a no-op; waiting still %d", b.CommandsWaiting())
	if b.CommandsWaiting() != 0 {
		t.Fatalf("empty pop made waiting=%d want 0", b.CommandsWaiting())
	}
}

// TestInboxNativesThreeSameTickFSV — the map script sends 3 commands in one
// tick via CommandAI; the AI sees all three on the SAME tick, top = the last
// one sent (LIFO). Issue edge 2 (AI-visible order + delivery tick).
func TestInboxNativesThreeSameTickFSV(t *testing.T) {
	bus := ai.NewCommandBus()
	bus.Add(1)
	// "tick T": the map domain sends three commands before the AI sub-phase.
	bus.CommandAI(1, 5, 50)
	bus.CommandAI(1, 6, 60)
	bus.CommandAI(1, 7, 70)
	box := bus.Box(1)
	// Same-tick delivery: all three already visible.
	t.Logf("FSV after 3 CommandAI on tick T: CommandsWaiting=%d (same-tick delivery)", box.CommandsWaiting())
	if box.CommandsWaiting() != 3 {
		t.Fatalf("waiting=%d want 3 (commands not delivered same tick)", box.CommandsWaiting())
	}
	// AI-visible order: top is the last sent (7,70), per LIFO.
	c, d, _ := box.LastPair()
	t.Logf("FSV AI sees top=(%d,%d) want (7,70); full drain=%v", c, d, drainTrace2(box))
	if c != 7 || d != 70 {
		t.Fatalf("top=(%d,%d) want (7,70)", c, d)
	}
}

func drainTrace2(b *ai.Inbox) [][2]int { return drainTrace(b) }

// TestInboxNativesSaveRestoreFSV — a bus with undrained commands serializes and
// restores byte-for-byte identical stacks. Issue edge 3.
func TestInboxNativesSaveRestoreFSV(t *testing.T) {
	bus := ai.NewCommandBus()
	bus.Add(1)
	bus.Add(2)
	// Player 1: 5 undrained commands; player 2: 2.
	for i := 1; i <= 5; i++ {
		bus.CommandAI(1, i, i*100)
	}
	bus.CommandAI(2, 9, 90)
	bus.CommandAI(2, 8, 80)
	// Force a drop on player 1 to confirm the dropped counter persists too.
	for i := 0; i < 10; i++ {
		bus.CommandAI(1, 99, 0) // 5 already + push to cap 12, then overflow
	}
	beforeW1, beforeDrop1 := bus.Box(1).CommandsWaiting(), bus.Box(1).Dropped()
	t.Logf("FSV before save: p1 waiting=%d dropped=%d, p2 waiting=%d", beforeW1, beforeDrop1, bus.Box(2).CommandsWaiting())
	blob := bus.Save(nil)

	// Fresh bus, same players registered, then Load.
	r := ai.NewCommandBus()
	r.Add(1)
	r.Add(2)
	if err := r.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	// Re-save must be byte-identical.
	if !bytes.Equal(blob, r.Save(nil)) {
		t.Fatal("re-save after restore not byte-identical")
	}
	// SoT: each player's stack drains to the identical trace, with identical
	// waiting + dropped counters.
	if r.Box(1).CommandsWaiting() != beforeW1 || r.Box(1).Dropped() != beforeDrop1 {
		t.Fatalf("p1 restored waiting=%d dropped=%d want %d/%d",
			r.Box(1).CommandsWaiting(), r.Box(1).Dropped(), beforeW1, beforeDrop1)
	}
	origTrace1 := drainTrace(bus.Box(1))
	restTrace1 := drainTrace(r.Box(1))
	t.Logf("FSV p1 orig trace len=%d, restored trace len=%d", len(origTrace1), len(restTrace1))
	if len(origTrace1) != len(restTrace1) {
		t.Fatalf("p1 trace lengths differ: %d vs %d", len(origTrace1), len(restTrace1))
	}
	for i := range origTrace1 {
		if origTrace1[i] != restTrace1[i] {
			t.Fatalf("p1 trace[%d] %v != restored %v", i, origTrace1[i], restTrace1[i])
		}
	}
	t.Logf("FSV save/restore: p1 %d cmds + %d dropped, p2 stack — all identical post-restore", beforeW1, beforeDrop1)
}

// TestInboxNativesDeterminismFSV — two runs of the same interleaved send/pop
// script produce identical observation traces. Issue edge 4.
func TestInboxNativesDeterminismFSV(t *testing.T) {
	script := func() [][2]int {
		bus := ai.NewCommandBus()
		bus.Add(3)
		box := bus.Box(3)
		var tr [][2]int
		record := func() {
			c, d, ok := box.LastPair()
			if ok {
				tr = append(tr, [2]int{c, d})
			} else {
				tr = append(tr, [2]int{-1, -1})
			}
		}
		bus.CommandAI(3, 1, 1)
		bus.CommandAI(3, 2, 2)
		record()              // top should be (2,2)
		box.PopLastCommand()  // remove 2
		bus.CommandAI(3, 3, 3)
		record()              // top should be (3,3)
		box.PopLastCommand()  // remove 3
		record()              // top should be (1,1)
		box.PopLastCommand()  // remove 1
		record()              // empty → (-1,-1)
		return tr
	}
	a := script()
	b := script()
	t.Logf("FSV run A trace=%v", a)
	t.Logf("FSV run B trace=%v", b)
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

// TestInboxNativesFailClosedLoadFSV — a corrupt bus blob leaves the bus
// unchanged (atomic, fail-closed).
func TestInboxNativesFailClosedLoadFSV(t *testing.T) {
	bus := ai.NewCommandBus()
	bus.Add(1)
	bus.CommandAI(1, 4, 40)
	bus.CommandAI(1, 5, 50)
	before := bus.Save(nil)

	if err := bus.Load([]byte("garbage-not-a-bus")); err == nil {
		t.Fatal("Load accepted a bad-magic blob")
	}
	good := bus.Save(nil)
	if err := bus.Load(good[:len(good)-3]); err == nil {
		t.Fatal("Load accepted a truncated blob")
	}
	// Player-mismatch: corrupt the player id.
	corrupt := append([]byte{}, good...)
	corrupt[8+2+4] ^= 0xFF
	if err := bus.Load(corrupt); err == nil {
		t.Fatal("Load accepted an unknown-player blob")
	}
	after := bus.Save(nil)
	t.Logf("FSV bus bytes before=%d after-failed-loads=%d", len(before), len(after))
	if !bytes.Equal(before, after) {
		t.Fatal("failed Load mutated the bus (not fail-closed)")
	}
	t.Logf("FSV three rejected loads; bus unchanged (waiting=%d)", bus.Box(1).CommandsWaiting())
}
