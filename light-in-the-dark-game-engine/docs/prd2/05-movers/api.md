# Movers — Public API & Lua Binding

> Value-type geometry (`Vec2`, `float64` seconds/units at the boundary → fixed-point
> inside), no G3N types. Options structs for the many-parameter cases (R-API-4).

---

## 1. Go API (`litd/api`)

### 1.1 Handle

```go
type Mover struct { /* g *Game; id sim.MoverID */ }
func (m Mover) Valid() bool
func (m Mover) Cancel()
func (m Mover) SetSpeed(u float64)
func (m Mover) Speed() float64
func (m Mover) Pierce() int
```

### 1.2 Attaching movers (options structs)

```go
// Common collision/completion options shared by all kinds.
type MoverFX struct {
    HitMask   HitMask        // Ground|Air|Structure|Enemy|Ally
    Radius    float64
    Pierce    int            // 0 = non-colliding; 1 = single; n = pierce n
    Decay     float64        // payload multiplier per hit (1.0 = none)
    Effects   EffectList     // run on each hit (damage/heal/buff/area/chain)
    OnDone    Cont           // completion continuation (0 = expire)
    DoneMode  MoverDone      // Expire|Loop|Detonate|Cont
    Flying    bool           // ignore ground collision
    Authority bool           // (units only) own the unit transform, suspend pathing
    ReArm     float64        // seconds before the same unit may be hit again (orbit/pierce)
}

func (g *Game) MoveLinear(proj Unit, dir Vec2, speed, rng float64, fx MoverFX) Mover
func (g *Game) MoveHoming(proj Unit, target Unit, speed, turnRatePerSec float64, fx MoverFX) Mover
func (g *Game) MovePoint(proj Unit, goal Vec2, speed float64, fx MoverFX) Mover
func (g *Game) MoveOrbitUnit(proj Unit, anchor Unit, radius, angVelPerSec, startAngle, height float64, fx MoverFX) Mover
func (g *Game) MoveOrbitPoint(proj Unit, center Vec2, radius, angVelPerSec, startAngle, height float64, fx MoverFX) Mover
func (g *Game) MoveArc(proj Unit, goal Vec2, speed, apexHeight float64, fx MoverFX) Mover
func (g *Game) MoveSpline(proj Unit, points []Vec2, speed float64, closed bool, fx MoverFX) Mover
func (g *Game) MoveCustom(proj Unit, step Cont, p Payload, fx MoverFX) Mover
```

`proj` is any entity whose transform the mover should drive — usually a freshly spawned
**projectile unit** (`g.SpawnProjectile(...)`), but it may be a normal `Unit` when combined
with `Authority` to move a character (dash/knockback/orbit-the-caster-as-a-unit).

### 1.3 Spawning a projectile to carry a mover

```go
// A projectile is a lightweight entity (model + team + render hooks, minimal sim columns).
proj := g.SpawnProjectile(ProjectileSpec{
    Kind:  "fireball",       // visual/data id
    Pos:   caster.Pos(),
    Owner: caster.Player(),
})
g.MoveLinear(proj, caster.Facing().Dir(), 30 /*u/s*/, 900 /*range*/, MoverFX{
    HitMask: HitEnemy | HitGround, Radius: 64, Pierce: 1,
    Effects: g.Effects("fireball_hit"), DoneMode: Detonate,
})
```

## 2. Lua binding (`litd/luabind`)

The Lua surface is the primary authoring path. It mirrors the Go calls with table options.

```lua
-- LINEAR skillshot
local p = SpawnProjectile("fireball", caster.pos, caster.owner)
MoveLinear(p, FacingDir(caster), 30, 900, {
    hit = "enemy,ground", radius = 64, pierce = 1,
    effects = "fireball_hit", done = "detonate",
})

-- HOMING, curving
local p = SpawnProjectile("seeker", caster.pos, caster.owner)
MoveHoming(p, target, 22, 180 --[[deg/s turn]], {
    hit = "enemy", radius = 48, pierce = 1, effects = "seek_hit", done = "expire",
})

-- ORBIT the caster forever (the electric ball)
local ball = SpawnProjectile("electric_ball", caster.pos, caster.owner)
MoveOrbitUnit(ball, caster, 200 --[[radius]], 180 --[[deg/s]], 0 --[[start angle]], 64 --[[height]], {
    hit = "enemy", radius = 50, pierce = 999, done = "loop", reArm = 0.5,
})

-- a RING of 6 satellites
for i = 0, 5 do
    local s = SpawnProjectile("spark", caster.pos, caster.owner)
    MoveOrbitUnit(s, caster, 180, 120, i * 60, 48, { done = "loop" })
end

-- ARC grenade over a wall
local g = SpawnProjectile("grenade", caster.pos, caster.owner)
MoveArc(g, targetPoint, 18, 256 --[[apex]], {
    hit = "enemy,ground", radius = 200, flying = true,
    effects = "grenade_blast", done = "detonate",
})

-- BOOMERANG (out-and-back spline)
local b = SpawnProjectile("boomerang", caster.pos, caster.owner)
MoveSpline(b, { caster.pos, FarPoint(caster, 700), caster.pos }, 26, false, {
    hit = "enemy", radius = 60, pierce = 99, effects = "cut", done = "expire",
})

-- DASH a UNIT (movement authority)
MovePoint(hero, dashTarget, 60, { authority = true, flying = false, done = "expire" })

-- KNOCKBACK a unit hit by something
MoveLinear(victim, KnockDir(source, victim), 40, 250, { authority = true, done = "expire" })

-- CUSTOM weave
local p = SpawnProjectile("wisp", caster.pos, caster.owner)
MoveCustom(p, "sine_weave", { phase = 0, amp = 80 }, { hit="enemy", radius=40, effects="zap" })

CancelMover(b)
```

`hit = "enemy,ground"` parses to the bit mask; `done = "detonate"|"expire"|"loop"|"cont"`;
`effects = "<effect-list id>"` references a compiled effect list (same ids abilities use).

## 3. Mapping to WC3 / the tutorial corpus

The tutorial corpus enumerates exactly five mover types ("tracking, curved, linear, loop
unit-based, loop point-based"). PRD2 covers all five and adds arc/spline/custom:

| Tutorial mover | PRD2 |
|----------------|------|
| tracking mover | `MoveHoming` |
| curved mover | `MoveHoming` with `turnRate`, or `MoveSpline` |
| linear mover | `MoveLinear` |
| loop mover (unit based) | `MoveOrbitUnit`, `done="loop"` |
| loop mover (point based) | `MoveOrbitPoint`, `done="loop"` |
| — (not in tutorial) | `MoveArc` (ballistic), `MoveCustom` (parametric) |

## 4. Determinism notes for authors

- Angles/speeds given in degrees/seconds at the API boundary are converted to fixed-point
  per-tick once, at attach time. The conversion is deterministic.
- Never integrate a position yourself in Lua across ticks — attach a mover. A hand-rolled
  Lua integrator is nondeterministic and unserializable; a mover is neither.
- `MoveCustom` continuations must be registered (Go) or be the persisted Lua coroutine form;
  both are save-safe.
