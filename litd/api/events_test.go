package litd

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// bareUnit creates an entity with no component rows beyond the
// transform — enough to be killed and named in a death event.
func bareUnit(t *testing.T, w *sim.World) sim.EntityID {
	t.Helper()
	var face fixed.Angle
	id, ok := w.CreateUnit(fixed.Vec2{}, face)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	return id
}

// TestEventOrder is the golden dispatch-order contract: within a tick
// handlers fire in registration order; across firings of one kind they
// fire in emit order. Two units die in one tick; the trace must equal
// (emit order × registration order) and be identical across two runs.
func TestEventOrder(t *testing.T) {
	run := func() string {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		x := bareUnit(t, w)
		y := bareUnit(t, w)
		label := func(e Event) string {
			switch e.src {
			case x:
				return "X"
			case y:
				return "Y"
			}
			return "?"
		}
		var trace []string
		for _, name := range []string{"A", "B", "C"} {
			n := name
			g.OnEvent(EventUnitDeath, func(e Event) {
				trace = append(trace, label(e)+":"+n)
			})
		}
		w.OnCombatPhase = func(tick uint32) {
			if tick == 1 {
				w.KillUnit(x) // emit order: X before Y
				w.KillUnit(y)
			}
		}
		w.Step()
		return strings.Join(trace, " ")
	}
	r1 := run()
	r2 := run()
	t.Logf("run1: %s", r1)
	t.Logf("run2: %s", r2)
	const golden = "X:A X:B X:C Y:A Y:B Y:C"
	if r1 != golden {
		t.Fatalf("dispatch order:\n got  %q\n want %q", r1, golden)
	}
	if r1 != r2 {
		t.Fatalf("nondeterministic dispatch: run1=%q run2=%q", r1, r2)
	}
}

// TestEventMidDispatchRegistration — edge (2): a handler that registers
// another handler for the same event mid-dispatch must not fire the new
// one this tick; it joins from the next firing.
func TestEventMidDispatchRegistration(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	x := bareUnit(t, w)
	y := bareUnit(t, w)

	dFires := 0
	registered := false
	g.OnEvent(EventUnitDeath, func(e Event) {
		if !registered {
			registered = true
			g.OnEvent(EventUnitDeath, func(Event) { dFires++ })
		}
	})
	w.OnCombatPhase = func(tick uint32) {
		switch tick {
		case 1:
			w.KillUnit(x)
		case 2:
			w.KillUnit(y)
		}
	}

	w.Step() // tick 1: A fires for X, registers D; D must NOT fire this tick
	afterT1 := dFires
	w.Step() // tick 2: D fires for Y
	afterT2 := dFires
	t.Logf("D fire count: after tick1=%d after tick2=%d", afterT1, afterT2)
	if afterT1 != 0 {
		t.Fatalf("mid-dispatch-registered handler fired same tick: %d, want 0", afterT1)
	}
	if afterT2 != 1 {
		t.Fatalf("mid-dispatch-registered handler fired %d times by tick2, want 1", afterT2)
	}
}

// TestEventCancelInHandler — edge (3): a subscription that cancels
// itself inside its handler fires zero times afterward.
func TestEventCancelInHandler(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	x := bareUnit(t, w)
	y := bareUnit(t, w)

	fires := 0
	var sub Subscription
	sub = g.OnEvent(EventUnitDeath, func(Event) {
		fires++
		sub.Cancel()
	})
	w.OnCombatPhase = func(tick uint32) {
		switch tick {
		case 1:
			w.KillUnit(x)
		case 2:
			w.KillUnit(y)
		}
	}
	w.Step() // fires once, then cancels
	afterT1 := fires
	w.Step() // cancelled: must not fire
	afterT2 := fires
	t.Logf("self-cancelling handler fires: after tick1=%d after tick2=%d", afterT1, afterT2)
	if afterT1 != 1 || afterT2 != 1 {
		t.Fatalf("self-cancel wrong: fires %d/%d, want 1/1", afterT1, afterT2)
	}
}

// TestEventFilterImpurity — edge (4): a filter that mutates a captured
// local returns different results across the debug double-run and is
// reported.
func TestEventFilterImpurity(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	x := bareUnit(t, w)

	var warnings []string
	g.OnInvalidHandle(func(s string) { warnings = append(warnings, s) })
	g.SetDebug(true)

	counter := 0
	fires := 0
	g.OnEvent(EventUnitDeath, func(Event) { fires++ },
		Where(func(EventView) bool {
			counter++
			return counter%2 == 0 // impure: depends on call count
		}))
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(x)
		}
	}
	w.Step()
	t.Logf("impurity warnings: %v", warnings)
	if len(warnings) != 1 || !strings.Contains(warnings[0], "impure") {
		t.Fatalf("expected 1 impurity warning, got %v", warnings)
	}
}

