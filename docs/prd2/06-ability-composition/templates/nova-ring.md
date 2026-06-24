# Template — Nova Ring

## What it does
Cast at the caster (or a point). Emits N projectiles outward in evenly spaced directions, a
radial burst. Each bolt is a short linear skillshot that damages the first enemy it hits (or
pierces). The tutorial "seed ring" / radial-nova pattern. Demonstrates static fan-out and
linear movers.

## Primitives used
- [05 Movers](../../05-movers/) — N × `Linear`
- effect arena — `nova_hit`
- [02 Unit Groups](../../02-unit-groups/) — optional, if you want a single AoE pulse instead of bolts
- [04 Custom Events](../../04-custom-events/) — `ability.impact`
- [01 Timers](../../01-timer-wheel/) — cooldown

## The spec (bolt nova)
```toml
[ability]
id         = "nova_ring"
name       = "Nova Ring"
cast_type  = "active"
indicator  = "circle"
mana_cost  = 90
cooldown   = 8.0

[ability.timing]
precast    = 0.3
backswing  = 0.4

[[ability.on_cast]]
op    = "loop_count"
count = 12                       # 12 bolts → every 30 degrees
[ability.on_cast.body]
  [[ability.on_cast.body.ops]]
  op         = "spawn_projectile"
  projectile = "seed"
  at         = "caster"
  [[ability.on_cast.body.ops]]
  op         = "attach_mover"
  mover      = "linear"
  dir        = "angle(index * 30)"   # 0,30,60,... degrees
  speed      = 26
  range      = 500
  fx = { hit = "enemy", radius = 40, pierce = 1, done = "detonate",
         effects = "nova_hit" }

[ability.effects.nova_hit]
ops = [
  { effect = "damage", amount = 90, type = "magic" },
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

## Variation — single AoE pulse (group fill instead of bolts)
If you want an instant ring of damage with no projectiles:
```toml
[[ability.on_cast]]
op     = "fill_group"
into   = "nova_targets"
shape  = "radius"
center = "caster"
radius = 500
filter = { enemy_of = "caster", alive_only = true }

[[ability.on_cast]]
op    = "for_each_in_group"
group = "nova_targets"
[ability.on_cast.body]
  ops = [ { effect = "damage", amount = 120, type = "magic" } ]
```

## Edit points
| Change | Field |
|--------|-------|
| Number of bolts / spacing | `loop_count.count` (+ `index * (360/count)`) |
| Reach / speed | `attach_mover.range`, `speed` |
| Pierce vs single hit | `fx.pierce` |
| Damage | `effects.nova_hit.ops[0].amount` |
| Bolt nova vs AoE pulse | use the variation block |
| Stagger the bolts over time | wrap the body in a `times` timer op (frame-spaced emission) |
