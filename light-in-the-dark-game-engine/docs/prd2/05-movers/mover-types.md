# Mover Type Catalog

Each kind defines: the fields it reads, the per-tick `stepPosition` math (fixed-point), the
natural completion condition, and a usage note. All math is `fixed.F64` (32.32) /
`fixed.Vec2` / `fixed.Angle`; distance comparisons use exact 128-bit `DistSq`/`RadiusSq`.

---

## `MoverLinear`

- **Reads:** `Dir` (unit vector), `Speed`, `Accel`, `RangeLeft`, collision policy.
- **Step:** `step = unitStep(Dir, Speed)`; `next = pos + step`; `RangeLeft -= |step|`;
  `Speed += Accel`.
- **Complete:** `RangeLeft <= 0` (and/or pierce exhausted). `DoneMode` decides expire vs
  detonate.
- **Use:** skillshots, bullets. This is the existing missile linear path
  (`missile.go:333-376`) under a mover.

## `MoverHoming`

- **Reads:** `Anchor` (target entity), `Speed`, `Accel`, `TurnRate`.
- **Step:** read `goal = Transforms.Pos[Anchor]` (live each tick); desired direction
  `d = normalize(goal - pos)`; if `TurnRate > 0`, rotate current facing toward `d` by at
  most `TurnRate` (curving homing); `step = unitStep(facingDir, Speed)`; snap-arrive when
  `DistSq(pos, goal) <= RadiusSq(Speed)`.
- **Anchor death:** if `MoverAoE`-equivalent flag set, continue to last-known point and
  detonate; else expire.
- **Use:** seeking missiles; `TurnRate=0` reproduces today's instant-track homing,
  `TurnRate>0` gives the curving homing that was impossible before.

## `MoverPoint`

- **Reads:** `Goal`, `Speed`, `Accel`.
- **Step:** like linear but toward `Goal`; snap-arrive on `DistSq(pos,Goal) <= RadiusSq(Speed)`.
- **Complete:** on arrival. `DoneMode` = Detonate for placed AoE, Cont for dash-to-point.
- **Use:** placed projectiles, dash/blink-to-point (with authority on a unit).

## `MoverOrbitUnit` / `MoverOrbitPoint`

- **Reads:** anchor (`Anchor` unit, or `Goal` point), `Radius`, `AngVel`, `Angle`,
  `Height`, optional `Accel` on `AngVel`.
- **Step:**
  ```
  Angle += AngVel                       // wraps mod 2π in fixed.Angle
  center := (Anchor live pos) or Goal
  offset := Vec2{ Radius * cos(Angle), Radius * sin(Angle) }   // fixed-point trig table
  next  := center + offset
  z     := Height                       // constant or animated
  ```
  `cos/sin` use the deterministic fixed-point trig table already used for facing/angles
  (`fixed.Angle` helpers); no `math.Sin`. The table is a **quarter-wave LUT with power-of-2
  angle units** (free wraparound, no range-reduction branch), `cos(θ)=sin(θ+quarter)` off the
  same table, 16-bit entries, cache-line aligned. This is both the *deterministic* choice
  (hardware float `sin` diverges across platforms) and the *fast* one (~3–30 cycles vs
  20–260 for hardware `fsin`). Rationale + sources:
  [../00-foundations/performance-budget.md §5.1](../00-foundations/performance-budget.md#51-orbitspline-trig--fixed-point-lut-mandatory-and-fast).
- **Complete:** never by itself (`DoneMode=Loop`), or after a `Times`/timer-bounded
  duration; on unit-anchor death, expire.
- **Use:** the orbiting electric ball, shield satellites, a guardian circling a totem.
  Multiple orbit movers at staggered `Angle` make a ring of satellites.

## `MoverArc` (ballistic, gameplay)

- **Reads:** launch `pos`, `Goal`, `Speed` (or flight time), `Height` (apex).
- **Step:** parameterize flight by progress `t ∈ [0,1]` advanced by `Speed`/distance:
  ```
  ground := lerp(launchPos, Goal, t)     // horizontal, fixed-point lerp
  z      := 4 * Height * t * (1 - t)     // parabola apex = Height at t=0.5
  next   := ground ; nextZ := z
  ```
  Unlike the current render-only `Arc`, this z is **gameplay** (a flying mover can clear
  ground collision; impact happens at `t=1`).
- **Complete:** `t >= 1` ⇒ detonate at `Goal`.
- **Use:** lobbed grenades, mortar, catapult, leaping units (authority + arc on a unit).

## `MoverSpline`

- **Reads:** `WpStart`/`WpLen` (Catmull-Rom control points in the waypoint arena),
  `WpParam`, `Speed`.
- **Step:** advance `WpParam` by an arc-length-approximating increment from `Speed`;
  evaluate the Catmull-Rom position at `WpParam` (fixed-point; the spline basis is rational
  with fixed coefficients, no float). Endpoints use duplicated control points or a closed
  loop.
- **Complete:** `WpParam >= WpLen-1` (open) ⇒ `DoneMode`; closed splines loop.
- **Use:** boomerang (out-and-back is a 3-point spline), scripted cinematic projectile
  paths, curved patrols. A boomerang that returns to and re-collides with the thrower is a
  spline whose last control point is the thrower's position, sampled at launch.

## `MoverCustom`

- **Reads:** `Cont` (ContID), `CState [4]int64`.
- **Step:** invoke the registered continuation `step(world, mover, CState) -> nextPos`
  (and optionally mutate `CState`). The continuation must be **pure and deterministic**
  (fixed-point only, no wall-clock, no map iteration). Because it is named by ContID and
  carries `CState`, it **serializes** — the custom step survives save/load, unlike a Lua
  closure integrator.
- **Complete:** the continuation signals done via a `CState` flag or returns a sentinel.
- **Use:** any parametric motion an author can express deterministically — spirals
  (`r` grows with angle), Lissajous figures, sine-weave skillshots, gravity wells. This is
  the escape hatch that keeps the system open-ended without per-map closures.

### Custom-step example (sine-weave skillshot)
```go
g.RegisterMoverStep(StepSineWeave, func(w *sim.World, m sim.MoverID, st *[4]int64) sim.StepOut {
    // st[0]=phase, st[1]=amp, st[2]=baseDirX bits, st[3]=baseDirY bits
    phase := fixed.F64(st[0])
    amp   := fixed.F64(st[1])
    base  := fixed.Vec2{X: fixed.F64(st[2]), Y: fixed.F64(st[3])}
    perp  := base.Perp()
    fwd   := base.Scale(SPEED)
    side  := perp.Scale(amp.Mul(fixedSin(phase)))
    st[0] = int64(phase + PHASE_STEP)
    return sim.StepOut{Delta: fwd.Add(side)}
})
```

---

## Choosing a kind (author cheat-sheet)

| I want… | Kind |
|---------|------|
| a bullet that flies straight and hits the first thing | `Linear` |
| a missile that chases a unit (optionally curving) | `Homing` |
| something placed at a spot, then it goes off | `Point` (Detonate) |
| a ball circling my hero forever | `OrbitUnit` (Loop) |
| a ring of satellites | several `OrbitUnit` at staggered `Angle` |
| a lobbed grenade that arcs over a wall | `Arc` (Flying) |
| a boomerang | `Spline` (out-and-back) |
| a leap/charge that moves my unit | `Point`/`Arc` + `MoverAuthority` |
| a knockback | short `Linear` + `MoverAuthority` (non-flying) |
| a spiral / weave / anything weird | `Custom` |
