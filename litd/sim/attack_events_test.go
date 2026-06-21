package sim

// FSV for the attack-lifecycle events (#468): the auto-attack cycle now emits
// EvAttackLaunch at the FIRE edge and EvAttackLanded when a weapon-sourced
// packet lands on a live target (immediately before that packet's
// EvUnitDamaged). SoT = the captured event stream (kind/tick/src/dst/arg)
// joined against target HP and StateHash.

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// captureAttackEvents records Launch/Landed/Damaged in dispatch order, tagged
// with the tick each fired on.
func captureAttackEvents(w *World, log *[]abilityEvent) {
	w.RegisterHandler(9, func(w *World, e Event) {
		switch e.Kind {
		case EvAttackLaunch, EvAttackLanded, EvUnitDamaged:
			*log = append(*log, abilityEvent{tick: w.Tick(), kind: e.Kind, src: e.Src, dst: e.Dst, arg: e.Arg})
		}
	})
	for _, k := range []uint16{EvAttackLaunch, EvAttackLanded, EvUnitDamaged} {
		w.Subscribe(k, 9)
	}
}

func attackEvName(k uint16) string {
	switch k {
	case EvAttackLaunch:
		return "Launch"
	case EvAttackLanded:
		return "Landed"
	case EvUnitDamaged:
		return "Damaged"
	}
	return fmt.Sprintf("kind%d", k)
}

func logAttackEvents(t *testing.T, log []abilityEvent) {
	t.Helper()
	for _, e := range log {
		t.Logf("  t%d %s src=%d dst=%d arg=%d", e.tick, attackEvName(e.kind), e.src, e.dst, e.arg)
	}
}

// TestAttackLifecycleMeleeHappyPath — a melee attacker hits a dummy once: the
// stream is Launch(t11) → Landed(t11) → Damaged(t11), Landed and Damaged carry
// the same post-mitigation amount, and HP drops by exactly that amount.
func TestAttackLifecycleMeleeHappyPath(t *testing.T) {
	w, a, v, _ := traceWorld(t, 0)
	hr := w.Healths.Row(v)

	var log []abilityEvent
	captureAttackEvents(w, &log)

	hpBefore := w.Healths.Life[hr]
	for i := 0; i < 12; i++ { // FIRE at t11, lands same tick
		w.Step()
	}
	hpAfter := w.Healths.Life[hr]
	logAttackEvents(t, log)
	t.Logf("victim HP: before=%d after=%d", hpBefore, hpAfter)

	if len(log) != 3 {
		t.Fatalf("event count = %d, want 3 (Launch,Landed,Damaged): %v", len(log), log)
	}
	if log[0].kind != EvAttackLaunch || log[1].kind != EvAttackLanded || log[2].kind != EvUnitDamaged {
		t.Fatalf("order = %s,%s,%s, want Launch,Landed,Damaged",
			attackEvName(log[0].kind), attackEvName(log[1].kind), attackEvName(log[2].kind))
	}
	// all three same tick for a melee hit; Launch carries the weapon slot,
	// Landed and Damaged the same post-mitigation amount.
	if log[0].tick != log[1].tick || log[1].tick != log[2].tick {
		t.Fatalf("melee Launch/Landed/Damaged ticks differ: %d/%d/%d", log[0].tick, log[1].tick, log[2].tick)
	}
	if log[0].src != a || log[0].dst != v || log[0].arg != 0 {
		t.Fatalf("Launch payload = src%d dst%d arg%d, want src%d dst%d arg0 (slot)", log[0].src, log[0].dst, log[0].arg, a, v)
	}
	if log[1].arg != log[2].arg {
		t.Fatalf("Landed arg %d != Damaged arg %d — must correlate to the same packet", log[1].arg, log[2].arg)
	}
	if hpBefore-hpAfter != fixed.F64(log[1].arg) {
		t.Fatalf("HP delta %d != Landed amount %d", hpBefore-hpAfter, log[1].arg)
	}
	if hpAfter != 90*fixed.One {
		t.Fatalf("victim HP after = %d, want 90 (10 melee damage)", hpAfter)
	}
}

