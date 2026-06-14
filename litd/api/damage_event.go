package litd

// Writable damage (#219; triggers-and-events.md "damage events: writable
// subset"). JASS exposes GetEventDamage / SetEventDamage on a unit-damage
// trigger so a script can scale a hit before it lands. The read side is
// already Event.Damage() (post-hoc observe). The write side needs a
// synchronous, pre-apply hook: the modifier runs inside the sim's combat
// phase, on the final post-mitigation amount, before the victim's life is
// reduced — so the change is real, not cosmetic. That hook is Game.OnDamage.
//
// A damage modifier must be pure (no waits, no spawns, no PRNG of its
// own); it runs mid-tick on the deterministic hot path. With no modifier
// installed the sim path is byte-identical to before (golden trace
// unperturbed).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// DamageEvent is the mutable payload handed to an OnDamage modifier. It
// is passed by pointer so SetAmount writes back into the hit that is
// about to be applied.
type DamageEvent struct {
	src, dst sim.EntityID
	amount   float64
	g        *Game
}

// Source returns the attacking unit (zero Unit on an environmental hit).
func (e *DamageEvent) Source() Unit { return Unit{id: e.src, g: e.g} }

// Unit returns the unit about to take the damage.
func (e *DamageEvent) Unit() Unit { return Unit{id: e.dst, g: e.g} }

// Amount returns the final post-mitigation damage that will be applied
// unless a modifier changes it.
func (e *DamageEvent) Amount() float64 { return e.amount }

// SetAmount overrides the damage to apply (clamped to >= 0 — damage never
// heals). JASS: SetEventDamage.
func (e *DamageEvent) SetAmount(v float64) {
	if v < 0 {
		v = 0
	}
	e.amount = v
}

// OnDamage registers a synchronous pre-apply damage modifier. Modifiers
// run in registration order on every applied hit; each may read and
// SetAmount. Nil-handler / nil-game safe. The first registration wires
// the single sim-side hook that fans out to all modifiers.
//
// Unlike OnEvent(EventUnitDamaged) — which observes the hit after it
// lands, in phase 6 — an OnDamage modifier runs during combat resolution
// and changes the damage that is actually dealt.
func (g *Game) OnDamage(fn func(*DamageEvent)) {
	if g == nil || g.w == nil || fn == nil {
		if g != nil {
			g.reportInvalid("OnDamage (nil handler or game)")
		}
		return
	}
	g.damageHandlers = append(g.damageHandlers, fn)
	if g.damageHookInstalled {
		return
	}
	g.damageHookInstalled = true
	g.w.SetDamageModifier(func(src, dst sim.EntityID, amount fixed.F64) fixed.F64 {
		de := DamageEvent{src: src, dst: dst, amount: toFloat(amount), g: g}
		for _, h := range g.damageHandlers {
			h(&de)
		}
		return fromFloat(de.amount)
	})
}
