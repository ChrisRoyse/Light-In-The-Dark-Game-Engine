package litd

// #217 Unit.Damage FSV. SoT = the sim Healths.Life store bytes for the victim,
// read directly before and after the combat phase applies the queued packet.
// X+X discipline: a 1000-per-mille (100%) matrix means 40 damage on a 100-life
// victim must leave exactly 60.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func combatWorld(t *testing.T) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	// 1x1 identity matrix: attack-type 0 vs armor-type 0 = 100% (1000 per-mille).
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	return w, newGame(w)
}

func mkCombatUnit(t *testing.T, w *sim.World, x int32, life float64) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
	if !ok || !w.Healths.Add(w.Ents, id, fromFloat(life), 0, 0, 0) || !w.Combats.Add(w.Ents, id) {
		t.Fatal("combat unit spawn failed")
	}
	return id
}

// TestUnitDamageAppliesFSV — happy path: 40 damage on a 100-life victim leaves
// 60 (read straight from Healths.Life).
func TestUnitDamageAppliesFSV(t *testing.T) {
	w, g := combatWorld(t)
	vid := mkCombatUnit(t, w, 100, 100)
	aid := mkCombatUnit(t, w, 200, 100)
	attacker := Unit{id: aid, g: g}
	victim := Unit{id: vid, g: g}
	hr := w.Healths.Row(vid)

	before := w.Healths.Life[hr]
	t.Logf("FSV before: victim life=%v (=%.0f)", before, toFloat(before))
	if !attacker.Damage(victim, 40) {
		t.Fatal("Damage returned false on a valid target")
	}
	w.Step() // combat phase applies the queued packet
	after := w.Healths.Life[hr]
	t.Logf("FSV after Damage(40): victim life=%v (=%.0f, want 60)", after, toFloat(after))
	if after != fromFloat(60) {
		t.Fatalf("Damage wrong: life %v -> %v, want 60", before, after)
	}
}

// TestUnitDamageEdgesFSV — edge cases: non-positive amount, item/invalid/dead
// target, and zero-value source are all no-ops that never change victim life.
func TestUnitDamageEdgesFSV(t *testing.T) {
	w, g := combatWorld(t)
	vid := mkCombatUnit(t, w, 100, 100)
	aid := mkCombatUnit(t, w, 200, 100)
	attacker := Unit{id: aid, g: g}
	victim := Unit{id: vid, g: g}
	hr := w.Healths.Row(vid)
	start := w.Healths.Life[hr]

	// (1) non-positive amount: no packet.
	if attacker.Damage(victim, 0) || attacker.Damage(victim, -5) {
		t.Fatal("non-positive damage should return false")
	}
	// (2) nil/zero-value widget target: no packet.
	if attacker.Damage(Unit{}, 40) {
		t.Fatal("zero-value target should return false")
	}
	// (3) item target (no health row): rejected.
	if attacker.Damage(Item{}, 40) {
		t.Fatal("item target should return false")
	}
	// (4) zero-value source: safe no-op.
	var zero Unit
	if zero.Damage(victim, 40) {
		t.Fatal("zero-value source should return false")
	}
	w.Step()
	end := w.Healths.Life[hr]
	t.Logf("FSV edges: victim life start=%v end=%v (want equal)", start, end)
	if start != end {
		t.Fatalf("a no-op edge case changed life: %v -> %v", start, end)
	}

	// (5) dead target: queue against a removed unit applies to nothing.
	victim.Remove()
	if attacker.Damage(victim, 40) { // victim now invalid -> false
		t.Fatal("dead target should return false")
	}
}
