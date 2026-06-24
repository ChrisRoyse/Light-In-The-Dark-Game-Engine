# Movers — Test & Verification Plan

> Movers are the largest subsystem and the one that supersedes existing missile motion, so
> the plan includes a **migration-parity** suite proving the new mover path reproduces the
> old missile behavior bit-for-bit before the old path is deleted.

---

## 1. Unit tests (per kind)

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-LIN | Linear: travels `range` then completes; sweep hits the first masked unit | R-MOV-2, R-MOV-4 |
| T-MOV-HOM | Homing: `turnRate=0` tracks instantly; `turnRate>0` curves (path matches a pinned fixture) | R-MOV-2 |
| T-MOV-PT | Point: snap-arrives without overshoot (exact `DistSq` compare) | R-MOV-3 |
| T-MOV-ORBU | OrbitUnit: stays at `radius` from a moving anchor; angle advances by `AngVel`/tick | R-MOV-2 |
| T-MOV-ORBP | OrbitPoint: circles a fixed center; `done=loop` runs indefinitely | R-MOV-2, R-MOV-5 |
| T-MOV-ARC | Arc: apex height correct at t=0.5; detonates at goal at t=1 | R-MOV-2 |
| T-MOV-SPL | Spline: passes through control points; boomerang returns to origin | R-MOV-2 |
| T-MOV-CUS | Custom: a registered sine-weave step produces the pinned path; gets collision for free | R-MOV-2, R-MOV-4 |

## 2. Collision & impact

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-COL-1 | Swept test catches a fast mover that a point-in-radius test would tunnel past | R-MOV-3 |
| T-MOV-COL-2 | Hit order is `(along, entity index)` deterministic; pierce hits closest-first | R-MOV-4 |
| T-MOV-COL-3 | Mask + enemy/ally relation filter correct (no friendly fire when ally bit unset) | R-MOV-4 |
| T-MOV-COL-4 | Pierce + decay: payload weakens per hit by `Decay` per-mille; completes at pierce 0 | R-MOV-4 |
| T-MOV-COL-5 | Already-hit ring + re-arm: an orbit ticks a stationary unit every `ReArm` ticks, not every tick | R-MOV-4 |
| T-MOV-COL-6 | Non-flying mover stops/detonates at a cliff; flying mover arcs over it | R-MOV-7 |
| T-MOV-COL-7 | Projectile breaks a destructible door (collision stamp) | R-MOV-4 |

## 3. Movement authority (units)

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-AUTH-1 | Authority mover suspends pathing for its unit; pathing resumes on completion | R-MOV-7 |
| T-MOV-AUTH-2 | A second authority mover on a unit cancels the first (last-wins, recorded) | R-MOV-7 |
| T-MOV-AUTH-3 | Knockback respects terrain; leap (flying) ignores it | R-MOV-7 |

## 4. Determinism / replay

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-DET-1 | 10k-tick run with mixed movers (orbits, splines, skillshots, customs); byte-identical `HashState` across runs and `-race` | R-SIM-1, R-MOV-3 |
| T-MOV-DET-2 | Advance order is slot-ascending; a fixture pins multi-mover same-tick resolution | R-MOV-9 |
| T-MOV-DET-3 | `FirstDivergence` localizes a corrupted mover column to `"movers"` | R-SIM-6 |
| T-MOV-DET-4 | Cross-platform fixed-point trig (orbit cos/sin) matches on the reference machine | R-MOV-3 |

## 5. Migration parity (supersede missiles, R-MOV-6)

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-MIG-1 | Every existing missile fixture replays through the mover path with a byte-identical final hash (Linear/Homing/Point) | R-MOV-6 |
| T-MOV-MIG-2 | Old saves containing missile rows upgrade into mover rows; post-upgrade hash equals a fresh mover-path run | R-MOV-6 |
| T-MOV-MIG-3 | The `"missiles"` sub-hash is removed and motion hashes once under `"movers"`; the full determinism fixture still passes | R-MOV-9 |
| T-MOV-MIG-4 | Render reads projectile transform driven by the mover; visual position matches the pre-migration screenshot within tolerance | FSV |

## 6. Save / load round-trip

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-SAV-1 | Save with live movers of every kind (incl. mid-spline, mid-orbit, mid-pierce, custom with CState); load; continue; hash tracks a no-save control | R-MOV-3 |
| T-MOV-SAV-2 | Spline waypoint arena + already-hit rings + free-list round-trip | R-MOV-9 |
| T-MOV-SAV-3 | Mirror invariant: save bytes == hashed bytes | R-SIM-6 |
| T-MOV-SAV-4 | Custom-step ContID re-binds on load; an unregistered step is dropped deterministically | R-MOV-2 |

## 7. Zero-alloc gate

| ID | Test | Asserts |
|----|------|---------|
| T-MOV-GC-1 | `AllocsPerRun` over attach→advance→collide→complete steady state = 0 allocs (all kinds) | R-GC-1 |
| T-MOV-GC-2 | Pool/arena exhaustion ⇒ invalid handle + counter, 0 allocs, no panic | R-MOV-8 |

## 8. Integration / FSV

| ID | Scenario | Source-of-truth check |
|----|----------|------------------------|
| F-MOV-1 | Orbiting electric ball damages enemies it passes, every 0.5s | event log: one `EvUnitDamaged` per enemy per 0.5s window; screenshot shows the ball circling |
| F-MOV-2 | Fireball skillshot pierces 3 enemies with decay, detonates at range | damage values decay per hit in the state JSON; detonation AoE at the end point |
| F-MOV-3 | Boomerang out-and-back hits on both legs | two impact windows per enemy in the event log |
| F-MOV-4 | Knockback moves a unit and stops at a cliff | unit transform path in state JSON ends at the cliff edge |
| F-MOV-5 | Save during an orbit + a mid-flight spline; load; both continue correctly | post-load positions track the no-save control screenshot-for-screenshot |

## 9. Preflight wiring

- Mover determinism fixture in the FULL 10k step; **core linear+orbit+collision subtest in
  `--fast`**.
- Migration-parity suite (`T-MOV-MIG-*`) gated as a **release blocker** before the old
  missile path is deleted.
- Zero-alloc gates added; heavy multi-mover e2e guarded with `testing.Short()`.
