package sim

// #597 FSV — data-only ability grant by id. SoT = the unit's ability slot list
// (UnitHasAbility / Abilities.AbilityID columns) AND the cast outcome (victim
// Life, mana, cooldown). A composable fireball deals a synthetic 50 damage via
// run_effects; granting then casting drops the victim 50, revoking refuses the
// cast (no damage).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// grantWorld builds a world with the 50-damage impact arena + damage matrix and
// registers a composable "fireball" (run_effects at the cast target). Returns
// the world, its ability ref, a caster (with mana) and a victim.
func grantWorld(t *testing.T) (w *World, ref uint16, caster, victim EntityID) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterEffectExec(data.EPDamage, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
		w.QueueDamage(DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 50 * fixed.One})
	})
	w = NewWorld(Caps{Units: 32})
	if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatalf("bind effects: %v", err)
	}
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	var err error
	ref, err = w.RegisterAbilitySpec(AbilitySpecSource{
		ID: "fireball", Name: "Fireball", CastType: "active",
		CastRange: 900, ManaCost: 20, Cooldown: 1.0, CastPoint: 0.1, // 20t cd, 2t castpoint
		OnCast: []OpSource{{Op: "run_effects", Effects: "impact"}},
	}, interpResolver{})
	if err != nil {
		t.Fatalf("RegisterAbilitySpec: %v", err)
	}
	caster = atkUnit(t, w, 1, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	victim = atkUnit(t, w, 2, fixed.Vec2{X: 1050 * fixed.One, Y: 1000 * fixed.One}, 0)
	return w, ref, caster, victim
}

func giveMana(w *World, unit EntityID, mana fixed.F64) {
	ar := w.Abilities.Row(unit)
	w.Abilities.Mana[ar] = mana
	w.Abilities.MaxMana[ar] = mana
}

// TestGrantThenCast: grant 'fireball' by id → unit has it → casts it → victim
// takes 50, mana spent, cooldown set. Then revoke → cannot cast (no damage).
func TestGrantThenCast(t *testing.T) {
	w, ref, caster, victim := grantWorld(t)

	if w.UnitHasAbility(caster, "fireball") {
		t.Fatal("caster has fireball before any grant")
	}
	slot, ok := w.GrantAbility(caster, "fireball")
	if !ok {
		t.Fatal("GrantAbility failed")
	}
	giveMana(w, caster, 100*fixed.One)
	ar := w.Abilities.Row(caster)
	t.Logf("AFTER grant: slot=%d AbilityID[%d]=%d hasAbility=%v",
		slot, slot, w.Abilities.AbilityID[ar][slot], w.UnitHasAbility(caster, "fireball"))
	if !w.UnitHasAbility(caster, "fireball") || w.Abilities.AbilityID[ar][slot] != ref {
		t.Fatal("grant did not land in the slot list (SoT)")
	}

	// Cast it at the victim.
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 8; i++ {
		w.Step()
	}
	life := w.Healths.Life[w.Healths.Row(victim)]
	mana := w.Abilities.Mana[ar]
	ready := w.Abilities.ReadyAt[ar][slot]
	t.Logf("AFTER cast: victimLife=%d (raw) mana=%d ready=%d",
		int64(life), int64(mana), ready)
	if 100*fixed.One-life != 50*fixed.One {
		t.Fatalf("composable cast damage=%d, want 50", int64(100*fixed.One-life))
	}
	if mana != 80*fixed.One {
		t.Fatalf("mana=%d, want 80 (100-20 cost)", int64(mana))
	}
	if ready == 0 {
		t.Fatal("cooldown not set after cast")
	}

	// Revoke → slot cleared → a fresh cast order is refused (no further damage).
	if !w.RevokeAbility(caster, "fireball") {
		t.Fatal("RevokeAbility failed")
	}
	t.Logf("AFTER revoke: hasAbility=%v AbilityID[%d]=%d",
		w.UnitHasAbility(caster, "fireball"), slot, w.Abilities.AbilityID[ar][slot])
	if w.UnitHasAbility(caster, "fireball") || w.Abilities.AbilityID[ar][slot] != 0 {
		t.Fatal("revoke did not clear the slot (SoT)")
	}
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 8; i++ {
		w.Step()
	}
	life2 := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("AFTER revoked cast: victimLife=%d (raw, want unchanged)", int64(life2))
	if life2 != life {
		t.Fatalf("revoked ability still cast: life %d -> %d", int64(life), int64(life2))
	}
}

