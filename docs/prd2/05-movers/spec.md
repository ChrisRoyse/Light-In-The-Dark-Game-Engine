# Movers — Specification

## 1. Data model

```go
// litd/sim/mover.go (new). SoA pool, Caps.Movers, shared by units & projectiles.

type MoverID uint32 // [ generation:8 | index:24 ]; stale ⇒ no-op

type MoverKind uint8
const (
    MoverLinear MoverKind = iota
    MoverHoming
    MoverPoint
    MoverOrbitUnit
    MoverOrbitPoint
    MoverArc        // ballistic (gameplay-affecting), not the render-only Arc
    MoverSpline
    MoverCustom
)

type MoverStore struct {
    Kind   []uint8

    // The transform this mover drives. Exactly one of:
    //  - a unit/projectile EntityID whose Transforms row we write, OR
    //  - (future) a detached transform slot. v1: always an EntityID.
    Target []EntityID

    // Anchors / goals (interpretation depends on Kind — see mover-types.md)
    Anchor   []EntityID    // orbit-unit anchor, homing target
    Goal     []fixed.Vec2  // point goal, orbit-point center, arc landing
    Dir      []fixed.Vec2  // linear direction (unit vector in fixed)

    // Scalars
    Speed    []fixed.F64   // world units per tick (linear/homing/point/arc)
    Accel    []fixed.F64   // per tick^2
    Radius   []fixed.F64   // orbit radius; also collision radius (collision-and-impact.md)
    AngVel   []fixed.Angle // orbit angular velocity per tick
    Angle    []fixed.Angle // orbit current angle; spline param accumulator
    RangeLeft[]fixed.F64   // linear remaining range; arc total handled via param
    Height   []fixed.F64   // orbit z / arc apex (gameplay z when flying)
    TurnRate []fixed.Angle // homing max turn per tick (0 = instant)

    // Spline waypoints: index span into a shared waypoint arena.
    WpStart  []int32
    WpLen    []int32
    WpParam  []fixed.F64   // 0..WpLen-1 progress along the spline

    // Custom: a continuation that computes the next position from State.
    Cont     []uint16      // ContID for MoverCustom step
    CState   [][4]int64    // value payload for the custom step (serializable)

    // Collision & completion policy (collision-and-impact.md)
    HitMask  []uint16
    Pierce   []int32
    Decay    []uint16      // per-mille payload decay between hits
    Payload  []data.EffectList
    Packet   []DamagePacket
    OnDone   []uint16      // ContID invoked at completion (0 = none)
    DoneMode []uint8       // MoverDoneExpire | Loop | Detonate | Cont
    Flags    []uint8       // MoverAuthority (owns unit transform), MoverFlying, ...

    Owner    []EntityID    // for auto-cancel on death (R-MOV-10)
    Gen      []uint8
    live     []bool
    free     []int32
    count    int32

    waypoints []fixed.Vec2 // shared arena for spline points (Caps-bounded)
}
```

All motion fields are **fixed-point** (`fixed.F64`, `fixed.Vec2`, `fixed.Angle`); no float
ever (R-MOV-3). This matches `fixed/fixed.go` (32.32) and the existing missile/movement
math.

## 2. The advance loop (phase 4)

Each tick, in slot-ascending order (canonical, deterministic), every live mover advances
its target's transform:

```
for each live mover m (ascending slot):
    pos := Transforms.Pos[ rowOf(m.Target) ]
    next := stepPosition(m, pos)        // kind-specific; see mover-types.md
    if m has collision policy:
        hits := sweptCollision(pos, next, m)   // collision-and-impact.md
        for each hit (deterministic order): applyPayload(m, hit); decrement Pierce
        if Pierce exhausted: complete(m, Detonate?)
    Transforms.Pos[ rowOf(m.Target) ] = next
    Transforms.Facing[...] = facingFor(m, pos, next)   // optional
    accelerate(m)                        // Speed += Accel (clamped)
    if reachedCompletion(m): complete(m)
```

- **Ordering.** Slot-ascending iteration is the canonical order; because slot assignment is
  deterministic, advance order is deterministic. Movers do not read each other's
  mid-advance state in a way that depends on order beyond this (collision queries read the
  collision grid as of phase start).
