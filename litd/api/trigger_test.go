package litd

// Full State Verification for the public Trigger noun (#461, ADR #451).
// SoT = the sim trigger slab (the API call's effect) + Game state hash +
// the action's own side effect. Happy path + the four mandated edges.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// ownedUnit creates a bare unit and assigns it to a player slot so
// owner-scoped triggers have something to test.
func ownedUnit(t *testing.T, w *sim.World, player uint8) sim.EntityID {
	t.Helper()
	id := bareUnit(t, w)
	if !w.Owners.Add(w.Ents, id, player, player, player) {
		t.Fatal("Owners.Add failed")
	}
	return id
}

// TestTriggerAPIOwnerFilter — happy path: a death trigger gated on
// OwnerIs(P1) fires once when a P1 unit and a P2 unit both die.
func TestTriggerAPIOwnerFilter(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	uP1 := ownedUnit(t, w, 1)
	uP2 := ownedUnit(t, w, 2)

	fired := 0
	tr := g.NewTrigger().On(EventUnitDeath).When(OwnerIs(g.Player(1))).Do(func(e Event) { fired++ })
	t.Logf("sim slab after build: Count=%d events=%v actions=%v cond=%v",
		w.Triggers.Count(), w.Triggers.Events(tr.id), w.Triggers.Actions(tr.id), w.Triggers.Condition(tr.id))

	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(uP1)
			w.KillUnit(uP2)
		}
	}
	w.Step()
	t.Logf("after killing a P1 and a P2 unit: action fired %d times (want 1, P1 only)", fired)
	if fired != 1 {
		t.Fatalf("owner-filtered trigger fired %d times, want 1", fired)
	}
	if w.Triggers.Count() != 1 {
		t.Fatalf("sim slab has %d triggers, want 1", w.Triggers.Count())
	}
}

// TestTriggerAPIDestroyInvalid — edge 1: a Destroy()'d trigger handle is
// invalid; its slot may be reused, the old handle stays stale.
func TestTriggerAPIDestroyInvalid(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	tr := g.NewTrigger().On(EventUnitDeath).Do(func(Event) {})
	t.Logf("created: Valid=%v", tr.Valid())
	if !tr.Valid() {
		t.Fatal("fresh trigger invalid")
	}
	tr.Destroy()
	t.Logf("after Destroy: Valid=%v (want false)", tr.Valid())
	if tr.Valid() {
		t.Fatal("destroyed trigger still valid")
	}
	// reuse the slot — old handle must remain stale.
	tr2 := g.NewTrigger()
	if tr.Valid() {
		t.Fatal("stale handle aliases the reused trigger slot")
	}
	if !tr2.Valid() {
		t.Fatal("reused trigger handle invalid")
	}
}

// TestTriggerAPIScopeOwnedBy — edge 2: On(..., OwnedBy(P1)) fires only on
// that player's events.
func TestTriggerAPIScopeOwnedBy(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	uP1 := ownedUnit(t, w, 1)
	uP2 := ownedUnit(t, w, 2)
	fired := 0
	g.NewTrigger().On(EventUnitDeath, OwnedBy(g.Player(1))).Do(func(Event) { fired++ })
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(uP1)
			w.KillUnit(uP2)
		}
	}
	w.Step()
	t.Logf("OwnedBy(P1) scope, P1+P2 die: fired %d (want 1)", fired)
	if fired != 1 {
		t.Fatalf("OwnedBy scope fired %d, want 1", fired)
	}
}

// TestTriggerAPINoOpBuild — edge 3: a trigger with no On() never fires.
func TestTriggerAPINoOpBuild(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u := ownedUnit(t, w, 1)
	fired := 0
	g.NewTrigger().Do(func(Event) { fired++ }) // no On — no event registered
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(u)
		}
	}
	w.Step()
	t.Logf("no-On trigger, a unit dies: fired %d (want 0)", fired)
	if fired != 0 {
		t.Fatal("a trigger with no event registration fired")
	}
}

// TestTriggerAPIvsSimParity — edge 4: building via the API produces the
// identical sim slab + state hash as the equivalent direct sim build.
func TestTriggerAPIvsSimParity(t *testing.T) {
	reg := sim.NewHashRegistry()

	// Path A: build via the public API.
	wa := sim.NewWorld(sim.Caps{Units: 16})
	ga := newGame(wa)
	ga.NewTrigger().On(EventUnitDeath).When(OwnerIs(ga.Player(1))).Do(func(Event) {})
	var ha statehash.Snapshot
	wa.HashState(reg, &ha)

	// Path B: the equivalent direct sim build — same handler names in the
	// same order (cond seq 0, act seq 1), same event/condition/action.
	wb := sim.NewWorld(sim.Caps{Units: 16})
	condRef := wb.RegisterHandlerID("api.trig.cond.0", func(w *sim.World, e sim.Event) bool {
		return w.Owners.Player[maxInt32(w.Owners.Row(e.Src), 0)] == 1 // mirror only — refs are what hash
	})
	actRef := wb.RegisterHandlerID("api.trig.act.1", func(w *sim.World, e sim.Event) bool { return true })
	tb, _ := wb.Triggers.New()
	wb.Triggers.AddEvent(tb, sim.EventReg{Kind: sim.EvUnitDeath})
	wb.Triggers.SetCondition(tb, wb.Cond(condRef))
	wb.Triggers.AddAction(tb, actRef)
	var hb statehash.Snapshot
	wb.HashState(reg, &hb)

	t.Logf("API slab: triggers-sub=%016x handlers-sub=%016x top=%016x",
		subOfReg(reg, &ha, "triggers"), subOfReg(reg, &ha, "handlers"), ha.Top)
	t.Logf("sim slab: triggers-sub=%016x handlers-sub=%016x top=%016x",
		subOfReg(reg, &hb, "triggers"), subOfReg(reg, &hb, "handlers"), hb.Top)
	if ha.Top != hb.Top {
		t.Fatalf("API build and direct sim build diverge: %016x != %016x", ha.Top, hb.Top)
	}
}

func maxInt32(a, b int32) int32 {
	if a > b {
		return a
	}
	return b
}

// subOfReg returns a named system's sub-hash from a snapshot.
func subOfReg(reg *statehash.Registry, s *statehash.Snapshot, name string) uint64 {
	for i, n := range reg.Names() {
		if n == name {
			return s.Subs[i]
		}
	}
	return 0
}
