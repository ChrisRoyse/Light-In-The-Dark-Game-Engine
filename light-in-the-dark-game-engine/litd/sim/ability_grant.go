package sim

// Data-only ability grant — PRD2 06 (epic #549, #597). Granting an ability is
// data: a unit/item references an ability by string id and the engine resolves
// it to the existing #160 per-unit ability slot (R-ABL-4, no per-ability code).
// Composable AbilitySpecs (#594/#595) ride the SAME slot machine via a backing
// runtime ability def whose EFFECT edge runs the op interpreter — so cooldown,
// mana, cast states, save and hash are all reused, never reinvented.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"

// itemAbilityGrant binds an item type to an ability ref it grants its carrier.
type itemAbilityGrant struct {
	itemType uint16
	ref      uint16
}

// RegisterAbilitySpec compiles a composable ability source, registers it in the
// AbilityBook, and bridges it into the #160 ability system as a runtime def so
// it can be granted by id and cast through the existing slot machine. Returns
// the ability ref (defIndex+1). Setup-only (RegisterAbilityDef refuses calls
// during a tick). Fail-closed: a compile or registration failure registers
// nothing.
func (w *World) RegisterAbilitySpec(src data.AbilitySpecLowered, res AbilityResolver) (uint16, error) {
	spec, err := CompileAbilitySpec(src, res)
	if err != nil {
		return 0, err
	}
	def := data.Ability{
		ID:   spec.ID,
		Name: spec.Name,
		// Behavior is neither a static effect list nor a bound trigger: the
		// EFFECT edge routes to the op interpreter (driveCast, #595). Empty
		// Effects + empty TriggerName is the documented "no static behavior".
		ManaCost: spec.ManaCost,
		// Windup to the EFFECT edge = precast anticipation + cast point.
		CastPointTicks: spec.Precast + spec.CastPoint,
		BackswingTicks: spec.Backswing,
		CooldownTicks:  spec.Cooldown,
		CastRange:      spec.CastRange,
	}
	ref, ok := w.RegisterAbilityDef(def)
	if !ok {
		return 0, errAbilityRegister(spec.ID)
	}
	specIdx := w.AbilityDefs.RegisterSpec(spec)
	w.setSpecRef(ref, specIdx)
	return ref, nil
}

type abilityRegisterError struct{ id string }

func (e abilityRegisterError) Error() string {
	return "sim: RegisterAbilitySpec: backing ability def registration refused for " + e.id +
		" (during a tick, pool full, or duplicate id)"
}
func errAbilityRegister(id string) error { return abilityRegisterError{id} }

// setSpecRef records ref → spec index+1, growing the table as needed.
func (w *World) setSpecRef(ref, specIdx uint16) {
	for int(ref) >= len(w.specRefs) {
		w.specRefs = append(w.specRefs, 0)
	}
	w.specRefs[ref] = specIdx + 1
}

// specForRef returns the composable spec index bound to an ability ref, if any.
// Read in the cast EFFECT edge — a plain slice index, deterministic, zero-alloc.
func (w *World) specForRef(ref uint16) (uint16, bool) {
	if int(ref) >= len(w.specRefs) || w.specRefs[ref] == 0 {
		return 0, false
	}
	return w.specRefs[ref] - 1, true
}

// GrantAbility grants the ability with string id to a unit, placing it in the
// first free slot. Idempotent: a unit that already has the ability keeps its
// existing slot (no duplicate). Returns (slot, true) on success; fail-closed on
// an unknown id, a full ability bar, or a dead unit. The unit's ability row is
// created on demand so a freshly spawned unit can be granted abilities.
func (w *World) GrantAbility(unit EntityID, id string) (int, bool) {
	ref, ok := w.AbilityRefByCode(id)
	if !ok {
		return -1, false
	}
	return w.grantAbilityRef(unit, ref)
}

// grantAbilityRef is the ref-based grant core shared by id grants and item
// grants. Idempotent on (unit, ref).
func (w *World) grantAbilityRef(unit EntityID, ref uint16) (int, bool) {
	if ref == 0 || !w.Ents.Alive(unit) || w.abilityDefByRef(ref) == nil {
		return -1, false
	}
	if w.Abilities.Row(unit) == -1 && !w.Abilities.Add(w.Ents, unit) {
		return -1, false
	}
	ar := w.Abilities.Row(unit)
	free := -1
	for s := 0; s < AbilitySlots; s++ {
		switch w.Abilities.AbilityID[ar][s] {
		case ref:
			return s, true // already granted — idempotent
		case 0:
			if free == -1 {
				free = s
			}
		}
	}
	if free == -1 {
		return -1, false // ability bar full
	}
	if !w.SetAbilityRef(unit, free, ref) {
		return -1, false
	}
	return free, true
}

// RevokeAbility removes the ability with string id from a unit. Returns false
// if the unit never had it (or the id is unknown).
func (w *World) RevokeAbility(unit EntityID, id string) bool {
	ref, ok := w.AbilityRefByCode(id)
	if !ok {
		return false
	}
	return w.revokeAbilityRef(unit, ref)
}

func (w *World) revokeAbilityRef(unit EntityID, ref uint16) bool {
	ar := w.Abilities.Row(unit)
	if ar == -1 || ref == 0 {
		return false
	}
	for s := 0; s < AbilitySlots; s++ {
		if w.Abilities.AbilityID[ar][s] == ref {
			return w.RemoveAbility(unit, s)
		}
	}
	return false
}

// UnitHasAbility reports whether a unit currently has the ability with the
// given string id in any slot. SoT query for grant/revoke FSV.
func (w *World) UnitHasAbility(unit EntityID, id string) bool {
	ref, ok := w.AbilityRefByCode(id)
	if !ok {
		return false
	}
	ar := w.Abilities.Row(unit)
	if ar == -1 {
		return false
	}
	for s := 0; s < AbilitySlots; s++ {
		if w.Abilities.AbilityID[ar][s] == ref {
			return true
		}
	}
	return false
}

// RegisterItemAbilityGrant binds an item type to an ability (by id) it grants
// its carrier on pickup and revokes on drop. Setup-only. Fail-closed on an
// unknown ability id.
func (w *World) RegisterItemAbilityGrant(itemType uint16, abilityID string) bool {
	ref, ok := w.AbilityRefByCode(abilityID)
	if !ok {
		return false
	}
	w.itemGrants = append(w.itemGrants, itemAbilityGrant{itemType: itemType, ref: ref})
	return true
}

// applyItemGrants grants (or revokes) every ability bound to an item type for a
// unit. Called from the pickup/drop/give paths so item-granted abilities follow
// the item deterministically. Two sources compose: the data-authored
// item.grants-abilities (#621) and the sim-side RegisterItemAbilityGrant
// registry (#597, for runtime/scripted grants).
func (w *World) applyItemGrants(unit EntityID, itemType uint16, grant bool) {
	// Data-authored grants from the item definition.
	if int(itemType) < len(w.itemDefs) {
		for _, ai := range w.itemDefs[itemType].GrantsAbilities {
			ref := ai + 1 // data index → ability ref (defIndex+1)
			if grant {
				w.grantAbilityRef(unit, ref)
			} else {
				w.revokeAbilityRef(unit, ref)
			}
		}
	}
	// Runtime/scripted grants registered on the world.
	for i := range w.itemGrants {
		g := &w.itemGrants[i]
		if g.itemType != itemType {
			continue
		}
		if grant {
			w.grantAbilityRef(unit, g.ref)
		} else {
			w.revokeAbilityRef(unit, g.ref)
		}
	}
}
