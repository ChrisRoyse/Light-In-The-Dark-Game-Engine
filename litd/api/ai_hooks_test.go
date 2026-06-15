package litd

// #257 AI-hooks FSV. SoT = the sim AI state read back after each API call
// (g.w.AIAttached/AIDifficulty/AIPaused/AICommandCount/LastAICommand) — proving
// the public verbs write real replay-safe state, not just stash their args.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func aiGame(t *testing.T) *Game {
	t.Helper()
	return newGame(sim.NewWorld(sim.Caps{Units: 16}))
}

// stubAI is a minimal AIController for AttachAI binding.
type stubAI struct{ ticks int }

func (s *stubAI) Tick(view AIView, cmd AICommander) { s.ticks++ }

// TestAttachAIFSV — AttachAI sets attached + difficulty in the sim and records
// the controller at the api layer.
func TestAttachAIFSV(t *testing.T) {
	g := aiGame(t)
	p := g.Player(2)

	t.Logf("FSV before: attached=%v difficulty=%d (want false/0)", g.IsAIPlayer(p), g.AIDifficulty(p))
	if g.IsAIPlayer(p) || g.AIDifficulty(p) != DifficultyEasy {
		t.Fatalf("player should start non-AI, easy")
	}

	ai := &stubAI{}
	g.AttachAI(p, ai, DifficultyInsane)

	// SoT: sim flags
	t.Logf("FSV after attach: simAttached=%v simDifficulty=%d (want true/2)",
		g.w.AIAttached(2), g.w.AIDifficulty(2))
	if !g.w.AIAttached(2) || g.w.AIDifficulty(2) != uint8(DifficultyInsane) {
		t.Fatalf("AttachAI did not write sim state: attached=%v diff=%d", g.w.AIAttached(2), g.w.AIDifficulty(2))
	}
	if g.IsAIPlayer(p) != true || g.AIDifficulty(p) != DifficultyInsane {
		t.Fatalf("api readback wrong")
	}
	// SoT: controller recorded at api layer
	if got, ok := g.aiControllers[2]; !ok || got != ai {
		t.Fatalf("controller not recorded for slot 2: %v", g.aiControllers)
	}

	// edge: invalid player handle is a no-op (no panic, nothing attached)
	var bad Player
	g.AttachAI(bad, ai, DifficultyNormal)
	if len(g.aiControllers) != 1 {
		t.Fatalf("invalid AttachAI mutated state: %v", g.aiControllers)
	}
}

// TestPauseAIFSV — PauseAI toggles the sim paused flag.
func TestPauseAIFSV(t *testing.T) {
	g := aiGame(t)
	p := g.Player(0)
	g.AttachAI(p, nil, DifficultyNormal)
	t.Logf("FSV paused default: %v (want false)", g.w.AIPaused(0))
	if g.w.AIPaused(0) {
		t.Fatal("AI should not start paused")
	}
	g.PauseAI(p, true)
	t.Logf("FSV after pause: %v (want true)", g.w.AIPaused(0))
	if !g.w.AIPaused(0) {
		t.Fatal("PauseAI(true) did not set the flag")
	}
	g.PauseAI(p, false)
	if g.w.AIPaused(0) {
		t.Fatal("PauseAI(false) did not clear the flag")
	}
}

// TestCommandAIFSV — CommandAI pushes integer pairs onto the sim command stack;
// the stack is LIFO and reads back exactly. Synthetic X+X=Y: push (7,11) then
// (9,13); top must be (9,13), count 2.
func TestCommandAIFSV(t *testing.T) {
	g := aiGame(t)
	p := g.Player(5)

	t.Logf("FSV empty stack: count=%d", g.w.AICommandCount(5))
	if g.w.AICommandCount(5) != 0 {
		t.Fatal("command stack should start empty")
	}

	g.CommandAI(p, 7, 11)
	g.CommandAI(p, 9, 13)
	t.Logf("FSV after 2 pushes: count=%d", g.w.AICommandCount(5))
	if g.w.AICommandCount(5) != 2 {
		t.Fatalf("want 2 commands, got %d", g.w.AICommandCount(5))
	}
	cmd, data, ok := g.w.LastAICommand(5)
	t.Logf("FSV top of stack: (%d,%d) ok=%v (want 9,13,true)", cmd, data, ok)
	if !ok || cmd != 9 || data != 13 {
		t.Fatalf("LIFO top wrong: (%d,%d)", cmd, data)
	}
	// pop reveals the earlier command
	g.w.PopAICommand(5)
	cmd, data, _ = g.w.LastAICommand(5)
	t.Logf("FSV after pop: top=(%d,%d) (want 7,11)", cmd, data)
	if cmd != 7 || data != 11 {
		t.Fatalf("after pop, top wrong: (%d,%d)", cmd, data)
	}
}

// TestCommandAICapFSV — the command stack caps cleanly (edge: max+1 rejected,
// no growth past the cap).
func TestCommandAICapFSV(t *testing.T) {
	g := aiGame(t)
	p := g.Player(1)
	for i := 0; i < 200; i++ { // far past the cap
		g.CommandAI(p, i, i)
	}
	n := g.w.AICommandCount(1)
	t.Logf("FSV cap: pushed 200, stored %d (want bounded)", n)
	if n > 64 {
		t.Fatalf("command stack grew past cap: %d", n)
	}
}
