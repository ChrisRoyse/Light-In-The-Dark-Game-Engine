# Ability Template Library

> Ready-to-use, drag-and-drop ability specs covering the common motion archetypes. Each
> template is a complete, valid `AbilitySpec` (illustrated as TOML) plus notes on what to
> edit. Copy one into your map's `abilities/` folder, change the documented parameters, run
> `tools/abilitycheck`, and cast. Implements R-ABL-3.

These templates are deliberately spread across the [mover catalog](../../05-movers/mover-types.md)
so every motion kind has a worked starting point.

| Template | Mover kind | Archetype | File |
|----------|-----------|-----------|------|
| **Homing Bolt** | `Homing` | single-target seeker | [homing-bolt.md](homing-bolt.md) |
| **Orbital Guardian** | `OrbitUnit` (loop) | persistent orbiting damage | [orbital-strike.md](orbital-strike.md) |
| **Nova Ring** | many `Linear`/`Point` | radial AoE burst | [nova-ring.md](nova-ring.md) |
| **Chain Bounce** | `Homing` + group | bouncing chain with decay | [chain-bounce.md](chain-bounce.md) |
| **Boomerang** | `Spline` | out-and-back, double hit | [boomerang.md](boomerang.md) |
| **Persistent Field** | `OrbitPoint`/timer | ground AoE over time (DoT zone) | [persistent-field.md](persistent-field.md) |

## How templates are structured

Each template doc has four sections:
1. **What it does** — one-paragraph behavior.
2. **Primitives used** — which of the five it composes, so you know what it costs.
3. **The spec** — the full TOML, ready to copy.
4. **Edit points** — the parameters you'll most likely change, and any structural variations.

## Composition coverage

Together these six templates exercise every primitive:
- **Timers** — cooldowns everywhere; Nova staggering; Persistent Field DoT cadence.
- **Unit groups** — Chain Bounce target tracking; Nova/Field AoE selection.
- **KV store** — Chain Bounce remembers already-hit units; Field stores its tick budget.
- **Custom events** — every template emits `ability.impact` for map hooks.
- **Movers** — one per mover kind (Homing, OrbitUnit, Linear/Point, Spline, OrbitPoint).

An author who understands these six can build essentially any standard ability by
recombination.
