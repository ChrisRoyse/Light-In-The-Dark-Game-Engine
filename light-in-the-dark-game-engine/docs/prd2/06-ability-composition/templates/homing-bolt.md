# Template — Homing Bolt

## What it does
Cast on an enemy unit. Spawns a projectile that seeks the target (optionally curving),
deals magic damage on contact, and emits `ability.impact`. The simplest "spawn projectile →
move → collide → effect" ability — the canonical starting point.

## Primitives used
- [05 Movers](../../05-movers/) — `Homing`
- effect arena — `bolt_hit`
- [04 Custom Events](../../04-custom-events/) — `ability.impact`
- [01 Timers](../../01-timer-wheel/) — cooldown

## The spec
```toml
[ability]
id         = "homing_bolt"
name       = "Homing Bolt"
cast_type  = "active"
indicator  = "none"          # single-target; uses unit targeting
cast_range = 800
mana_cost  = 50
cooldown   = 4.0

[ability.timing]
precast    = 0.25
backswing  = 0.3

[[ability.on_cast]]
op         = "spawn_projectile"
projectile = "arcane_orb"
at         = "caster"

[[ability.on_cast]]
op         = "attach_mover"
mover      = "homing"
anchor     = "target_unit"   # seek the ordered target
speed      = 22
turn_rate  = 0               # 0 = instant track; >0 (deg/s) = curving
fx = { hit = "enemy", radius = 48, pierce = 1, done = "expire", effects = "bolt_hit" }

[ability.effects.bolt_hit]
ops = [
  { effect = "damage", amount = 180, type = "magic" },
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

## Edit points
| Change | Field |
|--------|-------|
| Damage | `effects.bolt_hit.ops[0].amount` / `type` |
| Speed / homing curve | `attach_mover.speed`, `turn_rate` (try `180` for a visible arc) |
| Projectile look | `spawn_projectile.projectile` |
| Add an on-hit effect (slow, stun, burn) | append to `effects.bolt_hit.ops` (`buff`, `area`, …) |
| Make it pierce | `fx.pierce = 3`, `fx.done = "expire"` and switch `anchor` to a direction (see Nova/Linear) |
| Cooldown / mana | `cooldown`, `mana_cost` |
