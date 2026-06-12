package data

// Effect-primitive registry and composition compiler (ADR #294,
// combat stack #296). An ability's (or weapon's) behavior is DATA: a
// composition tree of effect-primitive invocations that compiles at
// load into a flat arena of CompiledEffect records. The data side —
// this file — owns the primitive NAME/PARAM schemas and all
// validation; litd/sim registers the executable behavior for the same
// primitive IDs and a cross-check test keeps the two from drifting
// (the #146 smart-order pattern).
//
// Everything fails closed at load: unknown primitive, unknown or
// missing param, out-of-range value, nesting deeper than
// MaxEffectDepth, worst-case invocation fan-out beyond
// MaxEffectInvocations, or an empty composition is a LOAD ERROR,
// never a runtime fallback.

import (
	"fmt"
	"math"
	"sort"
)

// EffectPrimID indexes the closed primitive registry. The numbering
// is part of the data fingerprint — appending is safe, reordering is
// a sim-version change.
type EffectPrimID uint16

const (
	EPDamage EffectPrimID = iota
	EPHeal
	EPApplyBuff
	EPModifyStat
	EPSpawnMissile
	EPSummon
	EPTeleport
	EPArea  // combinator: every valid target in radius
	EPChain // combinator: hop target-to-target
	EPFork  // combinator: replicate N times
	EffectPrimCount
)

// Composition limits (#296 design, load-validated — static data needs
// static validation, not runtime budgets).
const (
	MaxEffectDepth       = 4
	MaxEffectInvocations = 256
	MaxEffectParams      = 6
)

// EffectParamKind drives authoring-value conversion.
type EffectParamKind uint8

const (
	EPKInt        EffectParamKind = iota // stored verbatim
	EPKFixed                             // authoring float (world units etc.) → fixed.F64 bits
	EPKPermille                          // integer per-mille
	EPKAttackType                        // name → damage-matrix row index
)

// EffectParamDef is one named parameter of a primitive.
type EffectParamDef struct {
	Name     string
	Kind     EffectParamKind
	Min, Max int64 // bounds on the CONVERTED value
	Required bool
	Default  int64 // used when !Required and absent
}

// EffectSchema describes one primitive to the loader.
type EffectSchema struct {
	Name       string
	Params     []EffectParamDef
	Combinator bool   // carries a nested "effects" list
	FanOut     string // combinator param bounding child invocations
}

// EffectSchemas is the closed v1 registry. litd/sim's exec table is
// indexed by the same IDs; TestEffectRegistryAgreement guards the
// pair.
var EffectSchemas = [EffectPrimCount]EffectSchema{
	EPDamage: {Name: "damage", Params: []EffectParamDef{
		{Name: "amount", Kind: EPKInt, Min: 0, Max: 1 << 20, Required: true},
		{Name: "dice", Kind: EPKInt, Min: 0, Max: 32},
		{Name: "sides", Kind: EPKInt, Min: 0, Max: 1024},
		{Name: "attack-type", Kind: EPKAttackType, Required: true},
	}},
	EPHeal: {Name: "heal", Params: []EffectParamDef{
		{Name: "amount", Kind: EPKInt, Min: 0, Max: 1 << 20, Required: true},
	}},
	EPApplyBuff: {Name: "apply-buff", Params: []EffectParamDef{
		{Name: "buff", Kind: EPKInt, Min: 0, Max: math.MaxUint16, Required: true},
		{Name: "duration", Kind: EPKFixed, Min: 0, Max: math.MaxInt64, Required: true},
		{Name: "stacks", Kind: EPKInt, Min: 1, Max: 255, Default: 1},
	}},
	EPModifyStat: {Name: "modify-stat", Params: []EffectParamDef{
		{Name: "stat", Kind: EPKInt, Min: 0, Max: 64, Required: true},
		{Name: "delta", Kind: EPKInt, Min: math.MinInt32, Max: math.MaxInt32, Required: true},
		{Name: "duration", Kind: EPKFixed, Min: 0, Max: math.MaxInt64},
	}},
	EPSpawnMissile: {Name: "spawn-missile", Params: []EffectParamDef{
		{Name: "missile-type", Kind: EPKInt, Min: 0, Max: math.MaxUint16, Required: true},
	}, Combinator: true, FanOut: ""}, // payload runs once on impact
	EPSummon: {Name: "summon", Params: []EffectParamDef{
		{Name: "unit-type", Kind: EPKInt, Min: 0, Max: math.MaxUint16, Required: true},
		{Name: "count", Kind: EPKInt, Min: 1, Max: 32, Default: 1},
	}},
	EPTeleport: {Name: "teleport", Params: []EffectParamDef{
		{Name: "range", Kind: EPKFixed, Min: 0, Max: math.MaxInt64, Required: true},
	}},
	EPArea: {Name: "area", Params: []EffectParamDef{
		{Name: "radius", Kind: EPKFixed, Min: 1, Max: math.MaxInt64, Required: true},
		{Name: "max-targets", Kind: EPKInt, Min: 1, Max: 64, Required: true},
	}, Combinator: true, FanOut: "max-targets"},
	EPChain: {Name: "chain", Params: []EffectParamDef{
		{Name: "hops", Kind: EPKInt, Min: 1, Max: 16, Required: true},
		{Name: "falloff-permille", Kind: EPKPermille, Min: 0, Max: 1000, Default: 1000},
		{Name: "range", Kind: EPKFixed, Min: 1, Max: math.MaxInt64, Required: true},
	}, Combinator: true, FanOut: "hops"},
	EPFork: {Name: "fork", Params: []EffectParamDef{
		{Name: "count", Kind: EPKInt, Min: 1, Max: 16, Required: true},
	}, Combinator: true, FanOut: "count"},
}

