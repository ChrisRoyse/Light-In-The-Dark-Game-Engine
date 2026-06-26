package litd

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestAbilityAPILevelAndFieldFSV(t *testing.T) {
	w, g, u, id := abilityAPITestUnit(t)
	before := abilityAPIDump(w, id)
	ref := g.RegisterAbility(AbilityDef{
		ID:       "api-bolt",
		Name:     "API Bolt",
		ManaCost: 9,
		Cooldown: 1.25,
	})
	afterRegister := abilityAPIDump(w, id)
	t.Logf("FSV RegisterAbility happy BEFORE: %s", before)
	t.Logf("FSV RegisterAbility happy AFTER:  ref=%d defCount=%d %s", ref, w.AbilityDefCount(), afterRegister)
	if ref == 0 {
		t.Fatal("RegisterAbility returned zero ref for valid def")
	}

	a := u.AddAbility(ref)
	afterAdd := abilityAPIDump(w, id)
	t.Logf("FSV Unit.AddAbility AFTER: handleValid=%v level=%d %s", a.Valid(), a.Level(), afterAdd)
	if !a.Valid() {
		t.Fatalf("AddAbility returned invalid handle: %s", afterAdd)
	}
	ar, slot := abilityAPISlot(t, w, id, ref)
	if slot != 0 || w.Abilities.Level[ar][slot] != 1 {
		t.Fatalf("AddAbility slot/level = slot %d level %d, want slot 0 level 1; %s", slot, w.Abilities.Level[ar][slot], afterAdd)
	}

	beforeMutate := abilityAPIDump(w, id)
	a.SetLevel(3)
	a.SetField(AbilityFieldCooldown, 1.25)
	a.SetField(AbilityFieldManaCost, 42)
	afterMutate := abilityAPIDump(w, id)
	t.Logf("FSV Ability Level/Field BEFORE: %s", beforeMutate)
	t.Logf("FSV Ability Level/Field AFTER:  Level=%d Cooldown=%v ManaCost=%v %s",
		a.Level(), a.Cooldown(), a.ManaCost(), afterMutate)

	ar, slot = abilityAPISlot(t, w, id, ref)
	if got := w.Abilities.Level[ar][slot]; got != 3 {
		t.Fatalf("store level = %d, want 3; %s", got, afterMutate)
	}
	if got, ok := w.AbilityFields.Get(id, slot, sim.AbilityFieldCooldown); !ok || got != fromFloat(1.25) {
		t.Fatalf("cooldown override = raw %d ok %v, want raw %d true; %s", int64(got), ok, int64(fromFloat(1.25)), afterMutate)
	}
	if got, ok := w.AbilityFields.Get(id, slot, sim.AbilityFieldManaCost); !ok || got != fromFloat(42) {
		t.Fatalf("mana override = raw %d ok %v, want raw %d true; %s", int64(got), ok, int64(fromFloat(42)), afterMutate)
	}
	if a.Cooldown() != 1.25 || a.ManaCost() != 42 {
		t.Fatalf("typed field getters = cooldown %v mana %v, want 1.25/42; %s", a.Cooldown(), a.ManaCost(), afterMutate)
	}
}

