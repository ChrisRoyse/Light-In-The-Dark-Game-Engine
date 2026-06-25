package worldhost_test

// FSV for the local 3x determinism repeat (#651, ultimate-test-plan Phase 4 +
// §11) — the substitute for #210's cross-OS clause, which stays DEFERRED (not
// silently skipped) because the no-CI / gate-locally decision is permanent
// (CLAUDE.md). SoT = the final StateHash of the firstclash match run
// independently N times. A single non-matching run is a failure.

import (
	"runtime"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// runToTerminal loads firstclash at seed, steps to a latched result (or cap),
// and returns (terminalTick, finalHash).
func runToTerminal(t *testing.T, seed int64) (int, uint64) {
	t.Helper()
	h, err := worldhost.Load(firstclashDir, seed, 50_000_000)
	if err != nil {
		t.Fatalf("load firstclash seed=%d: %v", seed, err)
	}
	defer h.Close()
	g := h.Game
	term := -1
	for int(g.Tick()) < detCap {
		g.Advance(1)
		if g.Player(0).Result() != api.ResultPlaying || g.Player(1).Result() != api.ResultPlaying {
			term = int(g.Tick())
			break
		}
	}
	return term, g.StateHash()
}

func TestFirstclashDeterminismRepeatFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("3+ full firstclash matches (~16s); runs in the full preflight gate")
	}

	// 1. Three consecutive independent runs at the SAME seed → identical hash.
	const seed = 424242
	var hashes [3]uint64
	var terms [3]int
	for i := 0; i < 3; i++ {
		terms[i], hashes[i] = runToTerminal(t, seed)
		t.Logf("FSV repeat run %d/3: seed=%d terminal@%d finalHash=%#016x", i+1, seed, terms[i], hashes[i])
		if terms[i] < 0 {
			t.Fatalf("run %d did not terminate within %d ticks", i+1, detCap)
		}
	}
	if hashes[0] != hashes[1] || hashes[1] != hashes[2] {
		t.Fatalf("non-deterministic across 3 runs: %#016x / %#016x / %#016x", hashes[0], hashes[1], hashes[2])
	}
	t.Logf("FSV #651: 3/3 runs identical — finalHash=%#016x @tick %d", hashes[0], terms[0])

	// 2. Edge — a DIFFERENT seed diverges (the trace is seed-sensitive) yet still
	//    reaches a terminal result. Proves the determinism is the seed's doing,
	//    not a constant.
	termAlt, hashAlt := runToTerminal(t, seed+1)
	t.Logf("FSV seed-varied: seed=%d terminal@%d finalHash=%#016x", seed+1, termAlt, hashAlt)
	if termAlt < 0 {
		t.Fatalf("alt-seed run did not terminate within %d ticks", detCap)
	}
	if hashAlt == hashes[0] {
		t.Fatalf("alt seed produced the IDENTICAL hash %#016x — seed is not actually feeding the PRNG", hashAlt)
	}

	// 3. Edge — GOMAXPROCS invariance (R-SIM-1): the sim is single-threaded and
	//    deterministic regardless of scheduler parallelism. Re-run the base seed
	//    pinned to 1 P, then back to the prior setting, asserting the hash is
	//    unchanged from the multi-P runs above.
	prev := runtime.GOMAXPROCS(1)
	_, hashP1 := runToTerminal(t, seed)
	runtime.GOMAXPROCS(prev)
	t.Logf("FSV GOMAXPROCS=1: finalHash=%#016x (prev P=%d)", hashP1, prev)
	if hashP1 != hashes[0] {
		t.Fatalf("hash changed under GOMAXPROCS=1: %#016x vs %#016x (sim leaked parallelism)", hashP1, hashes[0])
	}
	t.Logf("FSV #651: cross-OS DEFERRED (no-CI policy, CLAUDE.md); 3x-local + GOMAXPROCS sweep substitute green")
}