// TestEventPayload checks per-kind accessors and the R-API-5 wrong-kind
// degradation. A damage event carries victim/attacker/amount; accessors
// for other kinds return zero handles.
func TestEventPayload(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	attacker, _ := liveUnit(t, w, g, 0, 100)
	victim, vID := liveUnit(t, w, g, 1, 100)

	var got Event
	fired := false
	g.OnEvent(EventUnitDamaged, func(e Event) { got = e; fired = true })

	const dmg = 42
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.Emit(sim.Event{Kind: 7 /* EvUnitDamaged */, Src: attacker.id, Dst: vID, Arg: int64(fixed.FromInt(dmg))})
		}
	}
	w.Step()
	if !fired {
		t.Fatal("damage handler did not fire")
	}
	t.Logf("payload: Kind=%d Unit.valid=%v Source.valid=%v Damage=%v KillingUnit.zero=%v Target.zero=%v",
		got.Kind(), got.Unit().Valid(), got.Source().Valid(), got.Damage(),
		got.KillingUnit().IsZero(), got.Target().IsZero())

	if got.Kind() != EventUnitDamaged {
		t.Errorf("Kind = %d, want EventUnitDamaged", got.Kind())
	}
	if got.Unit().id != vID {
		t.Errorf("Unit() = %#x, want victim %#x", uint32(got.Unit().id), uint32(vID))
	}
	if got.Source().id != attacker.id {
		t.Errorf("Source() = %#x, want attacker %#x", uint32(got.Source().id), uint32(attacker.id))
	}
	if got.Damage() != dmg {
		t.Errorf("Damage() = %v, want %v", got.Damage(), float64(dmg))
	}
	// wrong-kind accessors degrade to zero (R-API-5)
	if !got.KillingUnit().IsZero() {
		t.Error("KillingUnit() on a damage event should be zero")
	}
	if !got.Target().IsZero() {
		t.Error("Target() on a damage event should be zero")
	}
	_ = victim
}

// TestEventScopeForPlayer checks ForPlayer scoping against the primary
// unit's owner.
func TestEventScopeForPlayer(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	p0Unit, p0ID := liveUnit(t, w, g, 0, 100)
	p1Unit, p1ID := liveUnit(t, w, g, 1, 100)
	_ = p0Unit
	_ = p1Unit

	var fired []string
	g.OnEvent(EventUnitDeath, func(e Event) {
		if e.src == p0ID {
			fired = append(fired, "p0")
		} else {
			fired = append(fired, "p1")
		}
	}, ForPlayer(Player{idx: 1, g: g})) // only player 1's units

	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(p0ID) // owned by player 0: must be filtered out
			w.KillUnit(p1ID) // owned by player 1: must fire
		}
	}
	w.Step()
	t.Logf("ForPlayer(1) fired for: %v", fired)
	if len(fired) != 1 || fired[0] != "p1" {
		t.Fatalf("ForPlayer scope wrong: %v, want [p1]", fired)
	}
}

// TestEventUnknownKind — fail-closed: an unregistered kind yields a
// zero Subscription and a debug report, never a silent dead handler.
func TestEventUnknownKind(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })
	g.SetDebug(true)

	sub := g.OnEvent(EventKind(9999), func(Event) {})
	t.Logf("unknown-kind sub zero=%v reports=%v", sub.s == nil, reports)
	if sub.s != nil {
		t.Fatal("unknown kind returned a live subscription")
	}
	if len(reports) != 1 || !strings.Contains(reports[0], "unknown event kind") {
		t.Fatalf("expected unknown-kind report, got %v", reports)
	}
}

// TestEventDispatchZeroAlloc — the api fan-out path allocates nothing
// at steady state, with and without a filter (R-GC-3).
func TestEventDispatchZeroAlloc(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	victim, vID := liveUnit(t, w, g, 1, 100)
	_ = victim
	sink := 0
	g.OnEvent(EventUnitDamaged, func(Event) { sink++ })
	kl := g.eventKinds[7]
	if kl == nil {
		t.Fatal("kindList not registered for damage")
	}
	e := sim.Event{Kind: 7, Src: vID, Dst: vID, Arg: int64(fixed.FromInt(10))}

	if n := testing.AllocsPerRun(1000, func() { g.dispatch(kl, e) }); n != 0 {
		t.Errorf("dispatch (no filter) allocates %.1f/run, want 0", n)
	}

	// add a filter (debug off): still zero alloc
	g.OnEvent(EventUnitDamaged, func(Event) { sink++ },
		Where(func(v EventView) bool { return v.Damage() > 0 }))
	if n := testing.AllocsPerRun(1000, func() { g.dispatch(kl, e) }); n != 0 {
		t.Errorf("dispatch (with filter) allocates %.1f/run, want 0", n)
	}
	t.Logf("zero-alloc dispatch verified (no-filter + filter); sink=%d", sink)
}
