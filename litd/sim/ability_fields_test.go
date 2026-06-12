package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestAbilityFieldOverrideCooldownManaReadyAt(t *testing.T) {
	w, caster := abilityFieldWorld(t, 1)
	ar := w.Abilities.Row(caster)

	baseCooldown, baseCooldownOK := w.ResolveAbilityField(caster, 0, AbilityFieldCooldown)
	baseMana, baseManaOK := w.ResolveAbilityField(caster, 0, AbilityFieldManaCost)
	before := abilityFieldDump(w)
	beforeAbility := abilityReadyManaDump(w, caster)
	if !w.SetAbilityField(caster, 0, AbilityFieldCooldown, fixed.FromInt(25)) {
		t.Fatal("SetAbilityField cooldown override failed")
	}
	if !w.SetAbilityField(caster, 0, AbilityFieldManaCost, fixed.FromInt(12)) {
		t.Fatal("SetAbilityField mana override failed")
	}
	afterSet := abilityFieldDump(w)
	afterSetAbility := abilityReadyManaDump(w, caster)

	w.tick = 99
	if !w.IssueOrder(caster, Order{Kind: OrderCastAbility, Data: 1}, false) {
		t.Fatal("IssueOrder cast failed")
	}
	w.Step() // tick 100: instant cast point, effect edge, cooldown commit.
	afterCast := abilityFieldDump(w)
	afterCastAbility := abilityReadyManaDump(w, caster)
	t.Logf("FSV ability fields override BEFORE: cooldown=%v/%v mana=%v/%v rows=%s ability=%s",
		baseCooldownOK, toFieldFloat(baseCooldown), baseManaOK, toFieldFloat(baseMana), before, beforeAbility)
	t.Logf("FSV ability fields override AFTER set: rows=%s ability=%s", afterSet, afterSetAbility)
	t.Logf("FSV ability fields override AFTER cast@100: rows=%s ability=%s", afterCast, afterCastAbility)

	if !baseCooldownOK || baseCooldown != fixed.FromInt(50) {
		t.Fatalf("base cooldown = %v/%d, want true/50 fixed", baseCooldownOK, int64(baseCooldown))
	}
	if !baseManaOK || baseMana != fixed.FromInt(20) {
		t.Fatalf("base mana = %v/%d, want true/20 fixed", baseManaOK, int64(baseMana))
	}
	if got := w.Abilities.ReadyAt[ar][0]; got != 125 {
		t.Fatalf("ReadyAt = %d, want cast tick 100 + override cooldown 25 = 125; rows=%s", got, afterCast)
	}
	if got := w.Abilities.Mana[ar]; got != 88*fixed.One {
		t.Fatalf("Mana = %d, want 88 fixed after override cost 12; rows=%s", got, afterCast)
	}
}

func TestAbilityFieldRemoveAbilityClearsSlotOverrides(t *testing.T) {
	w, caster := abilityFieldWorld(t, 2)
	if !w.SetAbilityField(caster, 0, AbilityFieldCooldown, fixed.FromInt(25)) ||
		!w.SetAbilityField(caster, 0, AbilityFieldManaCost, fixed.FromInt(12)) ||
		!w.SetAbilityField(caster, 1, AbilityFieldCooldown, fixed.FromInt(30)) {
		t.Fatal("override setup failed")
	}
	before := abilityFieldDump(w)
	removed := w.RemoveAbility(caster, 0)
	after := abilityFieldDump(w)
	t.Logf("FSV RemoveAbility BEFORE: %s", before)
	t.Logf("FSV RemoveAbility AFTER:  removed=%v %s", removed, after)

	ar := w.Abilities.Row(caster)
	if !removed || w.Abilities.AbilityID[ar][0] != 0 {
		t.Fatalf("RemoveAbility did not clear slot 0: removed=%v abilityID=%d", removed, w.Abilities.AbilityID[ar][0])
	}
	if w.AbilityFields.Count() != 1 {
		t.Fatalf("slot override cleanup left count=%d, want only slot 1 row; rows=%s", w.AbilityFields.Count(), after)
	}
	if _, ok := w.ResolveAbilityField(caster, 0, AbilityFieldCooldown); ok {
		t.Fatalf("removed slot still resolves cooldown; rows=%s", after)
	}
	if v, ok := w.ResolveAbilityField(caster, 1, AbilityFieldCooldown); !ok || v != fixed.FromInt(30) {
		t.Fatalf("slot 1 override was removed too: ok=%v value=%d rows=%s", ok, int64(v), after)
	}
}

