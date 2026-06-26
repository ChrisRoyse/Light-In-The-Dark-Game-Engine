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

	// Extended mover params (#622) — set on the MoverSpec by attach_mover.
	AngVel    fixed.Angle    // orbit angular velocity, BAM/tick
	TurnRate  fixed.Angle    // homing turn rate, BAM/tick (0 = instant track)
	Height    fixed.F64      // arc apex height
	Decay     uint16         // per-hit damage decay, per-mille
	DoneMode  MoverDoneMode  // on completion: expire/loop/detonate/cont
	OnDone    uint16         // cont id for DoneMode=cont
	Waypoints []fixed.Vec2   // spline control points

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

// The author-facing source form (AbilitySpecSource/OpSource/WaypointSource) and
// the float→fixed lowering (LowerAbilitySpec) live in litd/data (#628) — floats
// stay out of the deterministic sim core. CompileAbilitySpec below consumes the
// already-lowered, fixed-point data.AbilitySpecLowered and only resolves names.

// moverDoneModeNames maps the author done string to its compiled mode.
var moverDoneModeNames = map[string]MoverDoneMode{
	"expire": MoverDoneExpire, "loop": MoverDoneLoop,
	"detonate": MoverDoneDetonate, "cont": MoverDoneCont,
	"impact": MoverDoneImpact,
}

// AbilityResolver resolves author names to compiled ids. Supplied by the
// host at load (the registries are already populated at setup).
type AbilityResolver interface {
	EffectListByName(name string) (data.EffectList, bool)
	EventKindByName(name string) (uint16, bool)
	MoverKindByName(name string) (MoverKind, bool)
	KeyID(name string) uint32 // interns if new (KV keys are open)
}

// CompileAbilitySpec resolves a lowered (fixed-point) spec into the compiled
// record, or an error (whole-spec reject, never a partial load — R-ABL-2). The
// numeric lowering + validation already ran in litd/data.LowerAbilitySpec; this
// only resolves author names (cast type / indicator / op / mover / effect-list /
// event / key) to compiled ids.
func CompileAbilitySpec(src data.AbilitySpecLowered, res AbilityResolver) (AbilitySpec, error) {
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
	ops, err := compileOps(src.ID, src.OnCast, res, 0)
	if err != nil {
		return out, err
	}
	out = AbilitySpec{
		ID: src.ID, Name: src.Name, CastType: ct, Indicator: ind,
		CastRange: src.CastRange, ManaCost: src.ManaCost, Cooldown: src.Cooldown,
		Precast: src.Precast, CastPoint: src.CastPoint, Backswing: src.Backswing, OnCast: ops,
	}
	return out, nil
}

const maxOpDepth = 8

func compileOps(id string, srcs []data.OpLowered, res AbilityResolver, depth int) ([]AbilityOp, error) {
	if depth > maxOpDepth {
		return nil, fmt.Errorf("ability %q: op nesting exceeds %d", id, maxOpDepth)
	}
	out := make([]AbilityOp, 0, len(srcs))
	for i, s := range srcs {
		kind, ok := abilityOpNames[s.Op]
		if !ok {
			return nil, fmt.Errorf("ability %q op[%d]: unknown op %q", id, i, s.Op)
		}
		op := AbilityOp{
			Kind: kind, Cont: s.Cont, Arg: s.Arg, Count: s.Count, HitMask: s.HitMask, Pierce: s.Pierce, OnDone: s.Cont,
			Speed: s.Speed, AngVel: s.AngVel, TurnRate: s.TurnRate, Height: s.Height,
			Decay: s.Decay, Range: s.Range, Radius: s.Radius, Amount: s.Amount, Waypoints: s.Waypoints,
		}
		if s.Done != "" {
			dm, ok := moverDoneModeNames[s.Done]
			if !ok {
				return nil, fmt.Errorf("ability %q op[%d]: unknown done mode %q", id, i, s.Done)
			}
			op.DoneMode = dm
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
