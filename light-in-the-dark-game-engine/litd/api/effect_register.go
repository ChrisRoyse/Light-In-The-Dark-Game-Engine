package litd

// Runtime effect-primitive registration (#477): a modded map registers a named
// effect/Action during setup (e.g. "lifesteal"), then a trigger Action invokes
// it on a hit. The registered set (names, in registration order) hashes +
// serializes — two players must agree; the closure is re-bound in setup on load.

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// EffectInvocation is the context a registered effect receives when it runs:
// the source (caster/attacker) and target of the hit/cast that fired it.
type EffectInvocation struct {
	src, tgt sim.EntityID
	g        *Game
}

// Valid reports whether the invocation context is usable — a live game with a
// living source unit. An effect run on a since-dead source is invalid.
func (e EffectInvocation) Valid() bool {
	return e.g != nil && e.g.w != nil && e.g.w.Ents.Alive(e.src)
}

// Source returns the unit that triggered the effect (caster/attacker).
func (e EffectInvocation) Source() Unit { return Unit{id: e.src, g: e.g} }

// Target returns the effect's target unit (zero-value if the effect is sourced
// from a point or has no target).
func (e EffectInvocation) Target() Unit { return Unit{id: e.tgt, g: e.g} }

// Heal restores life to a unit (clamped to its max) and returns the amount
// actually restored — the building block of a lifesteal/regen effect.
func (e EffectInvocation) Heal(u Unit, amount float64) float64 {
	if e.g == nil || amount <= 0 {
		return 0
	}
	return toFloat(e.g.w.HealUnit(u.id, fromFloat(amount)))
}

// RegisterEffect installs a named runtime effect primitive, callable as a
// trigger Action via Game.RunEffect. Setup verb (R-API-5): fails closed if
// called after the match starts ticking, on an empty or duplicate name, a nil
// fn, or a full registry. The registered set hashes + serializes; on load the
// world must re-register the same set (re-run setup) or LoadState fails closed.
func (g *Game) RegisterEffect(name string, fn func(EffectInvocation)) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: RegisterEffect: nil game")
	}
	if fn == nil {
		return fmt.Errorf("api: RegisterEffect: nil effect fn")
	}
	_, ok := g.w.RegisterEffect(name, func(w *sim.World, ctx sim.EffectCtx) {
		fn(EffectInvocation{src: ctx.Source, tgt: ctx.Target, g: g})
	})
	if !ok {
		return fmt.Errorf("api: RegisterEffect: %q refused (registration is setup-only and names must be unique)", name)
	}
	g.logSetup("RegisterEffect: %q registered", name)
	return nil
}

// RunEffect invokes a registered effect by name against a source/target pair,
// returning false (a no-op) when the name is not registered. A trigger Action
// calls this when a hit or cast should fire the modded primitive.
func (g *Game) RunEffect(name string, source, target Unit) bool {
	if g == nil || g.w == nil {
		return false
	}
	id, ok := g.w.RegisteredEffectID(name)
	if !ok {
		return false
	}
	return g.w.RunRegisteredEffect(id, sim.EffectCtxFor(source.id, target.id))
}

// EffectRegistered reports whether a runtime effect name is registered.
func (g *Game) EffectRegistered(name string) bool {
	if g == nil || g.w == nil {
		return false
	}
	_, ok := g.w.RegisteredEffectID(name)
	return ok
}
