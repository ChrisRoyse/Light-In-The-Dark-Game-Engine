package litd

// Mana-regen buff-stat FSV (#522). SoT = Unit.Mana() before/after stepping the
// sim. Proves the new life-regen-twin stat actually folds into the ability
// system's per-tick mana fill (not a fail-open) and reverts on removal, while a
// unit with no mana-regen source is untouched (the determinism-golden-safety
// property).

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// manaWorld binds a "focus" buff whose mana-regen Add is addPerTick mana/tick.
func manaWorld(t *testing.T, addPerTick int64) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8, RuntimeAbilityDefs: 8})
	defs := []data.BuffType{
		{ID: "focus", DurationTicks: 10000, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatManaRegen, Add: addPerTick, Permille: 1000}}},
	}
	if !w.BindBuffTypes(defs) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, newGame(w)
}

// manaUnit spawns a unit with an ability (hence a mana pool), max mana 100,
// current mana 0, and zero base mana-regen.
func manaUnit(t *testing.T, w *sim.World, g *Game, x int32) Unit {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	u := Unit{id: id, g: g}
	// Unique ability id per unit — two units on one world must not collide on
	// the ability registry.
	ref := g.RegisterAbility(AbilityDef{ID: fmt.Sprintf("spell%d", x), Name: "Spell", ManaCost: 1, Cooldown: 1.0})
	if ref == 0 || !u.AddAbility(ref).Valid() {
		t.Fatal("AddAbility failed")
	}
	u.SetMaxMana(100)
	u.SetMana(0)
	return u
}

func stepMana(w *sim.World, n int) {
	for i := 0; i < n; i++ {
		w.Step()
	}
}

// TestManaRegenBuffAppliesFSV (#522): a mana-regen buff fills a unit's mana
// (base regen 0); a no-buff control stays empty; removal stops the fill.
func TestManaRegenBuffAppliesFSV(t *testing.T) {
	w, g := manaWorld(t, int64(5*fixed.One)) // focus = +5 mana/tick
	focus := g.BuffType("focus")
	if focus.IsZero() {
		t.Fatal("BuffType(focus) null")
	}
	u := manaUnit(t, w, g, 64)
	ctl := manaUnit(t, w, g, 200) // never buffed

	// No buff yet → no regen (base mana-regen is 0).
	stepMana(w, 10)
	t.Logf("no-buff 10 ticks: u.mana=%.0f ctl.mana=%.0f (want 0/0)", u.Mana(), ctl.Mana())
	if u.Mana() != 0 || ctl.Mana() != 0 {
		t.Fatalf("mana regenerated without a source: u=%.0f ctl=%.0f", u.Mana(), ctl.Mana())
	}

	// Apply focus (+5/tick) to u only.
	u.ApplyBuff(focus)
	stepMana(w, 10)
	t.Logf("focus 10 ticks: u.mana=%.0f (want 50) ctl.mana=%.0f (want 0)", u.Mana(), ctl.Mana())
	if u.Mana() != 50 {
		t.Fatalf("MANA-REGEN MOD IS A NO-OP: u.mana=%.0f after 10 ticks of +5/tick, want 50 (#522 fail-open)", u.Mana())
	}
	if ctl.Mana() != 0 {
		t.Fatalf("control gained mana without a buff: %.0f", ctl.Mana())
	}

	// Fill to the cap, then confirm it clamps at MaxMana (no overshoot).
	stepMana(w, 30) // 50 + 150 unclamped
	t.Logf("focus 30 more ticks: u.mana=%.0f (want clamp 100)", u.Mana())
	if u.Mana() != 100 {
		t.Fatalf("mana did not clamp at MaxMana: %.0f, want 100", u.Mana())
	}

	// Remove focus → fill stops (drain a bit first so we can see it not refill).
	u.SetMana(60)
	u.RemoveBuff(focus)
	stepMana(w, 10)
	t.Logf("after remove + 10 ticks: u.mana=%.0f (want unchanged 60)", u.Mana())
	if u.Mana() != 60 {
		t.Fatalf("mana regenerated after buff removal: u.mana=%.0f, want 60", u.Mana())
	}
}

// TestManaRegenDeterminismFSV (#522): two identical mana-regen runs hash equal.
func TestManaRegenDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := manaWorld(t, int64(3*fixed.One))
		focus := g.BuffType("focus")
		u := manaUnit(t, w, g, 64)
		u.ApplyBuff(focus)
		stepMana(w, 20)
		return g.StateHash()
	}
	h1, h2 := run(), run()
	if h1 != h2 {
		t.Fatalf("non-deterministic mana regen: h1=%#x h2=%#x", h1, h2)
	}
	t.Logf("FSV #522 determinism: two mana-regen runs identical hash=%#x", h1)
}
