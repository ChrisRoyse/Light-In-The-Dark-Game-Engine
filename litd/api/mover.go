package litd

// Mover noun (PRD2 05, #591) — the public surface over the sim unified
// motion controller. ONE serializable fixed-point mover drives a unit OR a
// spawned projectile across eight motion kinds, carrying collision policy
// and an effect payload, so `cast → spawn → move → collide → effect` is
// composable from script. Mover is a value handle {*Game, MoverID} that
// resolves through the generation-checked store — a stale handle is a
// detectable, safe no-op on every method (R-API-5).
//
// All world-unit inputs are float64 (converted to fixed once at the
// boundary); angular inputs are the api Angle (BAM). Pool exhaustion or a
// degenerate spec fails with the zero-value handle + a debug report, never
// a silent fallback.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// MoverDone selects what happens when a mover's motion completes.
type MoverDone uint8

const (
	MoverDoneExpire   MoverDone = MoverDone(sim.MoverDoneExpire)   // free the mover
	MoverDoneLoop     MoverDone = MoverDone(sim.MoverDoneLoop)     // re-arm and keep going
	MoverDoneDetonate MoverDone = MoverDone(sim.MoverDoneDetonate) // AoE payload at the end, then free
	MoverDoneCont     MoverDone = MoverDone(sim.MoverDoneCont)     // run an OnDone continuation, then free
	MoverDoneImpact   MoverDone = MoverDone(sim.MoverDoneImpact)   // single-shot payload once at the end (missile point/homing impact), then free
)

// MoverHitMask filters mover collision candidates (same class/team bits as
// the missile mask). The zero mask collides with nothing.
type MoverHitMask uint16

const (
	MoverHitGround    MoverHitMask = MoverHitMask(sim.MissileHitGround)
	MoverHitAir       MoverHitMask = MoverHitMask(sim.MissileHitAir)
	MoverHitStructure MoverHitMask = MoverHitMask(sim.MissileHitStructure)
	MoverHitEnemy     MoverHitMask = MoverHitMask(sim.MissileHitEnemy)
	MoverHitAlly      MoverHitMask = MoverHitMask(sim.MissileHitAlly)
)

// MoverOptions is the one option set for every Move* verb and
// SpawnProjectile (R-API-3). Each verb reads the subset its kind needs;
// the rest stay zero. The collision/payload/completion fields are shared.
type MoverOptions struct {
	// Target is the unit/projectile whose transform the mover drives. For
	// SpawnProjectile it is filled with the freshly spawned body.
	Target Unit

	// Motion inputs (interpretation per verb):
	Anchor    Unit    // homing target / orbit-unit anchor
	Goal      Vec2    // point goal / orbit-point center / arc landing
	Direction Angle   // linear heading
	Speed     float64 // world units per tick (spline: param units per tick)
	Accel     float64 // world units per tick^2
	Radius    float64 // orbit radius AND collision radius
	AngVel    Angle   // orbit angular velocity per tick
	Height    float64 // orbit z / arc apex
	TurnRate  Angle   // homing max turn per tick (0 = instant)
	Range     float64 // linear remaining range
	Waypoints []Vec2  // spline control points

	// Custom (MoveCustom): a registered step ContID + its [4]int64 state.
	Custom uint16
	CState [4]int64

	// Collision + payload:
	Owner   Unit         // caster — auto-cancels the mover on its death; scopes team
	HitMask MoverHitMask // which classes/teams to hit
	Damage  float64      // packet amount delivered per hit (when no effect list)
	Pierce  int          // hits before the mover is consumed (>= 1)
	Decay   int          // per-mille damage decay between hits (0..1000)

	// Completion:
	Done   MoverDone
	OnDone uint16 // MoverDoneCont: continuation ContID

	// Behavior flags:
	Authority bool // owns the Target unit's transform (suspends its pathing)
	Flying    bool // ignore ground terrain collision
	Consume   bool // kill the Target entity on completion (projectile body)

	// Presentation (non-hashing): a model id shown on the body via the
	// render-effect channel. 0 = none.
	FX uint16
}

