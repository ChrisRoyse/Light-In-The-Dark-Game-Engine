package data

// Composable-ability authoring forms + float→fixed lowering (#594/#628). The
// author-facing source uses float64 (seconds / world units / degrees); names are
// strings. LowerAbilitySpec converts every number to fixed-point with R-ABL-2
// fail-closed precision validation, producing the fixed-point AbilitySpecLowered
// (names still strings — litd/sim resolves those to ids).
//
// Floats live HERE, in litd/data (the authoring layer), and never in litd/sim
// (the deterministic core) — the same convention every other authoring form
// (buffs / hero / effects / tech / item / economy) follows, and what determlint
// enforces: no float in the scoped sim packages (hazard §2.3-1).

import (
	"fmt"
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// maxOpDepth bounds op nesting (matches the legacy compiler limit).
const maxOpDepth = 8

// ---- author-facing source form (decoded from TOML/Lua at setup) ----

// AbilitySpecSource is the pre-lower, author-facing ability. Numbers are
// float64 (seconds / world units); references are names. LowerAbilitySpec
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

	Mover   string // attach_mover: kind name
	Effects string // run_effects / mover payload: effect-list name
	Event   string // emit_event: event name
	Key     string // set_kv/get_kv: key name
	Cont    uint16 // after/loop/times: continuation id

	Speed  float64
	Range  float64
	Radius float64
	Amount float64
	Arg    int64

	Count   int
	HitMask uint16
	Pierce  int

	// Extended mover params (#622). Angles in degrees-per-tick; Decay per-mille.
	AngVel    float64
	TurnRate  float64
	Height    float64
	Decay     int
	Done      string // expire | loop | detonate | cont
	Waypoints []WaypointSource

	Children []OpSource
}

// WaypointSource is one author-facing spline control point (world units).
type WaypointSource struct{ X, Y float64 }

// ---- lowered form (fixed-point numbers; references still names) ----

// AbilitySpecLowered is the float-free intermediate: every number is fixed-point
// (durations already in ticks), every reference still a name. litd/sim's
// CompileAbilitySpec resolves the names to ids — it never sees a float.
type AbilitySpecLowered struct {
	ID        string
	Name      string
	CastType  string
	Indicator string
	CastRange fixed.F64
	ManaCost  int32
	Cooldown  uint16 // ticks

	Precast   uint16 // ticks
	CastPoint uint16
	Backswing uint16

	OnCast []OpLowered
}

// OpLowered is one lowered op (fixed numbers, name references).
type OpLowered struct {
	Op string

	Mover   string
	Effects string
	Event   string
	Key     string
	Cont    uint16

	Speed  fixed.F64
	Range  fixed.F64
	Radius fixed.F64
	Amount fixed.F64
	Arg    int64

	Count   int32
	HitMask uint16
	Pierce  int32

	AngVel    fixed.Angle
	TurnRate  fixed.Angle
	Height    fixed.F64
	Decay     uint16
	Done      string
	Waypoints []fixed.Vec2

	Children []OpLowered
}

// LowerAbilitySpec converts an author-facing source to the fixed-point lowered
// form, or an error (whole-spec reject, never a partial — R-ABL-2). Performs all
// numeric validation (finiteness, precision, ranges); name resolution is left to
// litd/sim's CompileAbilitySpec.
func LowerAbilitySpec(src AbilitySpecSource) (AbilitySpecLowered, error) {
	var out AbilitySpecLowered
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
	ops, err := lowerOps(src.ID, src.OnCast, 0)
	if err != nil {
		return out, err
	}
	out = AbilitySpecLowered{
		ID: src.ID, Name: src.Name, CastType: src.CastType, Indicator: src.Indicator,
		CastRange: castRange, ManaCost: int32(src.ManaCost), Cooldown: cd,
		Precast: pre, CastPoint: cp, Backswing: bs, OnCast: ops,
	}
	return out, nil
}

