# 05 — Unified Parametric Movers

> **Requirement namespace:** `R-MOV-*`
> **Supersedes:** the straight-line-only `MissileStore` motion (`litd/sim/missile.go`,
> `store_missile.go`) — all projectile motion becomes mover-driven; unit motion gains a
> mover-authority path.
> **Primary tick phase:** phase 4 (movement)
> **New sub-hash:** `"movers"` (the missile sub-hash folds in / retires)
> **New caps:** `Caps.Movers` (default 4,096), shared by units and projectiles

---

## Why this is the keystone

The user's directive: *abilities should be able to spawn a projectile, move it, and have
it collide with objects — and creators should be able to build custom abilities easily.*
That capability lives or dies on the motion system. Today motion is two disjoint,
under-powered halves:

- **Units** move only by pathing toward waypoints (`movement.go`).
- **Missiles** move only in a straight line / toward a point / homing, with optional
  acceleration; `Arc` is render-only; there is **no orbital, spline, or parametric
  motion** (`store_missile.go`).

So an orbiting guardian, a boomerang, a spiral nova, a charge-and-knockback, a curving
homing missile, or a "circle the caster then fire outward" pattern are all impossible
without bespoke per-map Lua that re-integrates positions by hand — nondeterministic and
unserializable.

PRD2 replaces both halves with **one motion primitive**: the **mover**. A mover is a
serializable, fixed-point, parametric motion controller that owns a transform (a unit's or
a missile's) and advances it deterministically each tick according to its kind. Movers
carry collision policy and effect payloads, so "spawn projectile → move → collide → run
effect" is a single composed object, not glue code.

## The mover in one sentence

> A **mover** binds a *target transform* (unit or missile) to a *parametric path*
> (linear / homing / orbit / arc / spline / custom), advances it in fixed-point each tick,
> optionally *collides* (hit mask + radius + pierce) running an *effect payload* on hit,
> and on *completion* expires / loops / detonates / invokes a continuation.

## Mover kinds (catalog in [mover-types.md](mover-types.md))

| Kind | Motion | Canonical use |
|------|--------|---------------|
| `Linear` | straight line along a direction for a range | skillshots, bullets |
| `Homing` | track a target entity, turn-rate limited | seeking missiles |
| `Point` | fly to a fixed point, then complete | placed projectiles, dashes-to-point |
| `Orbit` (unit/point anchor) | circle an anchor at radius with angular velocity | orbiting guardian, electric ball |
| `Arc` / `Ballistic` | parabolic flight to a point (gameplay, not just render) | lobbed grenades, mortar |
| `Spline` | Catmull-Rom through waypoints | boomerang, patrol curves, scripted paths |
| `Custom` | per-tick step via a continuation | anything parametric an author can express |

## What movers unlock

- **Composable projectile abilities** ([06](../06-ability-composition/)): cast → spawn
  projectile → attach mover → on-collision effect → on-complete cleanup, all as data.
- **Unit motion effects**: knockback, pull, dash, charge, leap, orbit — by giving a unit a
  mover with **movement authority** (suspends pathing for the duration, R-MOV-7).
- **Determinism + save/load** for all of the above, which per-map Lua could never provide.

## Documents

- [spec.md](spec.md) — data model, advance loop, authority, completion, supersession plan.
- [mover-types.md](mover-types.md) — each kind's parameters and exact fixed-point math.
- [collision-and-impact.md](collision-and-impact.md) — hit masks, swept tests, pierce,
  effect payloads, event emission.
- [api.md](api.md) — Go API and Lua binding.
- [test-plan.md](test-plan.md) — determinism, collision correctness, zero-alloc, FSV.