func TestAbilityZeroValueAndInvalidDebugFSV(t *testing.T) {
	w, g, _, id := abilityAPITestUnit(t)
	var zero Ability
	before := abilityAPIDump(w, id)
	zeroLevel := zero.Level()
	zeroField := zero.Field(AbilityFieldCooldown)
	zeroCooldown := zero.Cooldown()
	zeroMana := zero.ManaCost()
	zero.SetLevel(2)
	zero.SetField(AbilityFieldCooldown, 1.25)
	afterZero := abilityAPIDump(w, id)
	t.Logf("FSV zero Ability BEFORE: %s", before)
	t.Logf("FSV zero Ability AFTER:  Level=%d Field=%v Cooldown=%v ManaCost=%v zeroValid=%v %s",
		zeroLevel, zeroField, zeroCooldown, zeroMana, zero.Valid(), afterZero)
	if zeroLevel != 0 || zeroField != 0 || zeroCooldown != 0 || zeroMana != 0 || zero.Valid() {
		t.Fatalf("zero Ability getters/valid = %d/%v/%v/%v/%v, want zeros/false",
			zeroLevel, zeroField, zeroCooldown, zeroMana, zero.Valid())
	}
	if before != afterZero {
		t.Fatalf("zero Ability setters mutated store: before=%s after=%s", before, afterZero)
	}

	var reports []string
	g.OnInvalidHandle(func(report string) { reports = append(reports, report) })
	g.SetDebug(true)
	invalid := Ability{owner: id, ref: 999, g: g}
	beforeInvalid := abilityAPIDump(w, id)
	invalid.SetLevel(2)
	invalid.SetField(AbilityFieldCooldown, 1.25)
	afterInvalid := abilityAPIDump(w, id)
	t.Logf("FSV invalid Ability debug BEFORE: %s", beforeInvalid)
	t.Logf("FSV invalid Ability debug AFTER:  reports=%v %s", reports, afterInvalid)
	if beforeInvalid != afterInvalid {
		t.Fatalf("invalid Ability setters mutated store: before=%s after=%s", beforeInvalid, afterInvalid)
	}
	for _, verb := range []string{"Ability.SetLevel", "Ability.SetField"} {
		if !reportsContain(reports, verb) {
			t.Fatalf("debug reports missing %s: %v", verb, reports)
		}
	}
}

func TestRemoveAbilityInvalidatesHandleAndClearsOverridesFSV(t *testing.T) {
	w, g, u, id := abilityAPITestUnit(t)
	ref := g.RegisterAbility(AbilityDef{ID: "api-remove", ManaCost: 3})
	a := u.AddAbility(ref)
	if !a.Valid() {
		t.Fatal("AddAbility returned invalid handle")
	}
	a.SetField(AbilityFieldCooldown, 1.25)
	a.SetField(AbilityFieldManaCost, 7)
	ar, slot := abilityAPISlot(t, w, id, ref)
	before := abilityAPIDump(w, id)
	beforeCount := w.AbilityFields.Count()
	beforeID := w.Abilities.AbilityID[ar][slot]
	removed := u.RemoveAbility(ref)
	after := abilityAPIDump(w, id)
	t.Logf("FSV Unit.RemoveAbility BEFORE: slot=%d abilityID=%d fieldCount=%d %s",
		slot, beforeID, beforeCount, before)
	t.Logf("FSV Unit.RemoveAbility AFTER:  removed=%v handleValid=%v fieldCount=%d %s",
		removed, a.Valid(), w.AbilityFields.Count(), after)
	if !removed {
		t.Fatalf("RemoveAbility returned false: before=%s after=%s", before, after)
	}
	if a.Valid() {
		t.Fatalf("Ability handle remained valid after removal: %s", after)
	}
	if got := w.Abilities.AbilityID[ar][slot]; got != 0 {
		t.Fatalf("ability slot not cleared: got ref %d; %s", got, after)
	}
	if got := w.Abilities.Level[ar][slot]; got != 0 {
		t.Fatalf("ability level not cleared: got %d; %s", got, after)
	}
	if _, ok := w.AbilityFields.Get(id, slot, sim.AbilityFieldCooldown); ok {
		t.Fatalf("cooldown override survived removal: %s", after)
	}
	if _, ok := w.AbilityFields.Get(id, slot, sim.AbilityFieldManaCost); ok {
		t.Fatalf("mana override survived removal: %s", after)
	}
	if w.AbilityFields.Count() != 0 {
		t.Fatalf("override store not empty after removal: %s", after)
	}
}