func lowerOps(id string, srcs []OpSource, depth int) ([]OpLowered, error) {
	if depth > maxOpDepth {
		return nil, fmt.Errorf("ability %q: op nesting exceeds %d", id, maxOpDepth)
	}
	out := make([]OpLowered, 0, len(srcs))
	for i, s := range srcs {
		op := OpLowered{
			Op: s.Op, Mover: s.Mover, Effects: s.Effects, Event: s.Event, Key: s.Key,
			Cont: s.Cont, Arg: s.Arg, Count: int32(s.Count), HitMask: s.HitMask,
			Pierce: int32(s.Pierce), Done: s.Done,
		}
		var err error
		if op.Speed, err = fixedExtent(s.Speed, "speed"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.AngVel, err = degToBAM(s.AngVel); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: angvel: %w", id, i, err)
		}
		if op.TurnRate, err = degToBAM(s.TurnRate); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: turnrate: %w", id, i, err)
		}
		if op.Height, err = fixedExact(s.Height, "height"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if s.Decay < 0 || s.Decay > 1000 {
			return nil, fmt.Errorf("ability %q op[%d]: decay %d out of [0,1000] per-mille", id, i, s.Decay)
		}
		op.Decay = uint16(s.Decay)
		for wi, wp := range s.Waypoints {
			x, err := fixedExact(wp.X, "waypoint.x")
			if err != nil {
				return nil, fmt.Errorf("ability %q op[%d] waypoint[%d]: %w", id, i, wi, err)
			}
			y, err := fixedExact(wp.Y, "waypoint.y")
			if err != nil {
				return nil, fmt.Errorf("ability %q op[%d] waypoint[%d]: %w", id, i, wi, err)
			}
			op.Waypoints = append(op.Waypoints, fixed.Vec2{X: x, Y: y})
		}
		if op.Range, err = fixedExtent(s.Range, "range"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.Radius, err = fixedExtent(s.Radius, "radius"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if op.Amount, err = fixedExact(s.Amount, "amount"); err != nil {
			return nil, fmt.Errorf("ability %q op[%d]: %w", id, i, err)
		}
		if len(s.Children) > 0 {
			kids, err := lowerOps(id, s.Children, depth+1)
			if err != nil {
				return nil, err
			}
			op.Children = kids
		}
		out = append(out, op)
	}
	return out, nil
}

// degToBAM converts degrees to the sim binary angle (65536 = full circle),
// rejecting NaN/Inf. Used for angvel/turnrate (per-tick).
func degToBAM(deg float64) (fixed.Angle, error) {
	if math.IsNaN(deg) || math.IsInf(deg, 0) {
		return 0, fmt.Errorf("angle %v is not finite", deg)
	}
	return fixed.Angle(int64(math.Round(deg*65536.0/360.0)) & 0xffff), nil
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

// maxProjectionExtent caps Speed/Range/Radius (world units) so the integer
// swept-collision projection in litd/sim/mover_collision.go cannot overflow
// int64. That projection forms (Δ²)·l2 with l2 ≤ 2·dirIntScale² = 2^21
// (dirIntScale = 1024) and Δ bounded per tick by Speed+Radius; 1e5 keeps every
// term under ~5e16 — ~180× below MaxInt64 — while sitting far above any real
// ability (demo movers travel ≤ 1e3 u/tick). Authoring rejects anything larger,
// so the sim's "overflow-safe" projection invariant holds for all inputs (#631).
const maxProjectionExtent = 1e5

// fixedExtent is fixedExact plus the semantic magnitude bound for the geometry
// fields that feed the integer collision projection. Fail-closed at authoring —
// an absurd Speed/Radius is rejected here, never silently overflowed downstream.
func fixedExtent(f float64, what string) (fixed.F64, error) {
	q, err := fixedExact(f, what)
	if err != nil {
		return 0, err
	}
	if f > maxProjectionExtent || f < -maxProjectionExtent {
		return 0, fmt.Errorf("ability: %s = %v exceeds the %g world-unit projection limit", what, f, maxProjectionExtent)
	}
	return q, nil
}