func TestAbilityFieldDestroyUnitClearsOverrides(t *testing.T) {
	w, caster := abilityFieldWorld(t, 2)
	if !w.SetAbilityField(caster, 0, AbilityFieldCooldown, fixed.FromInt(25)) ||
		!w.SetAbilityField(caster, 1, AbilityFieldManaCost, fixed.FromInt(9)) {
		t.Fatal("override setup failed")
	}
	before := abilityFieldDump(w)
	destroyed := w.DestroyUnit(caster)
	after := abilityFieldDump(w)
	t.Logf("FSV DestroyUnit ability field cleanup BEFORE: %s", before)
	t.Logf("FSV DestroyUnit ability field cleanup AFTER:  destroyed=%v alive=%v %s", destroyed, w.Ents.Alive(caster), after)

	if !destroyed || w.Ents.Alive(caster) {
		t.Fatalf("DestroyUnit setup failed: destroyed=%v alive=%v", destroyed, w.Ents.Alive(caster))
	}
	if w.AbilityFields.Count() != 0 {
		t.Fatalf("DestroyUnit leaked ability field rows: %s", after)
	}
}

func TestAbilityFieldCapExceededRejectedUnchanged(t *testing.T) {
	w, caster := abilityFieldWorld(t, AbilitySlots)
	writes := 0
	for slot := 0; slot < AbilitySlots && writes < AbilityOverrideCapPerUnit; slot++ {
		for f := AbilityField(0); f < AbilityFieldCount && writes < AbilityOverrideCapPerUnit; f++ {
			if !w.SetAbilityField(caster, slot, f, fixed.FromInt(int32(writes+1))) {
				t.Fatalf("override write %d failed before cap: rows=%s", writes, abilityFieldDump(w))
			}
			writes++
		}
	}
	rowsBefore := abilityFieldRowsDump(w)
	before := abilityFieldDump(w)
	rejectedBefore := w.AbilityFields.Rejected()
	ok := w.SetAbilityField(caster, 2, AbilityFieldDamage, fixed.FromInt(99))
	rowsAfter := abilityFieldRowsDump(w)
	after := abilityFieldDump(w)
	t.Logf("FSV ability field cap BEFORE: rejected=%d %s", rejectedBefore, before)
	t.Logf("FSV ability field cap AFTER:  ok=%v rejected=%d %s", ok, w.AbilityFields.Rejected(), after)

	if ok {
		t.Fatal("cap-exceeded SetAbilityField returned true")
	}
	if w.AbilityFields.Rejected() != rejectedBefore+1 {
		t.Fatalf("rejection count = %d, want %d", w.AbilityFields.Rejected(), rejectedBefore+1)
	}
	if rowsBefore != rowsAfter {
		t.Fatalf("cap-exceeded write mutated rows: before=%s after=%s", rowsBefore, rowsAfter)
	}
}

func TestAbilityFieldUnknownFieldFailClosed(t *testing.T) {
	w, caster := abilityFieldWorld(t, 1)
	before := abilityFieldRowsDump(w)
	value, resolved := w.ResolveAbilityField(caster, 0, AbilityField(200))
	setOK := w.SetAbilityField(caster, 0, AbilityField(200), fixed.One)
	after := abilityFieldRowsDump(w)
	t.Logf("FSV unknown ability field BEFORE: %s", before)
	t.Logf("FSV unknown ability field AFTER:  resolved=%v valueRaw=%d setOK=%v %s", resolved, int64(value), setOK, after)

	if resolved || value != 0 {
		t.Fatalf("unknown field resolved: ok=%v value=%d", resolved, int64(value))
	}
	if setOK {
		t.Fatal("unknown field SetAbilityField returned true")
	}
	if before != after {
		t.Fatalf("unknown field mutated rows: before=%s after=%s", before, after)
	}
}

