package match_test

// Regression for #665: the match-flow controller must NOT perturb the sim state
// hash. flow.go used to count terminal-screen stats via g.OnEvent — the
// sim-HASHING subscription path (R-SIM-6) — so merely wiring the match-flow UI
// changed the StateHash (a flow-on game hashed differently than a flow-off one),
// breaking the windowed-vs-headless "same match" determinism guarantee. Stats are
// now pull-drained from the non-hashing render-event snapshot, so flow-on and
// flow-off advance bit-identically. SoT = StateHash after the same ticks with and
// without a Flow wired.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
)

func TestFlowDoesNotPerturbStateHashFSV(t *testing.T) {
	const ticks = 300

	// Game A: no flow. Game B: identical, with a Flow driven through play and
	// polled every tick (as the windowed shell does).
	gA, _, barracksA, footmanA := matchGame(t)
	gB, p0B, barracksB, footmanB := matchGame(t)

	flow := match.NewFlow(gB, p0B)
	if !flow.Begin(match.Setup{Faction: match.FactionVigil}) || !flow.StartPlay() {
		t.Fatalf("flow failed to reach play: phase=%s", flow.Phase())
	}

	// Same gameplay on both so any hash delta is attributable to the flow wiring
	// alone, not to divergent orders.
	barracksA.Train(footmanA)
	barracksB.Train(footmanB)

	hashBeforeA, hashBeforeB := gA.StateHash(), gB.StateHash()
	t.Logf("FSV before: flow-off=%#016x flow-on=%#016x equal=%v", hashBeforeA, hashBeforeB, hashBeforeA == hashBeforeB)
	if hashBeforeA != hashBeforeB {
		t.Fatalf("baseline hashes differ before any advance: %#016x vs %#016x — matchGame not deterministic", hashBeforeA, hashBeforeB)
	}

	for i := 0; i < ticks; i++ {
		gA.Advance(1)
		gB.Advance(1)
		flow.Poll()
	}

	hashAfterA, hashAfterB := gA.StateHash(), gB.StateHash()
	t.Logf("FSV after %d ticks: flow-off=%#016x flow-on=%#016x equal=%v (flow.Stats=%+v)", ticks, hashAfterA, hashAfterB, hashAfterA == hashAfterB, flow.Stats())
	if hashAfterA != hashAfterB {
		t.Fatalf("flow-on hash %#016x != flow-off hash %#016x — the match-flow controller perturbed the sim hash (regression of #665: stats back on the hashing path?)", hashAfterB, hashAfterA)
	}
	// Sanity: the flow actually did its job (counted the trained footman off the
	// non-hashing channel), proving the equality isn't because the flow was inert.
	if flow.Stats().UnitsTrained != 1 {
		t.Fatalf("flow UnitsTrained = %d, want 1 — flow was inert, so the hash-equality proves nothing", flow.Stats().UnitsTrained)
	}
}
