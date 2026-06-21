package litd

// FSV for #475: the programmable-combat API surface — ReplaceDamageStage /
// SetDamageFormula / SetArmorReduction and the full read/write DamageEvent.
// SoT = target HP after apply + the event fields observed in the stage fn.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// dmgFormulaGame builds a world with named types + a matrix and a victim at
// full life with the given armor, wired to a Game for the api surface.
func dmgFormulaGame(t *testing.T, attack, armor []string, matrix [][]int32, victimArmorType uint8, armorValue int16) (*sim.World, *Game, sim.EntityID, sim.EntityID) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{})
	if err := w.BindDamageTypes(attack, armor); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	if err := w.BindDamageMatrix(matrix); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	g := newGame(w)
	mk := func(x int32, av int16, at uint8) sim.EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok || !w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, av, at) || !w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	return w, g, mk(100, armorValue, victimArmorType), mk(200, 0, 0)
}

func fire(w *sim.World, src, dst sim.EntityID, raw fixed.F64) {
	w.OnCombatPhase = func(uint32) {
		w.QueueDamage(sim.DamagePacket{Source: src, Target: dst, Amount: raw})
		w.OnCombatPhase = nil
	}
	w.Step()
}

// TestAPIReplaceDamageStageIgnoreArmorFSV — a script stage replaces
// armor-reduction with a no-op; 100 at an armored target (coeff 1000) → 100.
func TestAPIReplaceDamageStageIgnoreArmorFSV(t *testing.T) {
	w, g, victim, attacker := dmgFormulaGame(t, []string{"normal"}, []string{"heavy"}, [][]int32{{1000}}, 0, 5)
	if err := g.ReplaceDamageStage("armor-reduction", func(e *DamageEvent) { /* ignore armor */ }); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	fire(w, attacker, victim, 100*fixed.One)
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #475 ignore-armor: raw 100, armor 5 → delta=%d want=%d", delta, 100*fixed.One)
	if delta != 100*fixed.One {
		t.Fatalf("ignore-armor delta = %d, want 100 (armor stage no-op'd)", delta)
	}
}

// TestAPISetAttackTypeMidPipelineFSV — a script stage sets attack-type→holy and
// re-applies the coefficient; the holy row coefficient lands.
func TestAPISetAttackTypeMidPipelineFSV(t *testing.T) {
	attack := []string{"normal", "holy"}
	armor := []string{"unarmored"}
	// normal vs unarmored = 1000 (100%); holy vs unarmored = 2000 (200%).
	matrix := [][]int32{{1000}, {2000}}
	w, g, victim, attacker := dmgFormulaGame(t, attack, armor, matrix, 0, 0)

	var sawAttack string
	// Replace coeff-lookup: read the original attack type, switch to holy, apply.
	if err := g.ReplaceDamageStage("coeff-lookup", func(e *DamageEvent) {
		sawAttack = e.AttackType()
		if !e.SetAttackType("holy") {
			t.Error("SetAttackType(holy) returned false")
		}
		e.ApplyCoefficient()
	}); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	fire(w, attacker, victim, 50*fixed.One) // 50 raw, holy 200% → 100
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #475 mid-pipeline holy: sawAttack=%q raw 50 → delta=%d want=%d", sawAttack, delta, 100*fixed.One)
	if sawAttack != "normal" {
		t.Fatalf("stage read attack type %q, want normal", sawAttack)
	}
	if delta != 100*fixed.One {
		t.Fatalf("holy delta = %d, want 100 (50·2000/1000)", delta)
	}
}

// TestAPIDamageEventSetZeroFSV — a stage that sets the amount to 0 → no HP loss.
func TestAPIDamageEventSetZeroFSV(t *testing.T) {
	w, g, victim, attacker := dmgFormulaGame(t, []string{"normal"}, []string{"unarmored"}, [][]int32{{1000}}, 0, 0)
	if err := g.ReplaceDamageStage("script-modifier", func(e *DamageEvent) { e.SetAmount(0) }); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	fire(w, attacker, victim, 80*fixed.One)
	after := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #475 set-zero: before=%d after=%d", before, after)
	if after != before {
		t.Fatalf("HP changed (%d→%d) after SetAmount(0)", before, after)
	}
}

// TestAPIDamageEventInvalidWriteFSV — an unknown attack-type name is refused
// (fail-closed), leaving the hit unchanged.
func TestAPIDamageEventInvalidWriteFSV(t *testing.T) {
	w, g, victim, attacker := dmgFormulaGame(t, []string{"normal"}, []string{"unarmored"}, [][]int32{{1000}}, 0, 0)
	var ok bool
	var rawSeen, flagsSeen float64
	if err := g.ReplaceDamageStage("coeff-lookup", func(e *DamageEvent) {
		rawSeen = e.RawAmount()
		flagsSeen = float64(e.Flags())
		ok = e.SetAttackType("does-not-exist") // must fail closed → false
		e.ApplyCoefficient()                   // attack type unchanged → normal coeff
	}); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	fire(w, attacker, victim, 60*fixed.One) // normal 100% → 60
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #475 invalid write: SetAttackType ok=%v rawSeen=%.0f flags=%.0f delta=%d", ok, rawSeen, flagsSeen, delta)
	if ok {
		t.Fatal("SetAttackType(unknown) returned true — must fail closed")
	}
	if rawSeen != 60 {
		t.Fatalf("RawAmount read = %.0f, want 60", rawSeen)
	}
	if delta != 60*fixed.One {
		t.Fatalf("delta = %d, want 60 (attack type unchanged after failed write)", delta)
	}
}

// TestAPIProgrammableCombatValidationFSV — setup verbs fail closed.
func TestAPIProgrammableCombatValidationFSV(t *testing.T) {
	_, g, _, _ := dmgFormulaGame(t, []string{"normal"}, []string{"unarmored"}, [][]int32{{1000}}, 0, 0)
	if err := g.ReplaceDamageStage("nonexistent", func(*DamageEvent) {}); err == nil {
		t.Fatal("ReplaceDamageStage unknown stage accepted")
	}
	if err := g.ReplaceDamageStage("clamp", nil); err == nil {
		t.Fatal("ReplaceDamageStage nil fn accepted")
	}
	if err := g.SetDamageFormula(nil); err == nil {
		t.Fatal("SetDamageFormula empty accepted")
	}
	if err := g.SetDamageFormula([]DamageStageSpec{{Name: "", Fn: func(*DamageEvent) {}}}); err == nil {
		t.Fatal("SetDamageFormula empty-name accepted")
	}
	if err := g.SetArmorReduction(0); err == nil {
		t.Fatal("SetArmorReduction(0) accepted")
	}
	t.Log("FSV #475 validation: unknown stage / nil fn / empty formula / bad coefficient refused")
}
