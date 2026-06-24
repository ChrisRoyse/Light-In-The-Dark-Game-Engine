# Template — Boomerang

## What it does
Cast in a direction. A projectile flies out along a curved path and returns to the caster,
able to hit enemies on **both** the outgoing and returning legs (pierce). Demonstrates the
`Spline` mover (out-and-back) and per-pass hit handling.

## Primitives used
- [05 Movers](../../05-movers/) — `Spline` (3 control points: caster → far point → caster)
- effect arena — `boomerang_hit`
- [02 Unit Groups](../../02-unit-groups/) — implicit via pierce hit ordering
- [04 Custom Events](../../04-custom-events/) — `ability.impact`
- [01 Timers](../../01-timer-wheel/) — cooldown

## The spec
```toml
[ability]
id         = "boomerang"
name       = "Boomerang"
cast_type  = "active"
indicator  = "line"
mana_cost  = 60
cooldown   = 6.0

[ability.timing]
precast    = 0.2
backswing  = 0.3

[[ability.on_cast]]
op         = "spawn_projectile"
projectile = "blade"
at         = "caster"

[[ability.on_cast]]
op       = "attach_mover"
mover    = "spline"
points   = [ "caster", "offset(cast_direction, 700)", "caster" ]  # out-and-back
speed    = 28
closed   = false
fx = { hit = "enemy", radius = 60, pierce = 99, done = "expire",
       re_arm = 0.4, effects = "boomerang_hit" }

[ability.effects.boomerang_hit]
ops = [
  { effect = "damage", amount = 110, type = "physical" },
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

## Edit points
| Change | Field |
|--------|-------|
| Throw distance | the `offset(cast_direction, 700)` control point |
| Travel speed | `attach_mover.speed` |
| Curve shape | add more control points (e.g. a lateral point for a hook) |
| Double-hit window | `fx.re_arm` (how soon a unit can be hit again on the return leg) |
| Damage / type | `effects.boomerang_hit.ops[0]` |
| Return-to-a-moving-caster | sample the caster's live position by setting the last control point to `"caster_live"` |

## Notes
- The last control point `"caster"` is sampled at launch; for a boomerang that returns to
  where the caster *is now* (if they moved), use `"caster_live"`, which the spline mover
  re-reads each tick for its tail control point.
- `pierce = 99` + `re_arm` lets the blade cut multiple enemies on each pass without
  consuming itself; `done = "expire"` ends it when the spline completes (back at the caster).
