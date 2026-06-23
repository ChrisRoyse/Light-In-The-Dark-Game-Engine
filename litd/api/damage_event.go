package litd

// Writable damage (#219; triggers-and-events.md "damage events: writable
// subset") and programmable combat (#475, ADR #453). JASS exposes
// GetEventDamage / SetEventDamage so a script can scale a hit before it lands.
// The read side is already Event.Damage(); the write side is a synchronous,
// pre-apply hook that runs inside the sim's combat phase, before the victim's
// life is reduced — so the change is real, not cosmetic. Two surfaces share the
// DamageEvent payload:
//
//   - Game.OnDamage: a post-mitigation amount modifier (the legacy #219 hook,
//     runs in the base "script-modifier" stage). amount-only.
//   - Game.ReplaceDamageStage / SetDamageFormula: a script owns a named stage
//     of the #473 pipeline and reads/writes the FULL context — attack type,
//     armor type, raw + running amount, flags — and may re-apply the
//     coefficient after changing the attack type.
//
// A damage modifier/stage must be pure (no waits, no spawns, no PRNG of its
// own); it runs mid-tick on the deterministic hot path. With nothing installed
// the sim path is byte-identical to before (golden trace unperturbed).

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// DamageEvent is the mutable payload handed to an OnDamage modifier or a
// programmable damage stage. Passed by pointer so writes land in the hit that
// is about to be applied. When ctx is non-nil (stage mode) the getters/setters
// operate on the live pipeline context; otherwise (legacy OnDamage) it carries
// the amount-only fields.
type DamageEvent struct {
	src, dst sim.EntityID
	amount   float64
	ctx      *sim.DamageCtx
	g        *Game
}

// Valid reports whether the event handle is usable (live only inside a callback).
func (e *DamageEvent) Valid() bool { return e != nil && e.g != nil }

// Source returns the attacking unit (zero Unit on an environmental hit).
func (e *DamageEvent) Source() Unit {
	if e.ctx != nil {
		return Unit{id: e.ctx.Source, g: e.g}
	}
	return Unit{id: e.src, g: e.g}
}

// Unit returns the unit about to take the damage.
func (e *DamageEvent) Unit() Unit {
	if e.ctx != nil {
		return Unit{id: e.ctx.Target, g: e.g}
	}
	return Unit{id: e.dst, g: e.g}
}

// Amount returns the current (running) damage that will be applied unless a
// modifier changes it.
func (e *DamageEvent) Amount() float64 {
	if e.ctx != nil {
		return toFloat(e.ctx.Amount)
	}
	return e.amount
}

// SetAmount overrides the damage to apply (clamped to >= 0 — damage never
// heals). JASS: SetEventDamage, BlzSetEventDamage.
// JASS: BlzSetEventDamage
func (e *DamageEvent) SetAmount(v float64) {
	if v < 0 {
		v = 0
	}
	if e.ctx != nil {
		e.ctx.Amount = fromFloat(v)
		return
	}
	e.amount = v
}

// RawAmount returns the original queued amount before any mitigation (stage
// mode only; 0 in the legacy amount-only hook).
func (e *DamageEvent) RawAmount() float64 {
	if e.ctx != nil {
		return toFloat(e.ctx.Raw)
	}
	return 0
}

// AttackType returns the declared attack-type name of the hit ("" in legacy
// mode or if the tables were never declared).
func (e *DamageEvent) AttackType() string {
	if e.ctx != nil {
		return e.g.w.AttackTypeName(e.ctx.AttackType)
	}
	return ""
}

// ArmorType returns the declared armor-type name of the target.
func (e *DamageEvent) ArmorType() string {
	if e.ctx != nil {
		return e.g.w.ArmorTypeName(e.ctx.ArmorType)
	}
	return ""
}

// SetAttackType changes the hit's attack type by name, returning false
// fail-closed on an unknown name or in legacy mode (no context). The new type
// takes effect on the next ApplyCoefficient (or a later coeff stage).
func (e *DamageEvent) SetAttackType(name string) bool {
	if e.ctx == nil {
		return false
	}
	id, ok := e.g.w.AttackTypeIndex(name)
	if !ok {
		return false
	}
	e.ctx.AttackType = id
	return true
}

