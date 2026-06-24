# Template — Chain Bounce

## What it does
Cast on an enemy. A projectile homes to the target, hits it, then bounces to the nearest
unhit enemy within range, repeating up to `max_bounces` times with damage decaying per
bounce. The classic chain-lightning pattern. Demonstrates groups + KV (already-hit memory) +
homing movers chained via completion continuations.

## Primitives used
- [05 Movers](../../05-movers/) — `Homing`, completion continuation re-targets the next bounce
- [02 Unit Groups](../../02-unit-groups/) — candidate pool per bounce
- [03 KV Store](../../03-keyvalue-store/) — remembers already-hit units + bounce counter + carried damage
- [04 Custom Events](../../04-custom-events/) — `ability.impact` per bounce
- [01 Timers](../../01-timer-wheel/) — cooldown

## The spec
```toml
[ability]
id         = "chain_bounce"
name       = "Chain Bounce"
cast_type  = "active"
cast_range = 700
mana_cost  = 70
cooldown   = 7.0

[ability.params]               # per-instance, stored as KV on the projectile
max_bounces   = 5
bounce_range  = 400
base_damage   = 160
decay         = 0.80           # 20% less per bounce

[ability.timing]
precast = 0.2

[[ability.on_cast]]
op         = "spawn_projectile"
projectile = "lightning_orb"
at         = "caster"

[[ability.on_cast]]
op    = "set_kv"               # initialize bounce state on the projectile
target = "projectile"
pairs = { bounces_left = "max_bounces", damage = "base_damage" }

[[ability.on_cast]]
op         = "attach_mover"
mover      = "homing"
anchor     = "target_unit"
speed      = 30
fx = { hit = "enemy", radius = 40, pierce = 1, done = "cont",
       on_done = "chain_next", effects = "chain_hit" }

# chain_next is a REGISTERED continuation (Go or persisted-Lua), referenced by name.
# Pseudocode of what it does (it is data-driven, not inline closure):
#   hit the current target (effects already ran on collision)
#   bounces_left -= 1; if 0 -> expire
#   fill a group of enemies within bounce_range, excluding already-hit (KV set)
#   pick nearest; record it in the already-hit KV set
#   damage *= decay
#   re-attach a homing mover to the new target with on_done = chain_next

[ability.effects.chain_hit]
ops = [
  { effect = "damage", amount = "kv:damage", type = "magic" },   # reads carried damage
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

## Edit points
| Change | Field |
|--------|-------|
| Number of bounces | `params.max_bounces` |
| Bounce search radius | `params.bounce_range` |
| Starting damage / falloff | `params.base_damage`, `params.decay` |
| Bounce speed | `attach_mover.speed` |
| What each hit does | `effects.chain_hit.ops` |

## Notes
- The **already-hit set** lives in the projectile's KV ([03](../../03-keyvalue-store/)) so it
  serializes; a saved mid-chain bolt resumes correctly.
- `chain_next` is a *registered* continuation, not an inline closure, so the whole chain is
  deterministic and save-safe. Authors edit numbers; the bounce logic is a shipped,
  validated continuation reused across chain abilities (frost chain, heal chain, etc.).
