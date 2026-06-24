# Template — Orbital Guardian

## What it does
Cast (or passive/toggle). Spawns one or more projectiles that orbit the caster
indefinitely, damaging enemies they pass through every `re_arm` seconds. The tutorial
"electric ball" pattern. Demonstrates `loop` completion and the already-hit ring.

## Primitives used
- [05 Movers](../../05-movers/) — `OrbitUnit` (`done = "loop"`), already-hit ring + re-arm
- effect arena — `orbit_hit`
- [01 Timers](../../01-timer-wheel/) — optional duration cap; cooldown
- [04 Custom Events](../../04-custom-events/) — `ability.impact`

## The spec
```toml
[ability]
id         = "orbital_guardian"
name       = "Orbital Guardian"
cast_type  = "active"
indicator  = "none"
mana_cost  = 80
cooldown   = 12.0

[ability.timing]
precast    = 0.2

# Spawn a RING of satellites by repeating spawn+orbit at staggered angles.
[[ability.on_cast]]
op         = "loop_count"        # author-side static repeat over N satellites
count      = 3
[ability.on_cast.body]
  [[ability.on_cast.body.ops]]
  op         = "spawn_projectile"
  projectile = "electric_ball"
  at         = "caster"
  [[ability.on_cast.body.ops]]
  op         = "attach_mover"
  mover      = "orbit_unit"
  anchor     = "caster"
  radius     = 200
  ang_vel    = 180              # deg/s
  start_angle= "index * 120"    # evenly spaced: 0,120,240
  height     = 64
  fx = { hit = "enemy", radius = 50, pierce = 9999, done = "loop",
         re_arm = 0.5, effects = "orbit_hit" }

# Optional: end the orbit after 8s (remove for a permanent aura).
[[ability.on_cast]]
op    = "after"
delay = 8.0
[ability.on_cast.body]
  ops = [ { op = "cancel_movers", tag = "orbital_guardian" } ]

[ability.effects.orbit_hit]
ops = [
  { effect = "damage", amount = 40, type = "magic" },
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

## Edit points
| Change | Field |
|--------|-------|
| Number of satellites | `loop_count.count` (and the `start_angle` divisor) |
| Orbit size / speed | `radius`, `ang_vel` |
| Tick cadence | `fx.re_arm` (seconds between hits on the same unit) |
| Per-tick damage | `effects.orbit_hit.ops[0].amount` |
| Permanent vs timed | remove the `after`/`cancel_movers` block for permanent |
| Orbit a POINT instead of the caster | `mover = "orbit_point"`, `center = "target_point"` |

## Notes
`pierce = 9999` with `done = "loop"` means the orbit never self-terminates on hits; the
already-hit ring + `re_arm` throttles repeat damage so a unit standing in the orbit takes
damage every 0.5 s, not every tick.
