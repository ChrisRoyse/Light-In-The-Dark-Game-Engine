package litd

// Missile noun (public-api-design.md §2 row 21, R-SIM-7) — a LitD
// extension with no JASS analogue. Missiles are independent first-class
// sim objects: spawned from an options struct, queried, and retargeted
// mid-flight, backed by the sim missile pool.
//
// Guidance selects how the missile flies — homing onto a unit, toward a
// fixed point, or as a linear skillshot along a direction. An unknown
// guidance, a degenerate spec, or pool exhaustion fails deterministically
// with the zero-value (invalid) handle and a debug report — never a
// silent fire-as-instant fallback (R-API-5).

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Guidance selects a missile's flight behavior.
type Guidance uint8

const (
	// GuidanceHoming flies toward a target Unit, tracking it mid-flight.
	GuidanceHoming Guidance = iota
	// GuidancePoint flies to a fixed Point and detonates.
	GuidancePoint
	// GuidanceLinear flies as a skillshot along Direction for Range
	// world units, hitting up to Pierce foes of the source's team.
	GuidanceLinear
)

// MissileOptions parameterizes SpawnMissile (R-API-3 options struct).
// Zero options spawn nothing useful — Source, Origin, Speed, and a
// guidance-appropriate aim are the meaningful inputs. Unsupported WC3
// knobs (acceleration, hit-mask, named guidance/impact registries) are
// deferred to follow-up issues and are not yet fields here.
type MissileOptions struct {
	Source Unit // the launcher (its team scopes a linear skillshot's foes)
	Origin Vec2 // spawn position

	Guidance  Guidance
	Target    Unit  // GuidanceHoming: the unit to track
	Point     Vec2  // GuidancePoint: the destination
	Direction Angle // GuidanceLinear: the flight heading
	AoE       bool  // GuidanceHoming: detonate at the last point if the target dies

	Speed  float64 // world units per tick (> 0, required)
	Arc    float64 // presentation arc height (render-only)
	Damage float64 // payload: damage delivered on impact

	Range  float64 // GuidanceLinear: travel before expiry (> 0)
	Pierce int     // GuidanceLinear: max hits (>= 1)
}

// SpawnMissile launches a missile and returns its handle, or the
// zero-value Missile on failure (unknown guidance, degenerate spec, or
// pool exhaustion). JASS: no analogue (R-SIM-7).
//
// Debug mode: a failed spawn is reported through OnInvalidHandle.
func (g *Game) SpawnMissile(o MissileOptions) Missile {
	if g == nil || g.w == nil {
		return Missile{}
	}
	spec := sim.MissileSpec{
		Pos:    vec(o.Origin),
		Source: o.Source.id,
		Speed:  fromFloat(o.Speed),
		Arc:    fromFloat(o.Arc),
		Packet: sim.DamagePacket{Source: o.Source.id, Amount: fromFloat(o.Damage)},
	}
	switch o.Guidance {
	case GuidanceHoming:
		spec.Target = o.Target.id
		if o.AoE {
			spec.Flags |= sim.MissileAoE
		}
	case GuidancePoint:
		spec.Point = vec(o.Point)
	case GuidanceLinear:
		spec.Flags |= sim.MissileLinear
		spec.Dir = dirFromAngle(o.Direction)
		spec.Range = fromFloat(o.Range)
		spec.Pierce = int32(o.Pierce)
	default:
		g.reportInvalid("SpawnMissile (unknown guidance)")
		return Missile{}
	}
	id, ok := g.w.SpawnMissile(spec)
	if !ok {
		g.reportInvalid("SpawnMissile (degenerate spec or pool exhausted)")
		return Missile{}
	}
	return Missile{id: id, g: g}
}

// Position returns the missile's current world position, or the zero
// Vec2 on an invalid handle.
func (m Missile) Position() Vec2 {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Position")
		return Vec2{}
	}
	tr := m.g.w.Transforms.Row(m.id)
	if tr < 0 {
		return Vec2{}
	}
	p := m.g.w.Transforms.Pos[tr]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
}

// Target returns the unit the missile is homing on, or the zero Unit
// for a point/linear missile or an invalid handle.
func (m Missile) Target() Unit {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Target")
		return Unit{}
	}
	r := m.g.w.Missiles.Row(m.id)
	if r < 0 {
		return Unit{}
	}
	return Unit{id: m.g.w.Missiles.GuideEnt[r], g: m.g}
}

// SetTarget retargets the missile mid-flight to home on u (a zero Unit
// clears the target). Converts a linear skillshot into a guided
// missile. No-op on an invalid handle or a dead target.
func (m Missile) SetTarget(u Unit) {
	if !m.Valid() {
		m.g.reportInvalid("Missile.SetTarget")
		return
	}
	m.g.w.SetMissileTarget(m.id, u.id)
}

// Source returns the launching unit (which may already be dead —
// delivery does not need a living source), or the zero Unit on an
// invalid handle.
func (m Missile) Source() Unit {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Source")
		return Unit{}
	}
	r := m.g.w.Missiles.Row(m.id)
	if r < 0 {
		return Unit{}
	}
	return Unit{id: m.g.w.Missiles.Source[r], g: m.g}
}

// Owner returns the player owning the launching unit, or the zero
// Player if the source is unowned or the handle is invalid.
func (m Missile) Owner() Player {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Owner")
		return Player{}
	}
	r := m.g.w.Missiles.Row(m.id)
	if r < 0 {
		return Player{}
	}
	idx := m.g.ownerOf(m.g.w.Missiles.Source[r])
	if idx < 0 {
		return Player{}
	}
	return Player{idx: idx, g: m.g}
}

// Expire removes the missile without delivering its payload, firing
// EventMissileExpired. No-op on an invalid handle.
func (m Missile) Expire() {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Expire")
		return
	}
	m.g.w.ExpireMissile(m.id)
}

// Detonate delivers the missile's payload at its current position
// immediately, firing EventMissileImpact, then removes it. No-op on an
// invalid handle.
func (m Missile) Detonate() {
	if !m.Valid() {
		m.g.reportInvalid("Missile.Detonate")
		return
	}
	m.g.w.DetonateMissile(m.id)
}

// vec converts a public Vec2 to the sim fixed-point vector.
func vec(v Vec2) fixed.Vec2 {
	return fixed.Vec2{X: fromFloat(v.X), Y: fromFloat(v.Y)}
}

// dirFromAngle converts a public Angle to a unit flight direction using
// the sim's deterministic fixed-point trig (never math.Sin, whose last
// bit can vary across architectures). The only float step is the
// radians→brads unit conversion, a plain scale that is exact enough and
// platform-stable.
func dirFromAngle(a Angle) fixed.Vec2 {
	const radToBrad = 65536.0 / (2 * math.Pi)
	b := fixed.Angle(uint16(int64(a.rad * radToBrad)))
	return fixed.Vec2{X: b.Cos(), Y: b.Sin()}
}