// effectPrimByName is derived once; closed set.
var effectPrimByName = func() map[string]EffectPrimID {
	m := make(map[string]EffectPrimID, EffectPrimCount)
	for id := EffectPrimID(0); id < EffectPrimCount; id++ {
		m[EffectSchemas[id].Name] = id
	}
	return m
}()

// CompiledEffect is one flattened invocation. Children of a
// combinator occupy [ChildOff, ChildOff+ChildLen) in the same arena.
type CompiledEffect struct {
	Prim     EffectPrimID
	Params   [MaxEffectParams]int64 // schema order
	ChildOff uint16
	ChildLen uint16
}

// EffectList references a composition inside Tables.Effects.
type EffectList struct {
	Off uint16
	Len uint16
}

// compileEffects flattens a raw composition tree into the arena,
// validating everything. ctx carries table state needed by enum
// params (attack-type names).
type effectCompiler struct {
	file        string
	attackTypes []string
	arena       []CompiledEffect
}

func (c *effectCompiler) compile(where string, raw []map[string]any, depth int) (EffectList, int, error) {
	if len(raw) == 0 {
		return EffectList{}, 0, fmt.Errorf("data: %s: %s: effects list must be non-empty", c.file, where)
	}
	if depth > MaxEffectDepth {
		return EffectList{}, 0, fmt.Errorf("data: %s: %s: effect nesting exceeds max depth %d", c.file, where, MaxEffectDepth)
	}
	// Reserve this level's slots contiguously, then fill (children
	// land after, referenced by offset).
	off := len(c.arena)
	if off+len(raw) > math.MaxUint16 {
		return EffectList{}, 0, fmt.Errorf("data: %s: %s: effect arena overflow", c.file, where)
	}
	c.arena = append(c.arena, make([]CompiledEffect, len(raw))...)
	totalInvocations := 0
	for i, node := range raw {
		ce, childRaw, fanOut, err := c.compileNode(fmt.Sprintf("%s[%d]", where, i), node)
		if err != nil {
			return EffectList{}, 0, err
		}
		selfInv := 1
		if childRaw != nil {
			childList, childInv, err := c.compile(fmt.Sprintf("%s[%d].effects", where, i), childRaw, depth+1)
			if err != nil {
				return EffectList{}, 0, err
			}
			ce.ChildOff, ce.ChildLen = childList.Off, childList.Len
			selfInv = 1 + fanOut*childInv
		}
		c.arena[off+i] = ce
		totalInvocations += selfInv
	}
	return EffectList{Off: uint16(off), Len: uint16(len(raw))}, totalInvocations, nil
}