- **Snap-arrival.** Point/arc/homing arrival uses the exact 128-bit `DistSq`/`RadiusSq`
  compare (as `missile.go:274-276` does today) to never overshoot.
- **Fixed-point normalization.** Direction/step normalization reuses `unitStep`
  (`movement.go:95-122`) so units and missiles share one integrator.

## 3. Movement authority over units (R-MOV-7)

A mover with `MoverAuthority` set **owns its target unit's transform** for its lifetime:

- While authority is held, the unit's normal pathing/movement system is suspended for that
  unit (a per-unit `authorityMover` field on the movement row, or the mover's presence in a
  reverse index, gates `phaseMovement` from also writing the transform).
- On mover completion/cancel, authority releases and pathing resumes (the unit re-acquires
  its order or idles).
- This is how knockback, pull, dash, charge, leap, and orbit drive units without fighting
  the pathfinder. Collision during an authority move respects terrain pathing flags if
  `MoverFlying` is unset (a knockback stops at a cliff), or ignores them if set (a leap).

> **Conflict rule.** At most one authority mover per unit. Attaching a second authority
> mover to a unit cancels the first (deterministic, last-wins) and is recorded.

## 4. Completion policy (R-MOV-5)

`DoneMode` selects what happens when the path ends (point reached, range exhausted, spline
finished, count done):

| DoneMode | Behavior |
|----------|----------|
| `Expire` | free the mover; if the target is a projectile entity, kill it |
| `Loop` | reset the parameter (orbit angle wraps; spline restarts; linear re-extends) — continues forever until cancelled |
| `Detonate` | run `Payload` at the final position (AoE), emit impact, then expire |
| `Cont` | invoke `OnDone` continuation with `CState`, then expire |

Completion is deterministic and the continuation form is serializable (ContID + CState),
just like timers.

## 5. Lifecycle & ownership (R-MOV-10)

- A mover bound to a **projectile** entity is consumed when the projectile dies (impact or
  expiry) — the projectile and mover free together.
- A mover bound to a **unit** with `Owner` set auto-cancels when the owner dies (cleanup
  phase), releasing authority.
- Cancel is idempotent and generation-checked.

## 6. Superseding missiles (R-MOV-6) — migration plan

The existing `MissileStore` (`store_missile.go`) is **refactored into the mover system**:

1. `MissileGuidanceLinear/Homing/Point` become `MoverLinear/Homing/Point`. The
   `MissileSpec` launch path constructs a mover + a lightweight projectile entity (model,
   team, render hooks) instead of a missile row.
2. The linear swept-collision code (`missile.go:333-376`) moves verbatim into the mover
   collision module (it is already exactly what `MoverLinear` needs).
3. Impact modes (`Deliver/Detonate/Pierce/Expire`) map onto `DoneMode` + collision policy.
4. The `"missiles"` sub-hash is **retired**; motion hashes once under `"movers"` (R-MOV-9).
   The save-format version bumps; old saves upgrade missile rows into mover rows.
5. The render layer's missile interpolation reads the projectile entity's transform (which
   the mover drives) — no render API change beyond sourcing position from the mover-driven
   transform.

This is the largest *supersession* in PRD2 and is sequenced last in the roadmap so it lands
on top of timers/groups/KV/events, which its completion and payload paths use.

> **Why supersede rather than add alongside?** Two motion systems mean two hashing paths,
> two save formats, two collision implementations, and an authoring fork ("is my thing a
> missile or a mover?"). The user's robustness goal is best served by **one** motion model
> that everything — units and projectiles — flows through.

## 7. Capacity & exhaustion (R-MOV-8)

- `Caps.Movers` (4,096) bounds concurrent movers across units + projectiles.
- A spline waypoint arena is bounded too; over-long splines clamp + counter.
- Exhaustion ⇒ invalid `MoverID` + `moverDropped++`. An ability that cannot attach a mover
  falls back to an instantaneous effect (deterministic degradation). No panic, no alloc.

## 8. Hashing (R-MOV-9)

Sub-hash `"movers"`: write `count`, `moverDropped`; iterate live movers in slot-ascending
order writing every motion/collision/completion column in declaration order, fixed-point
values as their int64 bits; then the spline waypoint arena spans + points; then the
free-list. Mirrors the save block. Render-only interpolation state is never hashed.
