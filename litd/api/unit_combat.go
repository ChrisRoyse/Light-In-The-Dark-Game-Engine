package litd

// Unit combat verbs (#217, units.md). Unit.Damage is the public face of the
// JASS UnitDamage{Target,Point} family: it enqueues a deterministic damage
// packet through the sim's single combat chokepoint (World.QueueDamage), which
// mitigates against the bound armor matrix and applies at the combat phase end
// (damage.go). The attack/damage/weapon-type WC3 args that are purely a render/
// sound cue are dropped; the one arg that drives the matrix — the attack-type
// row index — is preserved as the WithAttackType option (R-API-3), so the
// capability is kept, not averaged away (units.md porting hazard "capability
// preserved").

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// DamageOption configures Unit.Damage (R-API-3 functional option).
type DamageOption func(*damageConfig)

type damageConfig struct {
	attackType uint8
}

// WithAttackType selects the damage-matrix attack-type row (the index into the
// loaded combat damage table's attack-types, table order; 0 is the first/
// default row). Out-of-range indices clamp into uint8 and, if the bound matrix
// has no such row, the packet is a counted drop (never silently guessed).
func WithAttackType(idx int) DamageOption {
	return func(c *damageConfig) {
		if idx < 0 {
			idx = 0
		}
		if idx > 255 {
			idx = 255
		}
		c.attackType = uint8(idx)
	}
}

// Damage makes this unit deal amount damage to target (a Unit or Destructable;
// Items have no health and are rejected). The packet is mitigated by the
// target's armor against the bound damage matrix and applied at the next combat
// phase, deterministically. A non-positive amount, an invalid source/target, or
// an item target is a no-op returning false. JASS: UnitDamageTarget /
// UnitDamageTargetBJ / UnitDamagePoint / UnitDamagePointLoc collapse here.
// JASS: UnitDamagePoint, UnitDamagePointLoc, UnitDamageTarget, UnitDamageTargetBJ
func (u Unit) Damage(target Widget, amount float64, opts ...DamageOption) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Damage")
		return false
	}
	tid, ok := u.g.widgetID(target)
	if !ok || amount <= 0 {
		return false
	}
	cfg := damageConfig{}
	for _, o := range opts {
		o(&cfg)
	}
	return u.g.w.QueueDamage(sim.DamagePacket{
		Source:     u.id,
		Target:     tid,
		Amount:     fromFloat(amount),
		AttackType: cfg.attackType,
	})
}

// widgetID resolves a public Widget to its backing sim entity id, reporting ok
// only for a live entity that can take damage (Unit, Destructable). The zero
// value of any widget type, a stale handle, and an Item (no health row) all
// report ok=false.
func (g *Game) widgetID(w Widget) (sim.EntityID, bool) {
	switch v := w.(type) {
	case Unit:
		if v.Valid() {
			return v.id, true
		}
	case Destructable:
		if v.Valid() {
			return v.id, true
		}
	}
	return 0, false
}
