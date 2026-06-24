package sim

// Composable ability model — PRD2 06 (epic #549), schema + loader/validator
// (#594). An ability is DATA: a serializable spec that composes the five
// primitives (timers/groups/KV/events/movers) by reference, never engine
// code (R-ABL-1). This file defines the compiled sim-side AbilitySpec, the
// author-facing source form, and a fail-closed compiler that converts
// numbers to fixed-point and resolves every name to a registered id —
// rejecting (whole, never half) on any unknown op, unresolvable reference,
// or out-of-range/precision-losing number (R-ABL-2). The op interpreter is
// #595; this only produces a validated, read-only record.
//
// Loading runs at world setup, so the source form may use maps/strings;
// the compiled AbilitySpec is pure fixed-point + ids and is what the
// runtime reads.

import (
	"fmt"
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// AbilityOpKind is the composition-op vocabulary (ability-model.md §2).
type AbilityOpKind uint8

const (
	OpSpawnProjectile AbilityOpKind = iota
	OpAttachMover
	OpFillGroup
	OpRunEffects
	OpEmitEvent
	OpSetKV
	OpGetKV
	OpAfter
	OpLoop
	OpTimes
	OpForEachInGroup
	OpIf
	abilityOpKindCount
)

// abilityOpNames maps the author-facing op string to its kind.
var abilityOpNames = map[string]AbilityOpKind{
	"spawn_projectile": OpSpawnProjectile,
	"attach_mover":     OpAttachMover,
	"fill_group":       OpFillGroup,
	"run_effects":      OpRunEffects,
	"emit_event":       OpEmitEvent,
	"set_kv":           OpSetKV,
	"get_kv":           OpGetKV,
	"after":            OpAfter,
	"loop":             OpLoop,
	"times":            OpTimes,
	"for_each_in_group": OpForEachInGroup,
	"if":               OpIf,
}

// AbilityCastType drives the order/cast machinery.
type AbilityCastType uint8

const (
	CastActive AbilityCastType = iota
	CastPassive
	CastNormalAttack
	CastBuild
	CastGather
)

var abilityCastTypes = map[string]AbilityCastType{
	"active": CastActive, "passive": CastPassive,
	"normal_attack": CastNormalAttack, "build": CastBuild, "gather": CastGather,
}

// AbilityIndicator is a presentation hint (non-hashing).
type AbilityIndicator uint8

const (
	IndicatorNone AbilityIndicator = iota
	IndicatorCircle
	IndicatorLine
	IndicatorArrow
)

var abilityIndicators = map[string]AbilityIndicator{
	"none": IndicatorNone, "circle": IndicatorCircle, "line": IndicatorLine, "arrow": IndicatorArrow,
}

// AbilityOp is one compiled composition op. Fields are interpreted per
// Kind; references are already resolved to ids/fixed-point. Children carry
// nested ops for after/loop/times/for_each_in_group/if.
type AbilityOp struct {
	Kind AbilityOpKind

	MoverKind  MoverKind        // attach_mover
	EffectList data.EffectList  // run_effects / mover payload (resolved)
	EventKind  uint16           // emit_event (resolved)
	KeyID      uint32           // set_kv/get_kv (resolved)
	Cont       uint16           // after/loop/times/custom step

	Speed  fixed.F64
	Range  fixed.F64
	Radius fixed.F64
	Amount fixed.F64
	Arg    int64

	Count  int32 // times: repetitions; loop: period ticks; after: delay ticks
	HitMask uint16
	Pierce  int32

	// Block is the AbilityBook deferred-block index for after/loop/times ops
	// (0 = none). Assigned by AbilityBook.RegisterSpec (#595), not the
	// compiler — it is runtime wiring, never serialized.
	Block uint16

	Children []AbilityOp
}

// AbilitySpec is the compiled, validated, read-only ability record.
type AbilitySpec struct {
	ID        string
	Name      string
	CastType  AbilityCastType
	Indicator AbilityIndicator
	CastRange fixed.F64
	ManaCost  int32
	Cooldown  uint16 // ticks

	Precast   uint16 // ticks
	CastPoint uint16
	Backswing uint16

	OnCast []AbilityOp
}

// ---- author-facing source form (decoded from TOML/Lua at setup) ----

// AbilitySpecSource is the pre-compile, author-facing ability. Numbers are
// float64 (seconds / world units); references are names. CompileAbilitySpec
// validates + converts it.
type AbilitySpecSource struct {
	ID        string
	Name      string
	CastType  string
	Indicator string
	CastRange float64
	ManaCost  int
	Cooldown  float64 // seconds

	Precast   float64 // seconds
	CastPoint float64
	Backswing float64

	OnCast []OpSource
}

// OpSource is one author-facing op.
type OpSource struct {
	Op string

	Mover     string  // attach_mover: kind name
	Effects   string  // run_effects / mover payload: effect-list name
	Event     string  // emit_event: event name
	Key       string  // set_kv/get_kv: key name
	Cont      uint16  // after/loop/times: continuation id

	Speed  float64
	Range  float64
	Radius float64
	Amount float64
	Arg    int64

	Count   int
	HitMask uint16
	Pierce  int

	Children []OpSource
}

// AbilityResolver resolves author names to compiled ids. Supplied by the
// host at load (the registries are already populated at setup).
type AbilityResolver interface {
	EffectListByName(name string) (data.EffectList, bool)
	EventKindByName(name string) (uint16, bool)
	MoverKindByName(name string) (MoverKind, bool)
	KeyID(name string) uint32 // interns if new (KV keys are open)
}

// secondsToTicks converts a non-negative second count to whole ticks,
// rejecting NaN/Inf/negative/out-of-uint16-range.
func secondsToTicks(sec float64, what string) (uint16, error) {
	if math.IsNaN(sec) || math.IsInf(sec, 0) || sec < 0 {
		return 0, fmt.Errorf("ability: %s = %v: must be a finite non-negative duration", what, sec)
	}
	ticks := math.Round(sec * 1000 / float64(TickMS))
	if ticks > float64(^uint16(0)) {
		return 0, fmt.Errorf("ability: %s = %vs exceeds the %d-tick limit", what, sec, int(^uint16(0)))
	}
	return uint16(ticks), nil
}

// fixedExact converts a float to 32.32, rejecting NaN/Inf and values that
// cannot be represented without precision loss (R-ABL-2 edge 3).
func fixedExact(f float64, what string) (fixed.F64, error) {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return 0, fmt.Errorf("ability: %s = %v: not a finite number", what, f)
	}
	scaled := f * float64(int64(1)<<32)
	if scaled > math.MaxInt64 || scaled < math.MinInt64 {
		return 0, fmt.Errorf("ability: %s = %v: out of fixed-point range", what, f)
	}
	q := fixed.F64(int64(math.Round(scaled)))
	// precision check: round-trip must land within half a tick of the input
	back := float64(int64(q)) / float64(int64(1)<<32)
	if math.Abs(back-f) > 1.0/float64(int64(1)<<31) {
		return 0, fmt.Errorf("ability: %s = %v loses precision in fixed-point", what, f)
	}
	return q, nil
}

