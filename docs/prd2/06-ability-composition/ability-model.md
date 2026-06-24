# The Composable Ability Model

> An ability is a **declarative spec** the engine interprets by instantiating PRD2
> primitives. This document defines the spec schema, the execution lifecycle, and the
> guarantees. Implements R-ABL-1..6.

---

## 1. Spec schema

An ability spec is a serializable data record (authored as TOML/Lua table; compiled into a
sim-side `AbilitySpec`). It never contains executable engine code — only references to
registered continuations/effect-lists by id.

```toml
# abilities/fireball.toml  (illustrative)
[ability]
id          = "fireball"
name        = "Fireball"
cast_type   = "active"           # active | passive | normal_attack | build | gather
indicator   = "line"             # none | circle | line | arrow  (presentation hint)
cast_range  = 900
mana_cost   = 75
cooldown    = 6.0                # seconds → timer ticks

# Cast phases map to the existing ability cast state machine
# (Ready→Precast→CastPoint→Channel→Backswing→Cooldown).
[ability.timing]
precast     = 0.3                # anticipation (play cast animation)
cast_point  = 0.0                # the instant the effect triggers
backswing   = 0.4

# WHAT HAPPENS AT CAST — a list of composed actions over primitives 1-5.
[[ability.on_cast]]
op          = "spawn_projectile"
projectile  = "fireball_proj"    # visual/data id
at          = "caster"           # caster | target_point | target_unit | offset(...)

[[ability.on_cast]]
op          = "attach_mover"
mover       = "linear"           # any kind from 05-movers
dir         = "cast_direction"
speed       = 30
range       = 900
fx          = { hit = "enemy,ground", radius = 64, pierce = 1, done = "detonate",
                effects = "fireball_impact" }

# WHAT HAPPENS ON COLLISION / COMPLETION — referenced effect lists & events.
[ability.effects.fireball_impact]   # a compiled effect list (shared arena)
ops = [
  { effect = "damage", amount = 250, type = "magic", area = 200 },
  { effect = "emit_event", name = "ability.impact", arg = "caster" },
]
```

### Schema sections
- **Identity & gating:** `id`, `name`, `cast_type`, `indicator`, `cast_range`,
  `mana_cost`, `cooldown` — drive the existing ability/order machinery and HUD.
- **Timing:** maps to the existing cast state machine
  (`ability.go:27 castStateNames`), so animations, channels, and interrupts work
  unchanged.
- **`on_cast`:** an ordered list of **composition ops** (§2) executed at `cast_point`.
- **`effects.*`:** named compiled effect lists (the existing `data.EffectList` arena),
  referenced by movers and ops.

## 2. Composition ops (the vocabulary)

Every op is a thin wrapper over a primitive. The complete op set:

| Op | Primitive | Effect |
|----|-----------|--------|
| `spawn_projectile` | projectile entity | create a lightweight entity to carry a mover |
| `attach_mover` | [05](../05-movers/) | attach a mover (any kind) to a projectile or the caster/target unit |
| `fill_group` | [02](../02-unit-groups/) | populate a group by radius/rect/owner/type for AoE/chain targeting |
| `run_effects` | effect arena | run a named effect list at a context (damage/heal/buff/area/chain) |
| `emit_event` | [04](../04-custom-events/) | ring a custom-event bell (`ability.impact`, etc.) |
| `set_kv` / `get_kv` | [03](../03-keyvalue-store/) | store/read per-instance parameters & state |
| `after` / `loop` / `times` | [01](../01-timer-wheel/) | schedule delayed/periodic follow-ups (telegraphs, channels, DoTs) |
| `for_each_in_group` | [02](../02-unit-groups/) | iterate a group, applying nested ops |
| `if` | (control) | branch on a KV/predicate (deterministic) |

Ops compose recursively: a `for_each_in_group` may contain `spawn_projectile` +
`attach_mover` (a multi-bolt nova); an `after` may contain `run_effects` (a delayed
detonation).

> **No new sim state (R-ABL-6).** Executing an ability spec *only* allocates the
> primitives it names (a projectile entity, a mover, a timer, a group, KV pairs). The spec
> table itself is read-only data. Determinism, serialization, and zero-alloc all follow from
> the primitives.

## 3. Execution lifecycle

```
order: cast ability A on target/point
  └─ existing ability state machine runs: Ready → Precast(precast s) →
        CastPoint  ──► execute on_cast ops in order:
                          spawn projectile → attach mover → fill group → ...
        Channel (if any; periodic ops via timers)
        Backswing → Cooldown (cooldown timer)

per tick (phase 4): attached movers advance & collide → run effect lists, emit events
on mover completion: DoneMode (expire/detonate/cont) → optional cleanup timer/cont
```

- **Interrupts** cancel the ability's scheduled timers and active movers (the channel
  example in [01/api](../01-timer-wheel/api.md)).
- **Mana/cooldown** are a `mana_cost` check at `CastPoint` and a `cooldown` timer; both are
  existing mechanisms plus the new timer wheel.

## 4. Characters/units reference abilities by id (R-ABL-4)

Granting an ability is data:

```toml
[unit.pyromancer]
abilities = ["fireball", "flame_nova", "blink"]
```

No engine change is needed to give a unit a new ability — only the ability spec file and a
reference. The same applies to items (item-granted abilities) and modifiers.

## 5. Determinism & validation

- An ability spec is **validated at load** (fail-closed): unknown ops, unregistered effect
  lists/continuations/mover kinds, or out-of-range numbers reject the spec deterministically
  with a logged reason (it does not silently half-load).
- All numeric params convert to fixed-point at load; no float at runtime.
- The spec contributes to the data fingerprint (`#208`-style content hash) so two peers with
  different ability files cannot desync silently — they fail the fingerprint check at
  join/load (the existing mechanism).

## 6. Why this is robust *and* easy

- **Robust:** every ability is deterministic, serializable, hashable, and zero-alloc by
  construction, because it is nothing but a composition of primitives that already are.
- **Easy:** authoring is editing a data file. The op vocabulary is small (9 ops), the
  primitives are documented, and the [template library](templates/README.md) gives a working
  starting point for each archetype. An AI agent's job is "edit these numbers / swap this
  mover / change this effect list," not "write a deterministic engine subsystem."