func abilityFieldWorld(t *testing.T, slots int) (*World, EntityID) {
	t.Helper()
	w := NewWorld(abilityFieldTestCaps())
	bindAbilityFieldDefs(t, w)
	caster := addAbilityFieldUnit(t, w, slots)
	return w, caster
}

func abilityFieldTestCaps() Caps {
	return Caps{
		Units:             4,
		Projectiles:       1,
		Effects:           1,
		BuffInstances:     1,
		OrderQueueEntries: 64,
		PendingEvents:     32,
		PathRequests:      1,
		ScriptedDoodads:   1,
	}
}

func bindAbilityFieldDefs(t *testing.T, w *World) {
	t.Helper()
	if !w.BindAbilityDefs([]data.Ability{{
		ID:            "field-test",
		Name:          "Field Test",
		ManaCost:      20,
		CooldownTicks: 50,
	}}) {
		t.Fatal("BindAbilityDefs failed")
	}
}

func addAbilityFieldUnit(t *testing.T, w *World, slots int) EntityID {
	t.Helper()
	caster := atkUnit(t, w, 0, fixed.Vec2{X: 100 * fixed.One, Y: 100 * fixed.One}, 0)
	if !w.Abilities.Add(w.Ents, caster) {
		t.Fatal("Abilities.Add failed")
	}
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 100 * fixed.One
	w.Abilities.MaxMana[ar] = 100 * fixed.One
	if slots > AbilitySlots {
		slots = AbilitySlots
	}
	for slot := 0; slot < slots; slot++ {
		if !w.SetAbility(caster, slot, 0) {
			t.Fatalf("SetAbility slot %d failed", slot)
		}
	}
	return caster
}

func abilityReadyManaDump(w *World, id EntityID) string {
	ar := w.Abilities.Row(id)
	if ar == -1 {
		return "abilityRow=<missing>"
	}
	return fmt.Sprintf("tick=%d manaRaw=%d mana=%g ready0=%d castSlot=%d state0=%s",
		w.Tick(),
		int64(w.Abilities.Mana[ar]),
		toFieldFloat(w.Abilities.Mana[ar]),
		w.Abilities.ReadyAt[ar][0],
		w.Abilities.CastSlot[ar],
		CastStateName(w.Abilities.CastState[ar][0]),
	)
}

func abilityFieldDump(w *World) string {
	return fmt.Sprintf("count=%d free=%d rejected=%d rows=%s",
		w.AbilityFields.Count(),
		w.AbilityFields.FreeCount(),
		w.AbilityFields.Rejected(),
		abilityFieldRowsDump(w),
	)
}

func abilityFieldRowsDump(w *World) string {
	s := w.AbilityFields
	var b strings.Builder
	b.WriteByte('[')
	first := true
	for r := 0; r < s.Cap(); r++ {
		if !s.live[r] {
			continue
		}
		if !first {
			b.WriteByte(' ')
		}
		first = false
		fmt.Fprintf(&b, "row=%d ent=%#x idx=%d gen=%d slot=%d field=%d valueRaw=%d value=%g",
			r,
			uint32(s.Ent[r]),
			s.Ent[r].Index(),
			s.Ent[r].Generation(),
			s.Slot[r],
			s.Field[r],
			int64(s.Value[r]),
			toFieldFloat(s.Value[r]),
		)
	}
	b.WriteByte(']')
	return b.String()
}

func toFieldFloat(v fixed.F64) float64 {
	return float64(v) / float64(fixed.One)
}
