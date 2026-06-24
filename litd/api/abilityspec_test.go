package litd

// #599 — the Go authoring surface produces a WORKING ability, not just an
// identical fingerprint: a composable spec registered through Game.Register
// AbilitySpec, granted to a unit, and cast through the normal cast machine
// runs its ops (here attach_mover) at the EFFECT edge. SoT = the live mover
// count after the cast resolves.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestRegisterAbilitySpecCastsThroughMachine(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8, Movers: 8, RuntimeAbilityDefs: 8})
	g := newGame(w)

	ref, err := g.RegisterAbilitySpec(AbilitySpecDef{
		ID: "dart", Name: "Dart", CastType: "active",
		CastRange: 900, ManaCost: 10, Cooldown: 1.0, CastPoint: 0.05, // 1-tick castpoint
		OnCast: []AbilityOpDef{
			{Op: "attach_mover", Mover: "linear", Speed: 20, Range: 400, Radius: 16},
		},
	})
	if err != nil || ref == 0 {
		t.Fatalf("RegisterAbilitySpec: ref=%d err=%v", ref, err)
	}

	caster, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	w.Healths.Add(w.Ents, caster, 100*fixed.One, 0, 0, 0)
	w.Orders.Add(w.Ents, caster)
	target, _ := w.CreateUnit(fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, target, 2, 2, 2)
	w.Healths.Add(w.Ents, target, 100*fixed.One, 0, 0, 0)
	w.Orders.Add(w.Ents, target)

	cu := Unit{id: caster, g: g}
	tu := Unit{id: target, g: g}
	ab := cu.AddAbility(ref)
	cu.SetMaxMana(100)
	cu.SetMana(100)

	before := w.Movers.Count()
	t.Logf("BEFORE cast: movers=%d mana=%.0f", before, cu.Mana())
	if !cu.Cast(ab, tu) {
		t.Fatal("Cast returned false")
	}
	for i := 0; i < 5; i++ {
		w.Step()
	}
	after := w.Movers.Count()
	t.Logf("AFTER cast: movers=%d mana=%.0f", after, cu.Mana())
	if after <= before {
		t.Fatalf("composable ability did not spawn its mover at the EFFECT edge: movers %d -> %d", before, after)
	}
	if cu.Mana() != 90 {
		t.Fatalf("mana=%.0f, want 90 (100-10 cost) — cast machine did not run", cu.Mana())
	}
}
