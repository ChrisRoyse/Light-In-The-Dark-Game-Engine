package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// allEventKinds is the public census — every kind must be registerable.
var allEventKinds = []struct {
	name string
	kind EventKind
}{
	{"UnitDeath", EventUnitDeath}, {"UnitDamaged", EventUnitDamaged},
	{"OrderIssued", EventOrderIssued}, {"OrderDone", EventOrderDone},
	{"UnitTrained", EventUnitTrained}, {"ResearchFinished", EventResearchFinished},
	{"HeroLevel", EventHeroLevel}, {"ItemPickedUp", EventItemPickedUp},
	{"ConstructFinished", EventConstructFinished}, {"MissileImpact", EventMissileImpact},
	{"MissileExpired", EventMissileExpired}, {"Victory", EventVictory},
	{"Defeat", EventDefeat}, {"RegionEnter", EventRegionEnter},
	{"RegionLeave", EventRegionLeave}, {"OrderDropped", EventOrderDropped},
	{"BuffExpired", EventBuffExpired}, {"ResourceDeposited", EventResourceDeposited},
	{"ResourceDepleted", EventResourceDepleted}, {"TrainRefused", EventTrainRefused},
	{"HeroDied", EventHeroDied}, {"ItemUsed", EventItemUsed},
	{"ItemDropped", EventItemDropped}, {"ConstructStarted", EventConstructStarted},
	{"ConstructCancelled", EventConstructCancelled},
}

// TestEventTaxonomy — census: every public event kind maps to a sim kind
// and is registerable; an unknown kind is rejected fail-closed. SoT: the
// returned Subscription validity per kind.
func TestEventTaxonomy(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	registered := 0
	for _, ek := range allEventKinds {
		sub := g.OnEvent(ek.kind, func(Event) {})
		ok := sub.s != nil
		t.Logf("FSV kind %-18s registerable=%v", ek.name, ok)
		if !ok {
			t.Fatalf("event kind %s (%d) not registerable — missing simKindOf entry", ek.name, ek.kind)
		}
		registered++
	}
	t.Logf("FSV event-kind census: %d kinds registerable", registered)
	if registered != len(allEventKinds) {
		t.Fatalf("census = %d, want %d", registered, len(allEventKinds))
	}
	// Unknown kind rejected (fail-closed): zero-value Subscription.
	bad := g.OnEvent(EventKind(60000), func(Event) {})
	t.Logf("FSV unknown kind 60000 -> subscription valid=%v (want false)", bad.s != nil)
	if bad.s != nil {
		t.Fatalf("unknown kind was accepted")
	}
}

// TestEventSubscriptionAccessorFSV — Event.Subscription() (GetTriggeringTrigger):
// a handler cancels its own registration mid-dispatch; it must not fire on
// the next emit. SoT: the fire count across two deaths.
func TestEventSubscriptionAccessorFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)
	a := bareUnit(t, w)
	b := bareUnit(t, w)

	fires := 0
	gotSelf := false
	var sub Subscription
	sub = g.OnEvent(EventUnitDeath, func(e Event) {
		fires++
		// Event.Subscription() returns this very registration.
		if e.Subscription().s == sub.s && sub.s != nil {
			gotSelf = true
		}
		e.Subscription().Cancel() // cancel self after first fire
	})

	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(a)
			w.KillUnit(b)
		}
	}
	w.Step()
	t.Logf("FSV self-cancel: fires=%d gotSelf=%v (want fires=1)", fires, gotSelf)
	if fires != 1 {
		t.Fatalf("self-cancel via Event.Subscription() failed: fires=%d, want 1", fires)
	}
	if !gotSelf {
		t.Fatalf("Event.Subscription() did not return the firing registration")
	}
}

// TestEventWrongPayloadKindFSV — edge (4): accessors on the wrong event
// kind return zero-value handles, never a foreign entity or a panic. SoT:
// each accessor's zero-ness on a death event.
func TestEventWrongPayloadKindFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	u := bareUnit(t, w)

	var checked bool
	g.OnEvent(EventUnitDeath, func(e Event) {
		checked = true
		t.Logf("FSV on death event: Region.zero=%v Source.zero=%v Target.zero=%v Missile.zero=%v Damage=%v Player.zero=%v",
			e.Region().IsZero(), e.Source().IsZero(), e.Target().IsZero(), e.Missile().IsZero(), e.Damage(), e.Player().IsZero())
		if !e.Region().IsZero() {
			t.Errorf("Region() non-zero on death event")
		}
		if !e.Source().IsZero() {
			t.Errorf("Source() non-zero on death event")
		}
		if !e.Target().IsZero() {
			t.Errorf("Target() non-zero on death event")
		}
		if !e.Missile().IsZero() {
			t.Errorf("Missile() non-zero on death event")
		}
		if e.Damage() != 0 {
			t.Errorf("Damage() non-zero on death event")
		}
		// Unit() and KillingUnit() ARE valid on a death event.
		if e.Unit().id != u {
			t.Errorf("Unit() wrong on death event")
		}
	})
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(u)
		}
	}
	w.Step()
	if !checked {
		t.Fatalf("death handler never fired")
	}
}

// TestDamageEventAPIFSV — Game.OnDamage halves a hit; the victim's life
// drops by exactly half. SoT: the unit's Life through the sim, read before
// and after the tick.
func TestDamageEventAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	g := newGame(w)

	mk := func(x int32) sim.EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok || !w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) || !w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	victim := mk(100)
	attacker := mk(200)
	hr := w.Healths.Row(victim)

	var sawSrc, sawDst bool
	g.OnDamage(func(d *DamageEvent) {
		sawSrc = d.Source().id == attacker
		sawDst = d.Unit().id == victim
		t.Logf("FSV OnDamage: amount=%.0f srcOK=%v dstOK=%v -> set %.0f", d.Amount(), sawSrc, sawDst, d.Amount()/2)
		d.SetAmount(d.Amount() / 2)
	})

	t.Logf("FSV before: victim life=%v", w.Healths.Life[hr])
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.QueueDamage(sim.DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
		}
	}
	w.Step()
	t.Logf("FSV after: victim life=%v (want 80*One=%v) srcOK=%v dstOK=%v", w.Healths.Life[hr], 80*fixed.One, sawSrc, sawDst)
	if w.Healths.Life[hr] != 80*fixed.One {
		t.Fatalf("OnDamage halving failed: life=%v, want %v (80)", w.Healths.Life[hr], 80*fixed.One)
	}
	if !sawSrc || !sawDst {
		t.Fatalf("DamageEvent Source/Unit wrong: srcOK=%v dstOK=%v", sawSrc, sawDst)
	}
}
