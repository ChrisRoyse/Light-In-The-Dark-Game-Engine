# Template — Persistent Field

## What it does
Cast at a point. Creates a ground area that damages (or heals/buffs) units inside it every
`tick_interval` seconds for `duration` seconds — a damage-over-time zone (blizzard, poison
cloud, healing ward). Demonstrates timers (loop/count), group fill per pulse, and an
optional cosmetic `OrbitPoint` mover for swirling visuals.

## Primitives used
- [01 Timers](../../01-timer-wheel/) — `count` timer drives the pulses; cooldown
- [02 Unit Groups](../../02-unit-groups/) — fill per pulse for the AoE
- [03 KV Store](../../03-keyvalue-store/) — stores remaining pulses / per-field params
- [04 Custom Events](../../04-custom-events/) — `ability.impact` per pulse
- [05 Movers](../../05-movers/) — optional `OrbitPoint` cosmetic projectiles

## The spec
```toml
[ability]
id         = "persistent_field"
name       = "Blizzard"
cast_type  = "active"
indicator  = "circle"
cast_range = 1000
mana_cost  = 100
cooldown   = 14.0

[ability.params]
radius        = 300
tick_interval = 0.5
pulses        = 12          # 0.5s * 12 = 6s duration
damage        = 60

[ability.timing]
precast = 0.3

# Optional cosmetic: a few orbiting motes mark the zone.
[[ability.on_cast]]
op    = "loop_count"
count = 4
[ability.on_cast.body]
  [[ability.on_cast.body.ops]]
  op         = "spawn_projectile"
  projectile = "frost_mote"
  at         = "target_point"
  [[ability.on_cast.body.ops]]
  op       = "attach_mover"
  mover    = "orbit_point"
  center   = "target_point"
  radius   = 280
  ang_vel  = 90
  start_angle = "index * 90"
  fx = { pierce = 0, done = "loop" }     # cosmetic only (no hit)

# The gameplay: pulse damage every tick_interval, 'pulses' times.
[[ability.on_cast]]
op       = "times"
interval = "tick_interval"
count    = "pulses"
[ability.on_cast.body]
  [[ability.on_cast.body.ops]]
  op     = "fill_group"
  into   = "field_targets"
  shape  = "radius"
  center = "target_point"
  radius = "radius"
  filter = { enemy_of = "caster", alive_only = true }
  [[ability.on_cast.body.ops]]
  op    = "for_each_in_group"
  group = "field_targets"
  [ability.on_cast.body.ops.body]
    ops = [
      { effect = "damage", amount = "damage", type = "magic" },
      { effect = "emit_event", name = "ability.impact", arg = "caster" },
    ]

# When the 'times' timer completes, cancel the cosmetic motes.
[[ability.on_cast]]
op    = "after"
delay = "tick_interval * pulses"
[ability.on_cast.body]
  ops = [ { op = "cancel_movers", tag = "persistent_field" } ]
```

## Edit points
| Change | Field |
|--------|-------|
| Zone size | `params.radius` |
| Duration | `params.pulses` × `params.tick_interval` |
| Pulse cadence | `params.tick_interval` |
| Per-pulse effect | the `for_each_in_group` body (`damage` → `heal`/`buff`/`slow`) |
| Friendly zone (healing ward) | `filter.ally_of`, swap `damage` for `heal` |
| Remove visuals | delete the `loop_count` cosmetic block |

## Notes
- The DoT is driven by a single `times` (count) timer — deterministic and serializable; a
  save mid-field resumes with the correct number of remaining pulses (stored via the timer's
  `Remaining` and/or KV).
- The cosmetic orbit motes have `pierce = 0` so they never deal damage — they are pure
  presentation but still go through the mover system (so they too are deterministic and
  save-safe).
