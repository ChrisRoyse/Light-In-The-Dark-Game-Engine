package worldhost_test

// FSV for the divergence detector / bisection (#650, ultimate-test-plan Phase 4;
// feeds #210). An induced fault on a firstclash run is CAUGHT and bisected to the
// FIRST diverging named system at the exact tick it was injected. SoT = the
// (divergentTick, systemName) the statehash.Registry.FirstDivergence detector
// reports for a clean vs faulted snapshot pair. The fault lives only in the
// faulted game and is reverted by construction (the clean control proves the
// system is quiet without it).

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// bisect runs two firstclash games in lockstep at the same seed; at faultTick it
// applies inject to the faulted game only. It returns the FIRST tick at which the
// top hashes differ, the name of the FIRST diverging system at that tick, and
// whether a divergence was found at all.
func bisect(t *testing.T, inject func(g *api.Game), faultTick, nTicks int) (int, string, bool) {
	t.Helper()
	const seed = 909090
	clean, err := worldhost.Load(firstclashDir, seed, 50_000_000)
	if err != nil {
		t.Fatalf("load clean: %v", err)
	}
	defer clean.Close()
	faulted, err := worldhost.Load(firstclashDir, seed, 50_000_000)
	if err != nil {
		t.Fatalf("load faulted: %v", err)
	}
	defer faulted.Close()

	reg := sim.NewHashRegistry()
	for tick := 1; tick <= nTicks; tick++ {
		clean.Game.Advance(1)
		faulted.Game.Advance(1)
		if tick == faultTick && inject != nil {
			inject(faulted.Game)
		}
		ct, cs := clean.Game.HashSnapshot()
		ft, fs := faulted.Game.HashSnapshot()
		if ct != ft {
			name, ok := reg.FirstDivergence(&statehash.Snapshot{Subs: cs}, &statehash.Snapshot{Subs: fs})
			return tick, name, ok
		}
	}
	return -1, "", false
}

func TestFirstclashDivergenceBisectionFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("3 lockstep firstclash bisection runs; full preflight gate")
	}
	const faultTick = 50
	const nTicks = 150

	// Edge 1 — induced PRNG fault: one extra draw advances only the faulted
	// game's RNG cursor. The "prng" sub-hash carries that cursor, so it diverges
	// at the injection tick and is the FIRST (only) diverging system.
	tk, sys, ok := bisect(t, func(g *api.Game) { _ = g.RandomInt(0, 1<<30) }, faultTick, nTicks)
	t.Logf("FSV prng-fault: detected=%v divergentTick=%d firstSystem=%q (injected @%d)", ok, tk, sys, faultTick)
	if !ok {
		t.Fatal("induced PRNG fault NOT detected — the divergence detector is blind")
	}
	if tk != faultTick {
		t.Fatalf("PRNG fault detected at tick %d, want the injection tick %d (detector not tick-precise)", tk, faultTick)
	}
	if sys != "prng" {
		t.Fatalf("PRNG fault bisected to %q, want \"prng\" (FirstDivergence named the wrong system)", sys)
	}

	// Edge 2 — induced entities fault: an extra unit on the faulted game only.
	// "entities" is registered earlier than every system it perturbs, so the
	// detector must name "entities" — proving it bisects to DIFFERENT systems,
	// not a hard-coded answer.
	lamp := func(g *api.Game) {
		g.CreateUnit(g.Player(0), g.UnitType("lamplighter"), api.Vec2{X: 700, Y: 700}, api.Deg(0))
	}
	tk2, sys2, ok2 := bisect(t, lamp, faultTick, nTicks)
	t.Logf("FSV entities-fault: detected=%v divergentTick=%d firstSystem=%q", ok2, tk2, sys2)
	if !ok2 || tk2 != faultTick {
		t.Fatalf("entities fault: detected=%v tick=%d (want true @%d)", ok2, tk2, faultTick)
	}
	if sys2 != "entities" {
		t.Fatalf("entities fault bisected to %q, want \"entities\"", sys2)
	}
	if sys2 == sys {
		t.Fatalf("both faults bisected to %q — detector is not discriminating systems", sys)
	}

	// Edge 3 — clean control (no injection): the detector must stay SILENT across
	// the whole run. A false positive here would make every real divergence
	// untrustworthy.
	tk3, sys3, ok3 := bisect(t, nil, faultTick, nTicks)
	t.Logf("FSV clean-control: detected=%v tick=%d system=%q (want no divergence)", ok3, tk3, sys3)
	if ok3 {
		t.Fatalf("clean run reported a divergence (%q @tick %d) — false positive, fault leaked or hash nondeterministic", sys3, tk3)
	}
	t.Logf("FSV #650: divergence detector catches the induced fault, names the FIRST system (prng / entities), tick-precise, no false positives")
}