// SetArmorType changes the target's armor type by name, fail-closed as above.
func (e *DamageEvent) SetArmorType(name string) bool {
	if e.ctx == nil {
		return false
	}
	id, ok := e.g.w.ArmorTypeIndex(name)
	if !ok {
		return false
	}
	e.ctx.ArmorType = id
	return true
}

// Flags returns the packet flag bits (read-only; bit 0 = from a weapon attack).
func (e *DamageEvent) Flags() int {
	if e.ctx != nil {
		return int(e.ctx.Flags)
	}
	return 0
}

// ApplyCoefficient re-derives the amount from the raw value using the current
// attack/armor type — call after SetAttackType so the new coefficient lands.
// No-op in legacy mode.
func (e *DamageEvent) ApplyCoefficient() {
	if e.ctx != nil {
		e.ctx.ApplyCoefficient()
	}
}

// OnDamage registers a synchronous pre-apply damage modifier (#219). Modifiers
// run in registration order on every applied hit, in the base script-modifier
// stage; each may read and SetAmount. Nil-safe.
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

// DamageStageSpec is one named stage of a programmable damage formula (#475):
// a stable name (the identity that hashes + saves) and a script transform over
// the live DamageEvent.
type DamageStageSpec struct {
	Name string
	Fn   func(*DamageEvent)
}

// ReplaceDamageStage swaps the single named stage of the damage pipeline (#473)
// with a script transform that reads/writes the full DamageEvent. Setup verb
// (R-API-5): fails closed on an unknown stage name or nil game/fn. The override
// identity hashes + serializes; on load the world must re-bind the same stage
// name (re-run setup) or LoadState fails closed.
func (g *Game) ReplaceDamageStage(name string, fn func(*DamageEvent)) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: ReplaceDamageStage: nil game")
	}
	if fn == nil {
		return fmt.Errorf("api: ReplaceDamageStage: nil stage fn")
	}
	if err := g.w.ReplaceStage(name, func(c *sim.DamageCtx) {
		de := DamageEvent{ctx: c, g: g}
		fn(&de)
	}); err != nil {
		return fmt.Errorf("api: ReplaceDamageStage: %w", err)
	}
	g.logSetup("ReplaceDamageStage: %q replaced by a script stage", name)
	return nil
}

// SetDamageFormula installs a whole ordered list of script stages, replacing
// the base pipeline (#473). Setup verb (R-API-5): fails closed on an empty list
// or a stage with an empty name / nil fn.
func (g *Game) SetDamageFormula(stages []DamageStageSpec) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: SetDamageFormula: nil game")
	}
	out := make([]sim.DamageStage, len(stages))
	for i := range stages {
		spec := stages[i]
		if spec.Name == "" || spec.Fn == nil {
			return fmt.Errorf("api: SetDamageFormula: stage %d has empty name or nil fn", i)
		}
		fn := spec.Fn
		out[i] = sim.DamageStage{Name: spec.Name, Fn: func(c *sim.DamageCtx) {
			de := DamageEvent{ctx: c, g: g}
			fn(&de)
		}}
	}
	if err := g.w.SetDamageFormula(out); err != nil {
		return fmt.Errorf("api: SetDamageFormula: %w", err)
	}
	g.logSetup("SetDamageFormula: %d script stages installed", len(out))
	return nil
}

// SetArmorReduction sets the armor-reduction coefficient (#474) — the
// positive-branch k in 1/(1+a·k). Setup verb (R-API-5): fails closed on a
// non-positive value. The #330 negative-armor curve is preserved.
func (g *Game) SetArmorReduction(coefficient float64) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: SetArmorReduction: nil game")
	}
	if err := g.w.SetArmorCoefficient(fromFloat(coefficient)); err != nil {
		return fmt.Errorf("api: SetArmorReduction: %w", err)
	}
	g.logSetup("SetArmorReduction: coefficient %.4f", coefficient)
	return nil
}