// TestAbilityIncDecLevelFSV proves IncLevel/DecLevel against the store
// SoT (Abilities.Level[ar][slot]): happy +1/-1, saturation at 255,
// floor at 1, and the invalid-handle zero-value contract.
func TestAbilityIncDecLevelFSV(t *testing.T) {
	w, g, u, id := abilityAPITestUnit(t)
	ref := g.RegisterAbility(AbilityDef{ID: "api-incdec", ManaCost: 1})
	a := u.AddAbility(ref)
	if !a.Valid() {
		t.Fatal("AddAbility returned invalid handle")
	}
	ar, slot := abilityAPISlot(t, w, id, ref)

	// Happy: equipped at level 1 -> Inc -> 2.
	beforeLvl := w.Abilities.Level[ar][slot]
	got := a.IncLevel()
	afterLvl := w.Abilities.Level[ar][slot]
	t.Logf("FSV IncLevel happy BEFORE store=%d AFTER ret=%d store=%d", beforeLvl, got, afterLvl)
	if beforeLvl != 1 || got != 2 || afterLvl != 2 {
		t.Fatalf("IncLevel happy: before=%d ret=%d store=%d, want before 1 ret 2 store 2", beforeLvl, got, afterLvl)
	}

	// Happy: Dec -> back to 1.
	got = a.DecLevel()
	if afterLvl = w.Abilities.Level[ar][slot]; got != 1 || afterLvl != 1 {
		t.Fatalf("DecLevel happy: ret=%d store=%d, want 1/1", got, afterLvl)
	}

	// Edge — floor at 1: Dec when already at 1 must not drop to 0 (that
	// would mean "unequipped" inconsistent with a non-zero AbilityID).
	got = a.DecLevel()
	afterLvl = w.Abilities.Level[ar][slot]
	t.Logf("FSV DecLevel floor: ret=%d store=%d (want 1/1, removal is explicit)", got, afterLvl)
	if got != 1 || afterLvl != 1 {
		t.Fatalf("DecLevel floor: ret=%d store=%d, want 1/1", got, afterLvl)
	}

	// Edge — saturation at 255: write 255 directly, Inc must not wrap to 0.
	w.Abilities.Level[ar][slot] = 255
	got = a.IncLevel()
	afterLvl = w.Abilities.Level[ar][slot]
	t.Logf("FSV IncLevel saturate: ret=%d store=%d (want 255/255, no wrap)", got, afterLvl)
	if got != 255 || afterLvl != 255 {
		t.Fatalf("IncLevel saturate: ret=%d store=%d, want 255/255 (no uint8 wrap)", got, afterLvl)
	}

	// Edge — invalid handle (after removal): zero-value contract, no panic,
	// no store mutation on a slot that no longer holds the ref.
	if !u.RemoveAbility(ref) {
		t.Fatal("RemoveAbility failed")
	}
	if inc := a.IncLevel(); inc != 0 {
		t.Fatalf("IncLevel on removed ability = %d, want 0", inc)
	}
	if dec := a.DecLevel(); dec != 0 {
		t.Fatalf("DecLevel on removed ability = %d, want 0", dec)
	}
	if got := w.Abilities.Level[ar][slot]; got != 0 {
		t.Fatalf("store slot mutated by Inc/Dec on removed ability: got %d, want 0", got)
	}
}

func TestRegisterAbilityBadDefZeroRefNoMutationFSV(t *testing.T) {
	w, g, _, id := abilityAPITestUnit(t)
	var reports []string
	g.OnInvalidHandle(func(report string) { reports = append(reports, report) })
	g.SetDebug(true)

	beforeDump := abilityAPIDump(w, id)
	beforeTop, beforeDefs := abilityAPIHash(w)
	beforeCount := w.AbilityDefCount()
	ref := g.RegisterAbility(AbilityDef{ID: "api-bad", ManaCost: -1})
	afterDump := abilityAPIDump(w, id)
	afterTop, afterDefs := abilityAPIHash(w)
	afterCount := w.AbilityDefCount()
	t.Logf("FSV RegisterAbility bad BEFORE: count=%d top=%016x abilitydefs=%016x %s",
		beforeCount, beforeTop, beforeDefs, beforeDump)
	t.Logf("FSV RegisterAbility bad AFTER:  ref=%d count=%d top=%016x abilitydefs=%016x reports=%v %s",
		ref, afterCount, afterTop, afterDefs, reports, afterDump)
	if ref != 0 {
		t.Fatalf("bad def returned ref %d, want zero", ref)
	}
	if beforeCount != afterCount || beforeTop != afterTop || beforeDefs != afterDefs || beforeDump != afterDump {
		t.Fatalf("bad def mutated state:\nbefore count=%d top=%016x defs=%016x dump=%s\nafter  count=%d top=%016x defs=%016x dump=%s",
			beforeCount, beforeTop, beforeDefs, beforeDump, afterCount, afterTop, afterDefs, afterDump)
	}
	if !reportsContain(reports, "Game.RegisterAbility") {
		t.Fatalf("debug report missing Game.RegisterAbility: %v", reports)
	}
}

