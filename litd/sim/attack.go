package sim

// Attack-cycle state machine (#150, combat-and-orders.md §3.3): per
// WEAPON, all durations integer ticks, every clock an absolute "at
// tick T" value — never a float accumulator.
//
//	Idle → Chase → Windup → (FIRE edge) → Backswing → Cooldown → Windup …
//
// FIRE resolves through the weapon's compiled effect list (#296) when
// one is bound; a zero list is the degenerate built-in weapon — dice
// rolled at launch on the sim PRNG, one DamagePacket queued (#152).
// ProjSpeed > 0 launches a homing missile entity instead (#158); the
// payload travels with the missile and delivers at impact.
//
// Order interplay (§3.3): an order head outside {Stop, Hold, Attack}
// cancels WINDUP (cooldown NOT consumed unless the weapon's
// cancel-consumes-cooldown flag is set) and interrupts BACKSWING
// instantly — animation canceling preserved in the sim model.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Atk* are the per-weapon states (CombatStore.AtkState).
const (
	AtkIdle      uint8 = iota // no engaged target / waiting in range off cooldown
	AtkChase                  // closing from acquisition range to attack range
	AtkWindup                 // committed swing; FIRE at PhaseEnd
	AtkBackswing              // post-FIRE recovery; freely interruptible
	AtkCooldown               // in range, waiting on the ReadyAt clock
)

// atkStateNames renders traces (FSV SoT).
var atkStateNames = [...]string{"idle", "chase", "windup", "backswing", "cooldown"}

// AtkStateName returns the human name of an Atk* state.
func AtkStateName(s uint8) string {
	if int(s) < len(atkStateNames) {
		return atkStateNames[s]
	}
	return "?"
}

// Weapon flag bits (CombatStore.WFlags).
const (
	// WeaponCancelConsumesCooldown: a canceled windup still spends the
	// attack period (table-driven override of the default).
	WeaponCancelConsumesCooldown uint8 = 1 << 0
)

// weaponUsed: slot convention — a weapon exists iff its cooldown is
// nonzero (store doc: zero-valued slot = unused).
func weaponUsed(c *CombatStore, cr int32, s int) bool { return c.Cooldown[cr][s] != 0 }

// atkTransition flips one weapon's state and reports it.
func (w *World) atkTransition(id EntityID, cr int32, slot int, to uint8) {
	from := w.Combats.AtkState[cr][slot]
	if from == to {
		return
	}
	w.Combats.AtkState[cr][slot] = to
	if w.OnAttackTransition != nil {
		w.OnAttackTransition(w.tick, id, slot, from, to)
	}
}

// attackSystem drives every combat row, phase 5, after acquisition
// and before the damage apply pass — packets FIREd this tick land
// this tick.
func (w *World) attackSystem() {
	c := w.Combats
	for cr := int32(0); cr < c.count; cr++ {
		id := c.Entity[cr]

		// explicit attack orders adopt their target every tick (the
		// order is the authority while it is the head)
		interrupted := false
		if or := w.Orders.Row(id); or != -1 {
			switch w.Orders.Kind[or] {
			case OrderAttack:
				if t := w.Orders.Target[or]; t != 0 {
					c.Target[cr] = t
				}
			case OrderStop, OrderHold:
				// acquiring stances: auto-acquired target stands
			default:
				interrupted = true // move/cast/smart/…: combat yields
			}
		}

		tgt := c.Target[cr]
		engaged := tgt != 0 && !interrupted && w.validAcquireTarget(id, tgt)

		if !engaged {
			for s := 0; s < WeaponSlots; s++ {
				if !weaponUsed(c, cr, s) {
					continue
				}
				if c.AtkState[cr][s] == AtkWindup {
					w.cancelWindup(id, cr, s)
				} else if c.AtkState[cr][s] != AtkIdle {
					w.atkTransition(id, cr, s, AtkIdle) // backswing interrupt is instant
				}
			}
			if tgt != 0 && !interrupted && !w.Ents.Alive(tgt) {
				c.Target[cr] = 0 // dead target drops immediately, not on scan cadence
			}
			continue
		}

		tr, ttr := w.Transforms.Row(id), w.Transforms.Row(tgt)
		if tr == -1 || ttr == -1 {
			continue
		}
		dHi, dLo := fixed.DistSq(w.Transforms.Pos[tr], w.Transforms.Pos[ttr])

		anyInRange := false
		for s := 0; s < WeaponSlots; s++ {
			if !weaponUsed(c, cr, s) {
				continue
			}
			rHi, rLo := fixed.RadiusSq(c.Range[cr][s])
			inRange := dHi < rHi || (dHi == rHi && dLo <= rLo)
			if inRange {
				anyInRange = true
			}
			switch c.AtkState[cr][s] {
			case AtkIdle, AtkChase, AtkCooldown:
				if !inRange {
					w.atkTransition(id, cr, s, AtkChase)
					break
				}
				if CooldownReady(w.tick, c.ReadyAt[cr][s]) {
					c.PhaseEnd[cr][s] = w.tick + uint32(c.DamagePt[cr][s])
					w.atkTransition(id, cr, s, AtkWindup)
					break
				}
				w.atkTransition(id, cr, s, AtkCooldown)
			case AtkWindup:
				if !inRange {
					w.cancelWindup(id, cr, s) // target slipped out mid-swing
					break
				}
				if CooldownReady(w.tick, c.PhaseEnd[cr][s]) {
					w.fireWeapon(id, tgt, cr, s) // FIRE edge
					c.ReadyAt[cr][s] = c.PhaseEnd[cr][s] - uint32(c.DamagePt[cr][s]) + w.BuffedCooldown(id, c.Cooldown[cr][s])
					c.PhaseEnd[cr][s] = w.tick + uint32(c.Backswing[cr][s])
					w.atkTransition(id, cr, s, AtkBackswing)
				}
			case AtkBackswing:
				if CooldownReady(w.tick, c.PhaseEnd[cr][s]) {
					w.atkTransition(id, cr, s, AtkCooldown)
				}
			}
		}

		// chase movement is per UNIT: close until some weapon reaches
		// range, then halt the feet. An explicit move order never gets
		// here — it sets `interrupted` and combat yields above.
		if mr := w.Movements.Row(id); mr != -1 {
			if !anyInRange {
				w.StartMoveTo(id, w.Transforms.Pos[ttr])
			} else if w.Movements.State[mr] == MoveFollowing {
				w.Movements.State[mr] = MoveIdle
			}
		}
	}
}

