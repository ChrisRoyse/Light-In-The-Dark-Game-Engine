package sim

// #257 AI-hooks sim FSV. SoT = the per-player AI fields and command stacks,
// read back directly and through a save/load round-trip + state-hash compare.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// TestAIHookStateFSV — attach/pause/difficulty + command-stack LIFO at the SoT.
func TestAIHookStateFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 16})

	t.Logf("FSV before: attached=%v paused=%v diff=%d count=%d",
		w.AIAttached(3), w.AIPaused(3), w.AIDifficulty(3), w.AICommandCount(3))
	if w.AIAttached(3) || w.AIPaused(3) || w.AIDifficulty(3) != 0 || w.AICommandCount(3) != 0 {
		t.Fatal("AI state should be zero by default")
	}

	w.AttachAI(3, 2)
	w.SetAIPaused(3, true)
	t.Logf("FSV after attach+pause: attached=%v paused=%v diff=%d",
		w.AIAttached(3), w.AIPaused(3), w.AIDifficulty(3))
	if !w.AIAttached(3) || !w.AIPaused(3) || w.AIDifficulty(3) != 2 {
		t.Fatal("attach/pause/difficulty not applied")
	}

	// command stack LIFO: push (1,10),(2,20),(3,30) → top (3,30), pop → (2,20)
	w.PushAICommand(3, 1, 10)
	w.PushAICommand(3, 2, 20)
	w.PushAICommand(3, 3, 30)
	if n := w.AICommandCount(3); n != 3 {
		t.Fatalf("want 3 commands, got %d", n)
	}
	c, d, ok := w.LastAICommand(3)
	t.Logf("FSV top: (%d,%d) ok=%v", c, d, ok)
	if !ok || c != 3 || d != 30 {
		t.Fatalf("LIFO top wrong: (%d,%d)", c, d)
	}
	w.PopAICommand(3)
	c, d, _ = w.LastAICommand(3)
	if c != 2 || d != 20 {
		t.Fatalf("after pop, top wrong: (%d,%d)", c, d)
	}

	// edge: pop empties cleanly, then pop on empty is a no-op false
	w.PopAICommand(3)
	w.PopAICommand(3)
	if w.PopAICommand(3) {
		t.Fatal("pop on empty stack should return false")
	}
	_, _, ok = w.LastAICommand(3)
	if ok {
		t.Fatal("empty stack should report no last command")
	}

	// edge: detach clears everything
	w.PushAICommand(3, 9, 9)
	w.DetachAI(3)
	if w.AIAttached(3) || w.AIPaused(3) || w.AICommandCount(3) != 0 {
		t.Fatalf("DetachAI did not clear state")
	}

	// edge: out-of-range player is a safe no-op
	w.AttachAI(255, 1)
	if w.AIAttached(255) {
		t.Fatal("invalid player should not attach")
	}
}

// TestAICommandCapFSV — the per-player stack rejects past its cap.
func TestAICommandCapFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	accepted := 0
	for i := 0; i < maxAIInbox+50; i++ {
		if w.PushAICommand(0, int32(i), int32(i)) {
			accepted++
		}
	}
	t.Logf("FSV cap: accepted=%d count=%d (cap=%d)", accepted, w.AICommandCount(0), maxAIInbox)
	if accepted != maxAIInbox || w.AICommandCount(0) != maxAIInbox {
		t.Fatalf("stack cap not enforced: accepted=%d count=%d", accepted, w.AICommandCount(0))
	}
}

// TestAISaveRoundTripFSV — AI state survives save/load byte-identical (hash).
func TestAISaveRoundTripFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	w.AttachAI(0, 1)
	w.AttachAI(4, 2)
	w.SetAIPaused(4, true)
	w.PushAICommand(0, 100, 200)
	w.PushAICommand(0, 101, 201)
	w.PushAICommand(4, 7, 8)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x5151
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(Caps{Units: 16})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)

	t.Logf("FSV reload: p0 attached=%v diff=%d count=%d; p4 attached=%v paused=%v count=%d",
		w2.AIAttached(0), w2.AIDifficulty(0), w2.AICommandCount(0),
		w2.AIAttached(4), w2.AIPaused(4), w2.AICommandCount(4))
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)

	if !w2.AIAttached(0) || w2.AIDifficulty(0) != 1 || w2.AICommandCount(0) != 2 {
		t.Fatal("p0 AI state not restored")
	}
	if !w2.AIAttached(4) || !w2.AIPaused(4) || w2.AIDifficulty(4) != 2 || w2.AICommandCount(4) != 1 {
		t.Fatal("p4 AI state not restored")
	}
	c, d, _ := w2.LastAICommand(0)
	if c != 101 || d != 201 {
		t.Fatalf("p0 stack top wrong after reload: (%d,%d)", c, d)
	}
	if before.Top != after.Top {
		t.Fatalf("state hash diverged across save/load: %016x vs %016x", before.Top, after.Top)
	}
}