func abilityAPITestUnit(t *testing.T) (*sim.World, *Game, Unit, sim.EntityID) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 4, RuntimeAbilityDefs: 8})
	g := newGame(w)
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	return w, g, Unit{id: id, g: g}, id
}

func abilityAPISlot(t *testing.T, w *sim.World, id sim.EntityID, ref AbilityRef) (int32, int) {
	t.Helper()
	ar := w.Abilities.Row(id)
	if ar == -1 {
		t.Fatalf("ability row missing for %#x; %s", uint32(id), abilityAPIDump(w, id))
	}
	for slot := 0; slot < sim.AbilitySlots; slot++ {
		if w.Abilities.AbilityID[ar][slot] == uint16(ref) {
			return ar, slot
		}
	}
	t.Fatalf("ability ref %d not equipped; %s", ref, abilityAPIDump(w, id))
	return -1, -1
}

func abilityAPIDump(w *sim.World, id sim.EntityID) string {
	ar := w.Abilities.Row(id)
	overrides := make([]string, 0, sim.AbilitySlots*7)
	for slot := 0; slot < sim.AbilitySlots; slot++ {
		for field := sim.AbilityField(0); field < sim.AbilityFieldCount; field++ {
			if v, ok := w.AbilityFields.Get(id, slot, field); ok {
				overrides = append(overrides, fmt.Sprintf("s%d:%s=raw:%d float:%g", slot, abilityAPISimFieldName(field), int64(v), toFloat(v)))
			}
		}
	}
	if ar == -1 {
		return fmt.Sprintf("alive=%v ar=-1 fieldCount=%d free=%d rejected=%d defCount=%d overrides=[%s]",
			w.Ents.Alive(id), w.AbilityFields.Count(), w.AbilityFields.FreeCount(), w.AbilityFields.Rejected(),
			w.AbilityDefCount(), strings.Join(overrides, ","))
	}
	return fmt.Sprintf("alive=%v ar=%d ids=%v levels=%v fieldCount=%d free=%d rejected=%d defCount=%d overrides=[%s]",
		w.Ents.Alive(id), ar, w.Abilities.AbilityID[ar], w.Abilities.Level[ar],
		w.AbilityFields.Count(), w.AbilityFields.FreeCount(), w.AbilityFields.Rejected(),
		w.AbilityDefCount(), strings.Join(overrides, ","))
}

func abilityAPISimFieldName(field sim.AbilityField) string {
	switch field {
	case sim.AbilityFieldCooldown:
		return "cooldown"
	case sim.AbilityFieldManaCost:
		return "mana"
	case sim.AbilityFieldRange:
		return "range"
	case sim.AbilityFieldDamage:
		return "damage"
	case sim.AbilityFieldDuration:
		return "duration"
	case sim.AbilityFieldAreaOfEffect:
		return "aoe"
	case sim.AbilityFieldCastTime:
		return "casttime"
	default:
		return fmt.Sprintf("field%d", field)
	}
}

func abilityAPIHash(w *sim.World) (uint64, uint64) {
	reg := sim.NewHashRegistry()
	snap := w.HashState(reg, &statehash.Snapshot{})
	for i, name := range reg.Names() {
		if name == "abilitydefs" {
			return snap.Top, snap.Subs[i]
		}
	}
	return snap.Top, 0
}