// cancelWindup aborts a committed swing: no FIRE, no packet, and the
// cooldown clock is untouched (immediately re-acquirable) unless the
// weapon's flag says a cancel consumes the period.
func (w *World) cancelWindup(id EntityID, cr int32, s int) {
	if w.Combats.WFlags[cr][s]&WeaponCancelConsumesCooldown != 0 {
		windupStart := w.Combats.PhaseEnd[cr][s] - uint32(w.Combats.DamagePt[cr][s])
		w.Combats.ReadyAt[cr][s] = windupStart + w.BuffedCooldown(id, w.Combats.Cooldown[cr][s])
	}
	w.atkTransition(id, cr, s, AtkIdle)
}

// fireWeapon is the FIRE edge. Damage rolls at launch (one PRNG call
// site — R-SIM-2). Instant delivery resolves the payload now: the
// weapon's compiled effect list (#296) when bound, else one packet to
// the #152 buffer. ProjSpeed > 0 launches a homing missile entity
// (#158) carrying the same payload; spawn failure (pool exhausted) is
// a deterministic dud — NEVER a silent fire-as-instant fallback.
func (w *World) fireWeapon(src, tgt EntityID, cr int32, s int) {
	c := w.Combats
	if ps := c.ProjSpeed[cr][s]; ps > 0 {
		spec := MissileSpec{
			Source: src, Target: tgt, Speed: ps,
			Payload: c.Effects[cr][s],
		}
		if tr := w.Transforms.Row(src); tr != -1 {
			spec.Pos = w.Transforms.Pos[tr]
		}
		if spec.Payload.Len == 0 {
			spec.Packet = DamagePacket{Source: src, Target: tgt, Amount: w.rollWeapon(cr, s), AttackType: c.AttackType[cr][s]}
		}
		w.SpawnMissile(spec)
		return
	}
	if lst := c.Effects[cr][s]; lst.Len > 0 {
		w.ExecuteEffects(lst, EffectCtx{Source: src, Target: tgt})
		return
	}
	w.QueueDamage(DamagePacket{Source: src, Target: tgt, Amount: w.rollWeapon(cr, s), AttackType: c.AttackType[cr][s]})
}

// rollWeapon rolls base + Ndice×roll(sides) on the sim PRNG.
func (w *World) rollWeapon(cr int32, s int) fixed.F64 {
	c := w.Combats
	amt := fixed.FromInt(c.DmgBase[cr][s])
	if dice, sides := int(c.DmgDice[cr][s]), uint32(c.DmgSides[cr][s]); dice > 0 && sides > 0 {
		for i := 0; i < dice; i++ {
			amt = amt.Add(fixed.FromInt(int32(w.rng.Uint32()%sides) + 1))
		}
	}
	return amt
}

// SetWeapon fills one weapon slot from converted data-table values
// (the spawner's helper; tests use it directly until #217 wires
// data-driven unit spawning).
func (w *World) SetWeapon(id EntityID, slot int, a *data.Attack, flags uint8, effects data.EffectList) bool {
	cr := w.Combats.Row(id)
	if cr == -1 || slot < 0 || slot >= WeaponSlots || a.CooldownTicks == 0 {
		return false
	}
	c := w.Combats
	c.DmgBase[cr][slot] = a.DamageBase
	c.DmgDice[cr][slot] = uint8(a.Dice)
	c.DmgSides[cr][slot] = uint8(a.Sides)
	c.AttackType[cr][slot] = a.AttackType
	c.Cooldown[cr][slot] = a.CooldownTicks
	c.DamagePt[cr][slot] = a.DamagePointTicks
	c.Backswing[cr][slot] = a.BackswingTicks
	c.Range[cr][slot] = a.Range
	c.ProjSpeed[cr][slot] = a.ProjectileSpeedPerTick
	c.WFlags[cr][slot] = flags
	c.Effects[cr][slot] = effects
	c.AtkState[cr][slot] = AtkIdle
	c.ReadyAt[cr][slot] = 0
	c.PhaseEnd[cr][slot] = 0
	return true
}