// TestGrantIdempotent: granting the same ability twice keeps one slot.
func TestGrantIdempotent(t *testing.T) {
	w, ref, caster, _ := grantWorld(t)
	s1, ok1 := w.GrantAbility(caster, "fireball")
	s2, ok2 := w.GrantAbility(caster, "fireball")
	if !ok1 || !ok2 || s1 != s2 {
		t.Fatalf("idempotent grant returned different slots: %d/%v %d/%v", s1, ok1, s2, ok2)
	}
	ar := w.Abilities.Row(caster)
	count := 0
	for s := 0; s < AbilitySlots; s++ {
		if w.Abilities.AbilityID[ar][s] == ref {
			count++
		}
	}
	t.Logf("slots holding fireball ref after double grant = %d (want 1)", count)
	if count != 1 {
		t.Fatalf("duplicate grant created %d slots, want 1", count)
	}
}

// TestGrantUnknownRejected: an unknown ability id is refused (fail-closed).
func TestGrantUnknownRejected(t *testing.T) {
	w, _, caster, _ := grantWorld(t)
	if _, ok := w.GrantAbility(caster, "no_such_ability"); ok {
		t.Fatal("granting an unknown id succeeded (must fail closed)")
	}
	if w.UnitHasAbility(caster, "no_such_ability") {
		t.Fatal("unknown id reported as held")
	}
	t.Log("unknown id rejected, no slot consumed")
}

// TestItemGrantsAbilityOnPickup: an item bound to grant 'fireball' grants it on
// pickup and revokes it on drop. SoT = UnitHasAbility before/after each.
func TestItemGrantsAbilityOnPickup(t *testing.T) {
	w, _, caster, _ := grantWorld(t)
	if !w.BindItemDefs([]data.Item{{ID: "wand", Name: "Wand of Fire"}}) {
		t.Fatal("BindItemDefs failed")
	}
	if !w.RegisterItemAbilityGrant(0, "fireball") {
		t.Fatal("RegisterItemAbilityGrant failed")
	}
	if !w.AddInventory(caster) {
		t.Fatal("AddInventory failed")
	}
	item, ok := w.SpawnItem(0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One})
	if !ok {
		t.Fatal("SpawnItem failed")
	}

	t.Logf("BEFORE pickup: hasAbility=%v", w.UnitHasAbility(caster, "fireball"))
	if w.UnitHasAbility(caster, "fireball") {
		t.Fatal("caster has the item ability before pickup")
	}
	if rc := w.AddItemToInventory(caster, item); rc != ItemOK {
		t.Fatalf("pickup failed: rc=%d", rc)
	}
	t.Logf("AFTER pickup: hasAbility=%v", w.UnitHasAbility(caster, "fireball"))
	if !w.UnitHasAbility(caster, "fireball") {
		t.Fatal("item did not grant its ability on pickup")
	}

	// Find the slot the item is in, then drop it.
	ir := w.Invents.Row(caster)
	dropSlot := -1
	for s := 0; s < InventorySlots; s++ {
		if w.Invents.Slots[ir][s] == item {
			dropSlot = s
			break
		}
	}
	if rc := w.DropItem(caster, dropSlot); rc != ItemOK {
		t.Fatalf("drop failed: rc=%d", rc)
	}
	t.Logf("AFTER drop: hasAbility=%v", w.UnitHasAbility(caster, "fireball"))
	if w.UnitHasAbility(caster, "fireball") {
		t.Fatal("item ability not revoked on drop")
	}
}
