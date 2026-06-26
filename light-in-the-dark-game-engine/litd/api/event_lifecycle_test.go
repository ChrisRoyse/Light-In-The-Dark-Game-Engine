package litd

// FSV for the lifecycle-event API surface (#470): the new public EventKinds,
// the simKindOf mappings, the convenience binders (OnAbilityCast/OnAttack/
// OnBuffApplied), and the payload accessors. SoT = a registered handler's
// observed side-effect (it captures the event payload), plus the simKindOf
// table and the const ABI values.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// lifecycleWorld builds a 2-unit world (caster/attacker/applier + target).
func lifecycleWorld(t *testing.T) (*sim.World, *Game, sim.EntityID, sim.EntityID) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 4, PendingEvents: 16})
	g := newGame(w)
	a, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(10), Y: fixed.FromInt(10)}, 0)
	if !ok {
		t.Fatal("CreateUnit a failed")
	}
	b, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(20), Y: fixed.FromInt(10)}, 1)
	if !ok {
		t.Fatal("CreateUnit b failed")
	}
	return w, g, a, b
}

// TestLifecycleOnAbilityCastDelivers — OnAbilityCast fires for a synthetic
// EvAbilityCast with the caster, target, and ability ref intact.
func TestLifecycleOnAbilityCastDelivers(t *testing.T) {
	w, g, caster, target := lifecycleWorld(t)

	var got *Event
	g.OnAbilityCast(func(e Event) { c := e; got = &c })

	const ref = 7
	w.Emit(sim.Event{Kind: sim.EvAbilityCast, Src: caster, Dst: target, Arg: int64(ref)})
	w.Step()

	if got == nil {
		t.Fatal("OnAbilityCast handler never fired")
	}
	t.Logf("FSV ability: Unit=%#x Target=%#x Ability=%d", uint32(got.Unit().id), uint32(got.Target().id), got.Ability())
	if got.Unit().id != caster {
		t.Fatalf("Unit() = %#x, want caster %#x", uint32(got.Unit().id), uint32(caster))
	}
	if got.Target().id != target {
		t.Fatalf("Target() = %#x, want target %#x", uint32(got.Target().id), uint32(target))
	}
	if got.Ability() != AbilityRef(ref) {
		t.Fatalf("Ability() = %d, want %d", got.Ability(), ref)
	}
}

// TestLifecycleOnAttackDelivers — OnAttack fires for a synthetic
// EvAttackLaunch; attacker is Unit()/Source(), victim is Target().
func TestLifecycleOnAttackDelivers(t *testing.T) {
	w, g, attacker, victim := lifecycleWorld(t)

	var got *Event
	g.OnAttack(func(e Event) { c := e; got = &c })

	w.Emit(sim.Event{Kind: sim.EvAttackLaunch, Src: attacker, Dst: victim, Arg: 0})
	w.Step()

	if got == nil {
		t.Fatal("OnAttack handler never fired")
	}
	t.Logf("FSV attack: Unit=%#x Source=%#x Target=%#x", uint32(got.Unit().id), uint32(got.Source().id), uint32(got.Target().id))
	if got.Unit().id != attacker || got.Source().id != attacker {
		t.Fatalf("attacker mismatch: Unit=%#x Source=%#x, want %#x", uint32(got.Unit().id), uint32(got.Source().id), uint32(attacker))
	}
	if got.Target().id != victim {
		t.Fatalf("Target() = %#x, want victim %#x", uint32(got.Target().id), uint32(victim))
	}
}

// TestLifecycleOnBuffAppliedDelivers — OnBuffApplied fires for a synthetic
// EvBuffApplied; the buffed unit is Unit() (Dst), the applier is Source() (Src).
func TestLifecycleOnBuffAppliedDelivers(t *testing.T) {
	w, g, applier, buffed := lifecycleWorld(t)

	var got *Event
	g.OnBuffApplied(func(e Event) { c := e; got = &c })

	w.Emit(sim.Event{Kind: sim.EvBuffApplied, Src: applier, Dst: buffed, Arg: 0})
	w.Step()

	if got == nil {
		t.Fatal("OnBuffApplied handler never fired")
	}
	t.Logf("FSV buff: Unit=%#x Source=%#x", uint32(got.Unit().id), uint32(got.Source().id))
	if got.Unit().id != buffed {
		t.Fatalf("Unit() = %#x, want buffed unit %#x", uint32(got.Unit().id), uint32(buffed))
	}
	if got.Source().id != applier {
		t.Fatalf("Source() = %#x, want applier %#x", uint32(got.Source().id), uint32(applier))
	}
}

// TestLifecycleSimKindOfMappings — every new public kind maps to the right sim
// kind (the registration-time join, fail-closed on unknown).
func TestLifecycleSimKindOfMappings(t *testing.T) {
	want := map[EventKind]uint16{
		EventAbilityCast:         sim.EvAbilityCast,
		EventAbilityEffect:       sim.EvAbilityEffect,
		EventAbilityChannelStart: sim.EvAbilityChannelStart,
		EventAbilityChannelStop:  sim.EvAbilityChannelStop,
		EventAbilityFinish:       sim.EvAbilityFinish,
		EventAbilityStopped:      sim.EvAbilityStopped,
		EventAttackLaunch:        sim.EvAttackLaunch,
		EventAttackLanded:        sim.EvAttackLanded,
		EventBuffApplied:         sim.EvBuffApplied,
		EventBuffRefreshed:       sim.EvBuffRefreshed,
	}
	for k, sk := range want {
		got, ok := simKindOf[k]
		if !ok || got != sk {
			t.Fatalf("simKindOf[%d] = %d (ok=%v), want %d", k, got, ok, sk)
		}
	}
}

// TestLifecycleEventKindABIStable — the public ABI is append-only: the existing
// kinds keep their numbers and the new kinds occupy 26–35.
func TestLifecycleEventKindABIStable(t *testing.T) {
	// existing kinds (must never renumber)
	stable := map[EventKind]uint16{
		EventUnitDeath: 1, EventUnitDamaged: 2, EventOrderIssued: 3,
		EventConstructCancelled: 25,
	}
	for k, v := range stable {
		if uint16(k) != v {
			t.Fatalf("ABI BREAK: kind %d renumbered, want %d", k, v)
		}
	}
	// new kinds appended at 26..35
	newKinds := []EventKind{
		EventAbilityCast, EventAbilityEffect, EventAbilityChannelStart,
		EventAbilityChannelStop, EventAbilityFinish, EventAbilityStopped,
		EventAttackLaunch, EventAttackLanded, EventBuffApplied, EventBuffRefreshed,
	}
	for i, k := range newKinds {
		if uint16(k) != uint16(26+i) {
			t.Fatalf("new kind[%d] = %d, want %d (append-only)", i, k, 26+i)
		}
	}
	t.Logf("ABI: existing 1..25 stable; new kinds %d..%d", newKinds[0], newKinds[len(newKinds)-1])
}

// TestLifecycleUnmappedKindFailsClosed — OnEvent with a kind absent from
// simKindOf returns a zero Subscription (never a silently-dead registration).
func TestLifecycleUnmappedKindFailsClosed(t *testing.T) {
	_, g, _, _ := lifecycleWorld(t)
	sub := g.OnEvent(EventKind(9999), func(Event) {})
	if sub != (Subscription{}) {
		t.Fatal("OnEvent accepted an unmapped kind — must fail closed")
	}
	t.Log("FSV: unmapped kind 9999 → zero Subscription (fail-closed)")
}