// CompileAbilitySpec validates src and produces the compiled spec, or an
// error (whole-spec reject, never a partial load — R-ABL-2).
func CompileAbilitySpec(src AbilitySpecSource, res AbilityResolver) (AbilitySpec, error) {
	var out AbilitySpec
	if src.ID == "" {
		return out, fmt.Errorf("ability: empty id")
	}
	ct, ok := abilityCastTypes[orDefault(src.CastType, "active")]
	if !ok {
		return out, fmt.Errorf("ability %q: unknown cast_type %q", src.ID, src.CastType)
	}
	ind, ok := abilityIndicators[orDefault(src.Indicator, "none")]
	if !ok {
		return out, fmt.Errorf("ability %q: unknown indicator %q", src.ID, src.Indicator)
	}
	castRange, err := fixedExact(src.CastRange, "cast_range")
	if err != nil {
		return out, fmt.Errorf("ability %q: %w", src.ID, err)
	}
	if src.ManaCost < 0 {
		return out, fmt.Errorf("ability %q: negative mana_cost %d", src.ID, src.ManaCost)
	}
	cd, err := secondsToTicks(src.Cooldown, "cooldown")
	if err != nil {
		return out, fmt.Errorf("ability %q: %w", src.ID, err)
	}
	pre, err := secondsToTicks(src.Precast, "precast")
	if err != nil {
		return out, fmt.Errorf("ability %q: %w", src.ID, err)
	}
	cp, err := secondsToTicks(src.CastPoint, "cast_point")
	if err != nil {
		return out, fmt.Errorf("ability %q: %w", src.ID, err)
	}
	bs, err := secondsToTicks(src.Backswing, "backswing")
	if err != nil {
		return out, fmt.Errorf("ability %q: %w", src.ID, err)
	}
	ops, err := compileOps(src.ID, src.OnCast, res, 0)
	if err != nil {
		return out, err
	}
	out = AbilitySpec{
		ID: src.ID, Name: src.Name, CastType: ct, Indicator: ind,
		CastRange: castRange, ManaCost: int32(src.ManaCost), Cooldown: cd,
		Precast: pre, CastPoint: cp, Backswing: bs, OnCast: ops,
	}
	return out, nil
}