// armProjectile fills slot 0 of a's weapon as a ranged (ProjSpeed>0) weapon.
func armProjectile(w *World, a, v EntityID) {
	cr := w.Combats.Row(a)
	c := w.Combats
	c.DmgBase[cr][0] = 10
	c.Cooldown[cr][0] = 27
	c.DamagePt[cr][0] = 10
	c.Backswing[cr][0] = 10
	c.Range[cr][0] = 500 * fixed.One
	c.ProjSpeed[cr][0] = 100 * fixed.One
	c.Target[cr] = v
}

// TestAttackLifecycleProjectileTiming — a ranged weapon launches at the FIRE
// edge; Landed trails by the missile flight time (gap > 0).
func TestAttackLifecycleProjectileTiming(t *testing.T) {
	w, a, v := msWorld(t)
	armProjectile(w, a, v)

	var log []abilityEvent
	captureAttackEvents(w, &log)
	for i := 0; i < 30; i++ {
		w.Step()
	}
	logAttackEvents(t, log)

	var launchTick, landedTick uint32
	var sawLaunch, sawLanded bool
	for _, e := range log {
		if e.kind == EvAttackLaunch && !sawLaunch {
			launchTick, sawLaunch = e.tick, true
		}
		if e.kind == EvAttackLanded && !sawLanded {
			landedTick, sawLanded = e.tick, true
		}
	}
	if !sawLaunch || !sawLanded {
		t.Fatalf("launch=%v landed=%v, want both", sawLaunch, sawLanded)
	}
	if !(landedTick > launchTick) {
		t.Fatalf("projectile Landed (t%d) must trail Launch (t%d) — flight, not instant", landedTick, launchTick)
	}
	t.Logf("projectile: Launch t%d → Landed t%d (flight gap %d ticks)", launchTick, landedTick, landedTick-launchTick)
}

// TestAttackLifecycleDeadTargetNoLanded — target dies mid-flight: the missile's
// packet finds it gone, so NO EvAttackLanded (and no EvUnitDamaged) fires.
func TestAttackLifecycleDeadTargetNoLanded(t *testing.T) {
	w, a, v := msWorld(t)
	armProjectile(w, a, v)

	var log []abilityEvent
	captureAttackEvents(w, &log)
	// step until the missile is in flight (Launch fired), then kill the target.
	for i := 0; i < 12; i++ {
		w.Step()
		if w.Missiles.Count() > 0 {
			break
		}
	}
	w.KillUnit(v)
	for i := 0; i < 20; i++ {
		w.Step()
	}
	logAttackEvents(t, log)

	var sawLaunch, sawLanded, sawDamaged bool
	for _, e := range log {
		switch e.kind {
		case EvAttackLaunch:
			sawLaunch = true
		case EvAttackLanded:
			sawLanded = true
		case EvUnitDamaged:
			sawDamaged = true
		}
	}
	if !sawLaunch {
		t.Fatal("expected EvAttackLaunch before the target died")
	}
	if sawLanded || sawDamaged {
		t.Fatalf("dead target: landed=%v damaged=%v, want neither (packet is a no-op on a dead target)", sawLanded, sawDamaged)
	}
}

// TestAttackLifecycleDoubleRunIdentical — two identical melee runs produce a
// byte-identical event stream and identical StateHash.
func TestAttackLifecycleDoubleRunIdentical(t *testing.T) {
	reg := NewHashRegistry()
	run := func() ([]abilityEvent, uint64) {
		w, _, _, _ := traceWorld(t, 0)
		var log []abilityEvent
		captureAttackEvents(w, &log)
		for i := 0; i < 12; i++ {
			w.Step()
		}
		var s statehash.Snapshot
		w.HashState(reg, &s)
		return log, s.Top
	}
	log1, h1 := run()
	log2, h2 := run()
	t.Logf("run1 hash=%#x events=%d | run2 hash=%#x events=%d", h1, len(log1), h2, len(log2))
	if h1 != h2 {
		t.Fatalf("StateHash diverged: %#x vs %#x", h1, h2)
	}
	if len(log1) != len(log2) {
		t.Fatalf("event count diverged: %d vs %d", len(log1), len(log2))
	}
	for i := range log1 {
		if log1[i] != log2[i] {
			t.Fatalf("event[%d] diverged: %+v vs %+v", i, log1[i], log2[i])
		}
	}
}
