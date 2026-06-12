package sim

// Effect-exec registry — the process half of the ADR #294 effect
// system (#296). litd/data owns the primitive name/param schemas and
// compiles compositions to flat arenas; this file owns the executable
// behavior for the same primitive IDs. Exec funcs are the only
// audited determinism surface: they run inside sim phases only, take
// all randomness from the sim PRNG, and route ordering-sensitive
// writes through deferred buffers (R-SIM-2, ecs §6).
//
// Registration happens at engine init and is then frozen for the sim
// version; the backends land with their owning issues (#152 damage,
// #158 spawn-missile, #160 casts, #162 apply-buff). Fail-closed
// seam: BindEffects refuses any arena that uses a primitive nobody
// registered — data cannot reference behavior that does not exist.

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// EffectCtx is the invocation context an exec receives. Combinator
// execs derive per-target child contexts and recurse through
// RunEffectChildren.
type EffectCtx struct {
	Source EntityID // caster / attacker; 0 for world-sourced effects
	Target EntityID // primary target; 0 for point effects
	Point  fixed.Vec2
	Depth  uint8 // composition depth, root = 0 (load caps at data.MaxEffectDepth)
}

// EffectExec executes one compiled invocation. e.Params is in the
// primitive's schema order (data.EffectSchemas). ctx travels by
// value — pointer contexts escape through the indirect call and
// allocate (R-GC-1).
type EffectExec func(w *World, ctx EffectCtx, e *data.CompiledEffect)

var (
	effectExecs       [data.EffectPrimCount]EffectExec
	effectExecsFrozen bool
)

// RegisterEffectExec installs the behavior for one primitive at
// engine init. Duplicate registration, nil exec, or registration
// after freeze is a programming error — panic, never limp.
func RegisterEffectExec(id data.EffectPrimID, fn EffectExec) {
	if effectExecsFrozen {
		panic(fmt.Sprintf("sim: RegisterEffectExec(%d) after FreezeEffectExecs — the registry is frozen per sim version", id))
	}
	if id >= data.EffectPrimCount {
		panic(fmt.Sprintf("sim: RegisterEffectExec: %d is not a registered primitive ID", id))
	}
	if fn == nil {
		panic(fmt.Sprintf("sim: RegisterEffectExec(%d): nil exec", id))
	}
	if effectExecs[id] != nil {
		panic(fmt.Sprintf("sim: RegisterEffectExec(%d): duplicate registration for %q", id, data.EffectSchemas[id].Name))
	}
	effectExecs[id] = fn
}

// FreezeEffectExecs seals the registry. Called once after engine
// init; the registered set is part of the sim-version contract.
func FreezeEffectExecs() { effectExecsFrozen = true }

// resetEffectExecs is test scaffolding: the registry is package
// state, tests must restore it.
func resetEffectExecs() {
	effectExecs = [data.EffectPrimCount]EffectExec{}
	effectExecsFrozen = false
}

// BindEffects installs a loaded effect arena. Fail-closed: every
// primitive the arena actually uses must have a registered exec —
// a data table referencing an unimplemented primitive is a bind
// error, never a silent runtime no-op.
func (w *World) BindEffects(arena []data.CompiledEffect) error {
	for i := range arena {
		p := arena[i].Prim
		if p >= data.EffectPrimCount {
			return fmt.Errorf("sim: BindEffects: arena[%d] primitive %d out of range", i, p)
		}
		if effectExecs[p] == nil {
			return fmt.Errorf("sim: BindEffects: arena[%d] uses primitive %q but no exec is registered (fail closed)", i, data.EffectSchemas[p].Name)
		}
	}
	w.effects = arena
	return nil
}

// ExecuteEffects runs a compiled composition list against ctx, in
// arena order. Callers are sim phases (FIRE edge #150, missile
// impact #158, cast resolution #160). Zero allocations: contexts
// live on the stack, the arena is read-only.
func (w *World) ExecuteEffects(list data.EffectList, ctx EffectCtx) {
	for i := uint16(0); i < list.Len; i++ {
		e := &w.effects[list.Off+i]
		effectExecs[e.Prim](w, ctx, e)
	}
}

// RunEffectChildren runs a combinator's payload once with the given
// child context (the combinator's exec calls this once per resolved
// target, having set ctx.Target/Point). Depth bookkeeping only —
// the static load validation already capped nesting and fan-out.
func (w *World) RunEffectChildren(e *data.CompiledEffect, ctx EffectCtx) {
	if e.ChildLen == 0 {
		return
	}
	ctx.Depth++
	w.ExecuteEffects(data.EffectList{Off: e.ChildOff, Len: e.ChildLen}, ctx)
}