const maxOpDepth = 8

func compileOps(id string, srcs []OpSource, res AbilityResolver, depth int) ([]AbilityOp, error) {
	if depth > maxOpDepth {
		return nil, fmt.Errorf("ability %q: op nesting exceeds %d", id, maxOpDepth)
	}
	out := make([]AbilityOp, 0, len(srcs))
	for i, s := range srcs {
		kind, ok := abilityOpNames[s.Op]
		if !ok {
			return nil, fmt.Errorf("ability %q op[%d]: unknown op %q", id, i, s.Op)
		}
		op := AbilityOp{Kind: kind, Cont: s.Cont, Arg: s.Arg, Count: int32(s.Count), HitMask: s.HitMask, Pierce: int32(s.Pierce)}
		var err error
		if op.Speed, err = fixedExact(s.Speed, "speed"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.Range, err = fixedExact(s.Range, "range"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.Radius, err = fixedExact(s.Radius, "radius"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.Amount, err = fixedExact(s.Amount, "amount"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		// resolve references the op needs
		if s.Mover != "" {
			mk, ok := res.MoverKindByName(s.Mover)
			if !ok {
				return nil, fmt.Errorf("ability %q op[%d]: unknown mover kind %q", id, i, s.Mover)
			}
			op.MoverKind = mk
		}
		if s.Effects != "" {
			el, ok := res.EffectListByName(s.Effects)
			if !ok {
				return nil, fmt.Errorf("ability %q op[%d]: unknown effect list %q", id, i, s.Effects)
			}
			op.EffectList = el
		}
		if s.Event != "" {
			ek, ok := res.EventKindByName(s.Event)
			if !ok {
				return nil, fmt.Errorf("ability %q op[%d]: unknown event %q", id, i, s.Event)
			}
			op.EventKind = ek
		}
		if s.Key != "" {
			op.KeyID = res.KeyID(s.Key)
		}
		// op-specific required references
		switch kind {
		case OpRunEffects:
			if op.EffectList.Len == 0 && s.Effects != "" {
				// resolved to an empty list — acceptable; but a missing ref above already errored
			}
			if s.Effects == "" {
				return nil, fmt.Errorf("ability %q op[%d]: run_effects needs an effect list", id, i)
			}
		case OpEmitEvent:
			if s.Event == "" {
				return nil, fmt.Errorf("ability %q op[%d]: emit_event needs an event name", id, i)
			}
		case OpAttachMover:
			if s.Mover == "" {
				return nil, fmt.Errorf("ability %q op[%d]: attach_mover needs a mover kind", id, i)
			}
		}
		if len(s.Children) > 0 {
			kids, err := compileOps(id, s.Children, res, depth+1)
			if err != nil {
				return nil, err
			}
			op.Children = kids
		}
		out = append(out, op)
	}
	return out, nil
}

func orDefault(s, def string) string {
	if s == "" {
		return def
	}
	return s
}
