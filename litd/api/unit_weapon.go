package litd

// Live weapon overrides (#476): a trigger Action re-arms a unit's weapon at
// runtime — attack type, dice/sides/base, range, cooldown, delivery — without
// disturbing the unit-type default. Setting an override changes the unit's next
// attack; clearing it reverts to the data default. Overrides hash + serialize,
// so a re-armed unit survives save/load.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// WeaponField names one overridable per-instance weapon field. The integer
// values mirror the sim enum (append-only ABI). Range and ProjectileSpeed
// values are raw fixed-point bits; the rest are plain integers (an attack-type
// id from Game.AttackTypeID, a tick count, a die count, …).
type WeaponField uint8

const (
	// WeaponAttackType overrides the weapon's attack type (a Game.AttackTypeID).
	WeaponAttackType WeaponField = WeaponField(sim.WeaponFieldAttackType)
	// WeaponDamageBase overrides the flat base damage added before the dice roll.
	WeaponDamageBase WeaponField = WeaponField(sim.WeaponFieldDamageBase)
	// WeaponDice overrides the number of damage dice rolled.
	WeaponDice WeaponField = WeaponField(sim.WeaponFieldDice)
	// WeaponSides overrides the number of sides on each damage die.
	WeaponSides WeaponField = WeaponField(sim.WeaponFieldSides)
	// WeaponCooldown overrides the attack cooldown, in ticks (must be positive).
	WeaponCooldown WeaponField = WeaponField(sim.WeaponFieldCooldown)
	// WeaponRange overrides the acquisition/attack range, in fixed-point bits.
	WeaponRange WeaponField = WeaponField(sim.WeaponFieldRange)
	// WeaponDamagePoint overrides the windup before the hit lands, in ticks.
	WeaponDamagePoint WeaponField = WeaponField(sim.WeaponFieldDamagePoint)
	// WeaponBackswing overrides the recovery after the hit, in ticks.
	WeaponBackswing WeaponField = WeaponField(sim.WeaponFieldBackswing)
	// WeaponProjectileSpeed overrides the missile speed (fixed-point bits); 0 = instant.
	WeaponProjectileSpeed WeaponField = WeaponField(sim.WeaponFieldProjSpeed)
)

// SetWeaponField installs a live override on one of the unit's weapon slots,
// returning false (a no-op) on an invalid handle, slot, field, or value — or a
// slot with no weapon. JASS analog: BlzSetUnitWeapon* setters.
func (u Unit) SetWeaponField(slot int, field WeaponField, value int) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetWeaponField")
		return false
	}
	return u.g.w.SetUnitWeaponField(u.id, slot, sim.WeaponField(field), int64(value))
}

// ClearWeaponField removes one override, reverting that field to the unit-type
// default. Returns false if there was no override.
func (u Unit) ClearWeaponField(slot int, field WeaponField) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ClearWeaponField")
		return false
	}
	return u.g.w.ClearUnitWeaponField(u.id, slot, sim.WeaponField(field))
}

// ClearWeapon removes every override on a weapon slot (full revert). Returns the
// number of overrides cleared.
func (u Unit) ClearWeapon(slot int) int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ClearWeapon")
		return 0
	}
	return u.g.w.ClearUnitWeapon(u.id, slot)
}

// WeaponField returns the effective value of a weapon field (override if set,
// else the data default), with ok=false on an invalid handle/slot/field.
func (u Unit) WeaponField(slot int, field WeaponField) (int, bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.WeaponField")
		return 0, false
	}
	v, ok := u.g.w.GetUnitWeaponField(u.id, slot, sim.WeaponField(field))
	return int(v), ok
}
