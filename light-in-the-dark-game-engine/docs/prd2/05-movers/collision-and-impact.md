# Movers — Collision & Impact

> This is what makes a mover an *ability ingredient* rather than just motion: a mover can
> detect what it touches and run effects on contact. Implements R-MOV-4. The math reuses
> the existing missile collision code (`litd/sim/missile.go:333-417`); PRD2 generalizes it
> across all mover kinds.

---

## 1. Collision policy fields

```
HitMask  uint16   // what this mover can hit (see §2)
Radius   fixed.F64 // collision radius around the mover's point/sweep
Pierce   int32     // remaining hits before the mover completes (>=1; 1 = single hit)
Decay    uint16    // per-mille payload multiplier applied after each hit (e.g. 850 = -15%/hit)
Payload  data.EffectList // compiled effect list run at each hit (shared with abilities/triggers)
Packet   DamagePacket    // pre-rolled damage when Payload is empty
```

A mover with `Pierce == 0` (or no `HitMask`) is **non-colliding** — pure motion (an orbit
that only looks pretty, a camera-path projectile). Setting any hit policy turns it into a
weapon.

## 2. Hit mask (reuses `MissileHit*`)

```go
// from store_missile.go:34-44, reused verbatim
MissileHitGround    = data.TargetGround
MissileHitAir       = data.TargetAir
MissileHitStructure = data.TargetStructure
MissileHitEnemy     = 1 << 8   // relative to the mover's owner team
MissileHitAlly      = 1 << 9
```

A candidate is hit only if its target class bits intersect the mask **and** the
enemy/ally relation to the mover's owner is allowed (`missileHitAllowed`,
`missile.go:417`). Friendly fire, ally-only buffs, and ground-vs-air filtering all fall out
of the mask.

## 3. Collision test by mover kind

- **Linear / Arc (moving point with a sweep):** swept-segment test from `pos` to `next`.
  Project each nearby candidate into `(along, perp)` space relative to the sweep direction;
  hit if `along ∈ [0, stepLen)` and `perp² ≤ (Radius)²` (exact, in the integer projection
  space `dirIntScale=1024` that avoids F64 overflow — `missile.go:333-376`). This catches
  fast movers that would tunnel a point-in-radius test.
- **Point / Homing (arrival):** on snap-arrival, a radius query at the arrival point
  (`DistSq ≤ RadiusSq`).
- **Orbit:** a radius query at the mover's current orbit position each tick. (An orbiting
  blade that damages whatever it passes through.)
- **Spline:** swept test along the per-tick spline chord (treat the tick's movement as a
  short segment), same projection math as linear.
- **Custom:** the continuation returns a delta; the engine applies the swept test over that
  delta automatically, so custom movers get collision for free.

Candidate gathering uses the existing **collision grid** broad-phase (cells overlapping the
sweep/radius), visited in deterministic cell order then by entity index — never map order.
This is the textbook O(n²)→O(n) broad-phase; uniform-grid spatial hashing comfortably handles
10K+ colliders at frame rate (DOTS profiling: collision system 0.7–4.5 ms → 0.04–0.25 ms after
adding it). Tuning rules we adopt (sources in
[../00-foundations/performance-budget.md §5.2](../00-foundations/performance-budget.md#52-collision-broad-phase--reuse-the-existing-uniform-grid-on-not-on)):

- **Cell size ≈ 2× the common projectile/collision radius** — too small bloats multi-cell
  insertion; too large degrades toward O(n²).
- **Oversized colliders** (giant AoE, boss bodies) live in a separate small "oversized" list,
  not the grid, so they don't span dozens of cells.
- **Prefer incremental cell updates** to full rebuilds when a mover's cell membership is stable
  across a tick (~40× cheaper when only a few percent change cells).

## 4. Hit handling & ordering

For a single tick's collision:

1. Gather candidates passing the broad-phase + mask + relation filter.
2. **Order them deterministically** by `(along distance, entity index)` for swept tests, or
   `(DistSq, entity index)` for radius tests — closest first, ties by index. This makes
   pierce order (which N units a piercing shot hits first) reproducible.
3. For each candidate, in order, until `Pierce` is exhausted:
   - Run `Payload` at `EffectCtx{Source: Owner, Target: candidate, Point: hitPos}` (or apply
     `Packet` if `Payload` empty). Effects reuse the compiled `data.EffectList` arena shared
     with abilities and triggers (`effect.go`), so damage/heal/buff/area/chain all work.
   - Multiply the carried payload by `Decay` per-mille (a chain that weakens per bounce).
   - Record the candidate in a per-mover **already-hit set** (a small bounded ring keyed by
     entity index) so a piercing/orbiting mover does not re-hit the same unit every tick.
   - Decrement `Pierce`. If `Pierce == 0`, mark the mover complete (Detonate/Expire per
     `DoneMode`).
4. Emit an impact event (built-in `EVENT_MISSILE_IMPACT` and/or a custom
   `"ability.impact"` from [04](../04-custom-events/)) carrying source/target/point.

## 5. The already-hit set (anti-re-hit)

Orbiting and piercing movers stay in contact with a unit across multiple ticks. To avoid
re-applying the payload every tick:

- Each mover carries a tiny bounded **recent-hit ring** (e.g. last 8 entity indices) with a
  re-arm interval (ticks after which a unit may be hit again — for an orbit that should tick
  damage every 0.5 s, set re-arm to 10 ticks).
- The ring is fixed-size, value-typed, serialized, and hashed with the mover. No allocation;
  overflow evicts oldest (deterministic).

## 6. Terrain & obstacle collision

- A **non-flying** mover (`MoverFlying` unset) respects ground pathing flags: a knockback or
  ground-hugging skillshot stops/expires at a cliff or unwalkable cell (queried from the
  pathing grid, `mapdata`). `DoneMode` decides stop-vs-detonate at the wall.
- A **flying** mover ignores ground collision (arcs over walls), colliding only with units
  per its mask.
- Destructibles with collision participate via their existing collision stamp
  (`store_destructable.go`), so a projectile can break a destructible door (the
  tutorial maze-exit pattern).

## 7. Composability summary

```
spawn projectile entity ──► attach mover (Linear/Homing/Orbit/…) ──► per tick:
     move ─► swept/radius collide (mask+radius) ─► run EffectList on hits (pierce/decay)
                                                 └─► emit impact event
     on complete (range/arrival/pierce) ─► Detonate(payload) | Expire | Cont(cleanup)
```

Every arrow above is data on the mover record or the ability spec — **no per-ability Go or
Lua glue is required** to get "spawn, move, collide, apply effect." That is the property
the [ability composition layer](../06-ability-composition/) builds on.