// compileNode validates one invocation against its schema.
func (c *effectCompiler) compileNode(where string, node map[string]any) (CompiledEffect, []map[string]any, int, error) {
	fail := func(err error) (CompiledEffect, []map[string]any, int, error) {
		return CompiledEffect{}, nil, 0, err
	}
	primAny, ok := node["prim"]
	if !ok {
		return fail(fmt.Errorf("data: %s: %s: missing \"prim\"", c.file, where))
	}
	primName, ok := primAny.(string)
	if !ok {
		return fail(fmt.Errorf("data: %s: %s: \"prim\" must be a string", c.file, where))
	}
	prim, ok := effectPrimByName[primName]
	if !ok {
		return fail(fmt.Errorf("data: %s: %s: %q is not a registered effect primitive (ADR #294 registry)", c.file, where, primName))
	}
	schema := &EffectSchemas[prim]

	ce := CompiledEffect{Prim: prim}
	seen := map[string]bool{"prim": true}
	fanOut := 1
	for pi, def := range schema.Params {
		v, present := node[def.Name]
		seen[def.Name] = true
		if !present {
			if def.Required {
				return fail(fmt.Errorf("data: %s: %s: %s: missing required param %q", c.file, where, primName, def.Name))
			}
			ce.Params[pi] = def.Default
			continue
		}
		conv, err := c.convertParam(def, v)
		if err != nil {
			return fail(fmt.Errorf("data: %s: %s: %s.%s: %w", c.file, where, primName, def.Name, err))
		}
		// Enum kinds validate by membership in the converter; Min/Max
		// bounds apply to numeric kinds only.
		if def.Kind != EPKAttackType && (conv < def.Min || conv > def.Max) {
			return fail(fmt.Errorf("data: %s: %s: %s.%s: value %d out of range [%d, %d]", c.file, where, primName, def.Name, conv, def.Min, def.Max))
		}
		ce.Params[pi] = conv
		if schema.Combinator && def.Name == schema.FanOut {
			fanOut = int(conv)
		}
	}
	var childRaw []map[string]any
	if schema.Combinator {
		seen["effects"] = true
		eff, ok := node["effects"]
		if !ok {
			return fail(fmt.Errorf("data: %s: %s: combinator %q requires an \"effects\" list", c.file, where, primName))
		}
		childRaw, ok = asEffectSlice(eff)
		if !ok {
			return fail(fmt.Errorf("data: %s: %s: %s.effects must be a list of effect tables", c.file, where, primName))
		}
	}
	// unknown-key strictness (decodeStrict can't see inside the maps)
	var unknown []string
	for k := range node {
		if !seen[k] {
			unknown = append(unknown, k)
		}
	}
	if len(unknown) > 0 {
		sort.Strings(unknown)
		return fail(fmt.Errorf("data: %s: %s: %s: unknown param %q (schema rejects unrecognized keys)", c.file, where, primName, unknown[0]))
	}
	return ce, childRaw, fanOut, nil
}

func (c *effectCompiler) convertParam(def EffectParamDef, v any) (int64, error) {
	switch def.Kind {
	case EPKInt, EPKPermille:
		n, ok := asInt64(v)
		if !ok {
			return 0, fmt.Errorf("must be an integer (got %T)", v)
		}
		return n, nil
	case EPKFixed:
		f, ok := asFloat64(v)
		if !ok {
			return 0, fmt.Errorf("must be a number (got %T)", v)
		}
		fx, err := worldUnits(f)
		if err != nil {
			return 0, err
		}
		return int64(fx), nil
	case EPKAttackType:
		s, ok := v.(string)
		if !ok {
			return 0, fmt.Errorf("must be an attack-type name (got %T)", v)
		}
		idx := indexOf(c.attackTypes, s)
		if idx < 0 {
			return 0, fmt.Errorf("%q is not a damage-matrix attack type %v", s, c.attackTypes)
		}
		return int64(idx), nil
	}
	return 0, fmt.Errorf("unhandled param kind %d", def.Kind)
}

// asEffectSlice normalizes the decoder's representation of an effects
// list ([]map[string]any from TOML, []any from JSON).
func asEffectSlice(v any) ([]map[string]any, bool) {
	switch s := v.(type) {
	case []map[string]any:
		return s, true
	case []any:
		out := make([]map[string]any, len(s))
		for i, e := range s {
			m, ok := e.(map[string]any)
			if !ok {
				return nil, false
			}
			out[i] = m
		}
		return out, true
	}
	return nil, false
}

func asInt64(v any) (int64, bool) {
	switch n := v.(type) {
	case int64:
		return n, true
	case float64: // JSON numbers; accept only integral values
		if n == math.Trunc(n) {
			return int64(n), true
		}
	}
	return 0, false
}

func asFloat64(v any) (float64, bool) {
	switch n := v.(type) {
	case int64:
		return float64(n), true
	case float64:
		return n, true
	}
	return 0, false
}

// hashInto folds the compiled arena into the fingerprint stream.
func hashEffects(h interface {
	WriteU16(uint16)
	WriteI64(int64)
}, arena []CompiledEffect) {
	h.WriteU16(uint16(len(arena)))
	for i := range arena {
		e := &arena[i]
		h.WriteU16(uint16(e.Prim))
		for _, p := range e.Params {
			h.WriteI64(p)
		}
		h.WriteU16(e.ChildOff)
		h.WriteU16(e.ChildLen)
	}
}