// Mover is a value handle over a sim MoverID.
type Mover struct {
	id sim.MoverID
	g  *Game
}

// Valid reports whether the mover still exists.
func (m Mover) Valid() bool { return m.g != nil && m.g.w != nil && m.g.w.Movers.Alive(m.id) }

// IsZero reports whether this is the zero-value handle.
func (m Mover) IsZero() bool { return m == Mover{} }

// Cancel stops and frees the mover. Idempotent; stale ⇒ no-op.
func (m Mover) Cancel() {
	if m.g != nil && m.g.w != nil {
		m.g.w.Movers.Cancel(m.id)
	}
}

// baseSpec builds the shared collision/payload/completion/flag fields.
func (g *Game) baseSpec(o MoverOptions) sim.MoverSpec {
	var flags uint8
	if o.Authority {
		flags |= sim.MoverAuthority
	}
	if o.Flying {
		flags |= sim.MoverFlying
	}
	if o.Consume {
		flags |= sim.MoverConsume
	}
	return sim.MoverSpec{
		Target:   o.Target.id,
		Owner:    o.Owner.id,
		Speed:    fromFloat(o.Speed),
		Accel:    fromFloat(o.Accel),
		Radius:   fromFloat(o.Radius),
		Height:   fromFloat(o.Height),
		HitMask:  uint16(o.HitMask),
		Pierce:   int32(o.Pierce),
		Decay:    uint16(o.Decay),
		Packet:   sim.DamagePacket{Source: o.Owner.id, Amount: fromFloat(o.Damage)},
		DoneMode: sim.MoverDoneMode(o.Done),
		OnDone:   o.OnDone,
		Flags:    flags,
	}
}

func (g *Game) create(spec sim.MoverSpec, what string) Mover {
	id := g.w.Movers.Create(spec)
	if id == 0 {
		g.reportInvalid(what + " (mover pool exhausted)")
		return Mover{}
	}
	return Mover{id: id, g: g}
}

// MoveLinear drives the Target in a straight line along Direction at Speed
// for Range world units.
func (g *Game) MoveLinear(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverLinear
	s.Dir = dirFromAngle(o.Direction)
	s.RangeLeft = fromFloat(o.Range)
	return g.create(s, "MoveLinear")
}

// MoveHoming drives the Target toward the Anchor unit, turning at most
// TurnRate per tick (0 = instant).
func (g *Game) MoveHoming(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverHoming
	s.Anchor = o.Anchor.id
	s.Dir = dirFromAngle(o.Direction)
	s.TurnRate = angleToBrad(o.TurnRate)
	return g.create(s, "MoveHoming")
}

// MovePoint drives the Target toward Goal, snapping + completing on arrival.
func (g *Game) MovePoint(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverPoint
	s.Goal = vec(o.Goal)
	return g.create(s, "MovePoint")
}

// MoveOrbitUnit circles the Target around the Anchor unit at Radius/AngVel.
func (g *Game) MoveOrbitUnit(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverOrbitUnit
	s.Anchor = o.Anchor.id
	s.Goal = g.unitPos(o.Anchor) // frozen center if the anchor dies
	s.AngVel = angleToBrad(o.AngVel)
	return g.create(s, "MoveOrbitUnit")
}

// MoveOrbitPoint circles the Target around the fixed Goal point.
func (g *Game) MoveOrbitPoint(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverOrbitPoint
	s.Goal = vec(o.Goal)
	s.AngVel = angleToBrad(o.AngVel)
	return g.create(s, "MoveOrbitPoint")
}

// MoveArc drives the Target on a ballistic arc to Goal (horizontal Speed),
// rising to apex Height at the midpoint. The total distance + apex are
// computed from the Target's current position.
func (g *Game) MoveArc(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverArc
	s.Goal = vec(o.Goal)
	total := dist(g.unitPos(o.Target), vec(o.Goal))
	s.CState = [4]int64{int64(total), int64(fromFloat(o.Height))}
	return g.create(s, "MoveArc")
}

