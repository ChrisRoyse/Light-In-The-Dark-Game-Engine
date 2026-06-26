package sim

// Effect-primitive Action library (ADR #452, #465). The effect primitives
// (litd/sim/damage.go etc. — damage / heal / apply-buff / area / chain /
// fork …) are the engine's one effect vocabulary; this file exposes each as
// a named trigger Action so a trigger can DO a primitive invocation with the
// same params a data ability uses. An Action compiles into the SAME
// CompiledEffect arena (w.effects) the abilities run from, so a trigger
// Action and a data ability that name the same primitive execute
// byte-identically (the #465 parity requirement) — one vocabulary, no
// duplicate behavior.
//
// Registration is fail-closed exactly like BindEffects: an unimplemented
// primitive, an out-of-range id, a combinator/leaf shape mismatch, a missing
// required param, an unknown param, or an out-of-bounds value is a hard error
// at registration — never a silent runtime no-op. The returned HandlerRef
// (#455) is added to a trigger like any other action.

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

// EffectActionParam is one named parameter value of an Action, already
// converted to its stored integer form (enum name→index resolution — attack
// type, buff ref — is the authoring layer's job; the sim Action vocabulary is
// post-conversion). A slice (not a map) keeps the sim free of map iteration
// (R-SIM-2), and params number at most data.MaxEffectParams.
type EffectActionParam struct {
	Name  string
	Value int64
}

// EffectActionSpec describes one trigger Action: the primitive, its params by
// schema name, and — for a combinator (area/chain/fork) — the child payload.
type EffectActionSpec struct {
	Prim     data.EffectPrimID
	Params   []EffectActionParam
	Children []EffectActionSpec
}

// RegisterEffectAction binds spec as a named trigger Action and returns its
// HandlerRef. The action's effect(s) are appended to the world effect arena
// (cold path — call at setup/authoring, not from inside effect execution),
// and the handler runs them with ctx.Source = the event source and
// ctx.Target = the event target. Fail-closed on any invalid spec.
func (w *World) RegisterEffectAction(name string, spec EffectActionSpec) (HandlerRef, error) {
	list, err := w.appendActionLevel([]EffectActionSpec{spec})
	if err != nil {
		return NoHandler, err
	}
	return w.RegisterHandlerID(name, func(w *World, e Event) bool {
		w.ExecuteEffects(list, EffectCtx{Source: e.Src, Target: e.Dst})
		return true
	}), nil
}

// appendActionLevel validates and flattens one contiguous level of specs into
// w.effects, then recurses for each node's children (which land after the
// level, referenced by offset) — mirroring the data compiler's
// reserve-then-fill so the arena layout, and thus the run, is identical to a
// data-compiled composition. On any error it rolls the arena back to its
// pre-call length so a failed registration leaves no partial effects.
func (w *World) appendActionLevel(specs []EffectActionSpec) (data.EffectList, error) {
	off := len(w.effects)
	const maxUint16 = 1<<16 - 1 // ChildOff is uint16; math import is determlint-banned
	if off+len(specs) > maxUint16 {
		return data.EffectList{}, fmt.Errorf("sim: RegisterEffectAction: effect arena overflow (%d)", off+len(specs))
	}
	w.effects = append(w.effects, make([]data.CompiledEffect, len(specs))...)
	for i := range specs {
		ce, err := compileActionNode(specs[i])
		if err != nil {
			w.effects = w.effects[:off] // fail-closed: drop the partial append
			return data.EffectList{}, err
		}
		if len(specs[i].Children) > 0 {
			childList, err := w.appendActionLevel(specs[i].Children)
			if err != nil {
				w.effects = w.effects[:off]
				return data.EffectList{}, err
			}
			ce.ChildOff, ce.ChildLen = childList.Off, childList.Len
		}
		w.effects[off+i] = ce
	}
	return data.EffectList{Off: uint16(off), Len: uint16(len(specs))}, nil
}

// compileActionNode validates one spec node against its schema and builds the
// CompiledEffect (params in schema order). Children are wired by the caller.
func compileActionNode(s EffectActionSpec) (data.CompiledEffect, error) {
	if int(s.Prim) >= int(data.EffectPrimCount) {
		return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: primitive id %d out of range", s.Prim)
	}
	if effectExecs[s.Prim] == nil {
		return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: primitive %q has no registered exec (fail closed)", data.EffectSchemas[s.Prim].Name)
	}
	schema := data.EffectSchemas[s.Prim]
	switch {
	case schema.Combinator && len(s.Children) == 0:
		return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: combinator %q requires a child effects list", schema.Name)
	case !schema.Combinator && len(s.Children) > 0:
		return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: leaf primitive %q cannot have children", schema.Name)
	}

	// reject any param the schema does not declare (typo guard, fail-closed).
	for i := range s.Params {
		if schemaParamIndex(schema, s.Params[i].Name) < 0 {
			return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: primitive %q has no param %q", schema.Name, s.Params[i].Name)
		}
	}

	var ce data.CompiledEffect
	ce.Prim = s.Prim
	for i := range schema.Params {
		def := schema.Params[i]
		v, ok := actionParam(s.Params, def.Name)
		if !ok {
			if def.Required {
				return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: primitive %q missing required param %q", schema.Name, def.Name)
			}
			v = def.Default
		}
		// numeric kinds bound by [Min,Max]; enum kinds (attack-type, buff ref)
		// carry a pre-resolved index the authoring layer already validated.
		if def.Kind != data.EPKAttackType && def.Kind != data.EPKBuffRef && (v < def.Min || v > def.Max) {
			return data.CompiledEffect{}, fmt.Errorf("sim: RegisterEffectAction: primitive %q param %q = %d out of bounds [%d,%d]", schema.Name, def.Name, v, def.Min, def.Max)
		}
		ce.Params[i] = v
	}
	return ce, nil
}

// schemaParamIndex returns the index of param name in the schema, or -1.
func schemaParamIndex(schema data.EffectSchema, name string) int {
	for i := range schema.Params {
		if schema.Params[i].Name == name {
			return i
		}
	}
	return -1
}

// actionParam looks up a param value by name in the spec's param slice.
func actionParam(ps []EffectActionParam, name string) (int64, bool) {
	for i := range ps {
		if ps[i].Name == name {
			return ps[i].Value, true
		}
	}
	return 0, false
}
