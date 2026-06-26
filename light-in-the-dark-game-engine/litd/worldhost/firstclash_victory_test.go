package worldhost_test

// FSV for the firstclash resolution layer (#646 decisive last-standing + #647
// 24,000-tick score-decide timeout). SoT = each player's latched match result
// (api Player.Result) after stepping the headless match. Proves the AI-vs-AI
// match TERMINATES with exactly one winner and one loser — no stalemate.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
	apicore "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func TestFirstclashTerminatesWithWinnerFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("24,000-tick AI-vs-AI match (~4s); full preflight gate runs it")
	}
	h, err := api.Load(firstclashDir, 1337, 50_000_000)
	if err != nil {
		t.Fatalf("load firstclash: %v", err)
	}
	defer h.Close()
	g := h.Game

	// BEFORE: both players are Playing.
	r0, r1 := g.Player(0).Result(), g.Player(1).Result()
	t.Logf("FSV BEFORE: tick=%d p0.Result=%d p1.Result=%d", g.Tick(), int(r0), int(r1))
	if r0 != apicore.ResultPlaying || r1 != apicore.ResultPlaying {
		t.Fatalf("pre-step: results %d/%d, want both Playing(%d)", int(r0), int(r1), int(apicore.ResultPlaying))
	}

	// Step until terminal, with a safety cap a hair past the 24,000-tick timeout
	// so the score-decide backstop is guaranteed to have fired if nothing
	// decisive happened earlier.
	const cap = 24050
	resolvedAt := -1
	for i := 0; i < cap; i++ {
		g.Advance(1)
		if g.Player(0).Result() != apicore.ResultPlaying || g.Player(1).Result() != apicore.ResultPlaying {
			resolvedAt = int(g.Tick())
			break
		}
	}

	r0, r1 = g.Player(0).Result(), g.Player(1).Result()
	t.Logf("FSV AFTER: resolvedAt=%d tick=%d p0.Result=%d p1.Result=%d", resolvedAt, g.Tick(), int(r0), int(r1))

	if resolvedAt < 0 {
		t.Fatalf("match did not terminate within %d ticks — stalemate (the #647 backstop must force a winner)", cap)
	}
	// Exactly one winner, one non-winner (Lost or Left). No double-win, no draw.
	won := 0
	if r0 == apicore.ResultWon {
		won++
	}
	if r1 == apicore.ResultWon {
		won++
	}
	if won != 1 {
		t.Fatalf("want exactly one winner, got p0=%d p1=%d", int(r0), int(r1))
	}
	loser := r1
	if r1 == apicore.ResultWon {
		loser = r0
	}
	if loser == apicore.ResultPlaying || loser == apicore.ResultWon {
		t.Fatalf("loser still has a non-terminal/winning result: %d", int(loser))
	}
	t.Logf("FSV firstclash terminates: resolved at tick %d with exactly one winner (no stalemate)", resolvedAt)
}