// MoveSpline drives the Target along a Catmull-Rom curve through Waypoints,
// advancing Speed param units per tick. Returns the zero handle if the
// waypoint arena is full or fewer than two points are given.
func (g *Game) MoveSpline(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	if len(o.Waypoints) < 2 {
		g.reportInvalid("MoveSpline (need >= 2 waypoints)")
		return Mover{}
	}
	pts := make([]fixed.Vec2, len(o.Waypoints))
	for i, p := range o.Waypoints {
		pts[i] = vec(p)
	}
	start, n, ok := g.w.Movers.AddWaypoints(pts)
	if !ok {
		g.reportInvalid("MoveSpline (waypoint arena full)")
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverSpline
	s.WpStart = start
	s.WpLen = n
	return g.create(s, "MoveSpline")
}

// MoveCustom drives the Target with a step registered via RegisterMoverStep.
func (g *Game) MoveCustom(o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	if o.Custom == 0 {
		g.reportInvalid("MoveCustom (no step ContID)")
		return Mover{}
	}
	s := g.baseSpec(o)
	s.Kind = sim.MoverCustom
	s.Cont = o.Custom
	s.CState = o.CState
	return g.create(s, "MoveCustom")
}

// RegisterMoverStep binds a custom step (deterministic; setup-only). It
// returns the ContID to pass as MoverOptions.Custom.
func (g *Game) RegisterMoverStep(id uint16, fn func(pos Vec2, cs [4]int64) (next Vec2, ncs [4]int64, done bool)) {
	if g == nil || g.w == nil {
		return
	}
	g.w.Movers.RegisterMoverStep(id, func(w *sim.World, pos fixed.Vec2, cs [4]int64) (fixed.Vec2, [4]int64, bool) {
		np, ncs, done := fn(Vec2{X: toFloat(pos.X), Y: toFloat(pos.Y)}, cs)
		return vec(np), ncs, done
	})
}

// SpawnProjectile spawns a transform-only body at Origin and attaches a
// mover that drives it — the composable building block for ability
// projectiles. Defaults Flying + Consume (a fired projectile ignores ground
// terrain and dies on impact) unless overridden by the options. The kind is
// chosen by the verb the caller routes through; SpawnProjectile is a thin
// convenience over MovePoint/MoveLinear/MoveArc with a fresh body.
func (g *Game) SpawnProjectile(origin Vec2, kind ProjectileKind, o MoverOptions) Mover {
	if g == nil || g.w == nil {
		return Mover{}
	}
	body, ok := g.w.CreateUnit(vec(origin), 0)
	if !ok {
		g.reportInvalid("SpawnProjectile (entity pool exhausted)")
		return Mover{}
	}
	o.Target = Unit{id: body, g: g}
	if !o.Authority { // a projectile body has no pathing to suspend
		o.Flying = o.Flying || true
		o.Consume = true
	}
	var m Mover
	switch kind {
	case ProjectilePoint:
		m = g.MovePoint(o)
	case ProjectileArc:
		m = g.MoveArc(o)
	case ProjectileHoming:
		m = g.MoveHoming(o)
	default:
		m = g.MoveLinear(o)
	}
	if m.IsZero() {
		g.w.KillUnit(body) // unwind the orphaned body
		return Mover{}
	}
	if o.FX != 0 {
		g.w.EmitRenderEvent(sim.RenderEffectSpawn, body, o.FX)
	}
	return m
}

// ProjectileKind selects SpawnProjectile's motion.
type ProjectileKind uint8

const (
	ProjectileLinear ProjectileKind = iota
	ProjectilePoint
	ProjectileArc
	ProjectileHoming
)

// unitPos reads a unit's sim position (zero if invalid).
func (g *Game) unitPos(u Unit) fixed.Vec2 {
	if u.g == nil || u.g.w == nil {
		return fixed.Vec2{}
	}
	if r := g.w.Transforms.Row(u.id); r >= 0 {
		return g.w.Transforms.Pos[r]
	}
	return fixed.Vec2{}
}

// dist returns |b-a| in 32.32 (deterministic integer sqrt).
func dist(a, b fixed.Vec2) fixed.F64 {
	d := b.Sub(a)
	return fixed.F64(uint64(fixed.SqrtU64(uint64(d.LenSq()))) << 16)
}
