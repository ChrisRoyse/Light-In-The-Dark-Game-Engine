# Acceptance & Full-State Verification

> The single gate every PRD2 subsystem passes before merge. Acceptance is **evidence-based**
> (`prompts/fsv.md`, master PRD §5.5): run the thing, then inspect the source of truth — a
> state hash, the save bytes, the event log, a screenshot, the state JSON. Exit codes alone
> never constitute acceptance.

---

## 1. The universal subsystem checklist

A PRD2 subsystem is **done** only when every box is checked. (This is the
[architecture-principles conformance checklist](../00-foundations/architecture-principles.md#conformance-checklist-copy-into-each-subsystem-spec)
plus the verification rows.)

- [ ] **Functionality** — every requirement ID in the subsystem's namespace has a passing
      unit test (the `test-plan.md` matrix).
- [ ] **Determinism** — a 10k-tick replay produces a byte-identical `HashState` across two
      runs and under `-race`; a cross-platform recorded-hash fixture matches the reference
      machine (R-SIM-1).
- [ ] **Divergence localization** — corrupting one of the subsystem's columns makes
      `FirstDivergence` name that subsystem's sub-hash (R-SIM-6).
- [ ] **Save/load** — a mid-state save round-trips to a byte-identical continuation hash; the
      **mirror invariant** (save bytes == hashed bytes) holds.
- [ ] **Zero-alloc** — `testing.AllocsPerRun` reports 0 allocs/op at steady state for the
      subsystem's hot path (R-GC-1); no column is `append`-grown post-`NewWorld` (R-GC-2).
- [ ] **Performance** — the subsystem ships a `go test -bench` micro-benchmark recording its
      per-op and per-tick cost, wired into the `benchharness` preflight step so regressions are
      caught; measured cost fits the per-tick envelope in
      [00-foundations/performance-budget.md](../00-foundations/performance-budget.md) (PRD2
      total < 2 ms of the 50 ms tick at default caps).
- [ ] **Exhaustion posture** — pool exhaustion returns an invalid handle + increments a hashed
      drop counter; never panics, never allocates.
- [ ] **Handle safety** — stale (generation-mismatched) handles resolve to a no-op.
- [ ] **Dual surface** — Go API and Lua binding exist with identical semantics; an FSV
      scenario exercises the Lua path (the AI-authoring surface).
- [ ] **FSV scenario** — at least one end-to-end scenario verified by reading the source of
      truth (state JSON + screenshot + event log), not return values.
- [ ] **Preflight wired** — determinism fixture in the FULL 10k step; a core subtest in
      `--fast`; zero-alloc gate added; heavy e2e guarded with `testing.Short()`.

## 2. Per-subsystem FSV scenarios (the "prove it works" demos)

| Subsystem | FSV demo | Source of truth |
|-----------|----------|-----------------|
| Timers | spawner respawns 10 s after clear; telegraph detonates after 3 s; both survive a mid-delay save | state JSON unit counts at exact ticks; event log timing; post-load timing |
| Groups | radius-fill AOE damages exactly the in-radius enemies; spawner emptiness drives respawn | per-enemy `EvUnitDamaged`; baseline unit count restoration |
| KV | spawner config drives spawns; quest `state` transitions drive UI; equipment limit drops the old weapon | KV dump vs spawn outcome; UI text snapshots; inventory JSON |
| Custom Events | boss state machine sleep→battle→transform→death; BT AI branches by unit type | transition event log in order; per-unit order log |
| Movers | orbiting ball ticks every 0.5 s; piercing fireball decays per hit; boomerang double-hits; knockback stops at a cliff | event windows; decay in damage values; transform path in state JSON; screenshots |
| Abilities | each of the six templates casts correctly | per-template state JSON + screenshot + impact event |

## 3. Migration gates (P3 movers)

The missile→mover supersession carries extra, blocking gates because it changes the
determinism fingerprint and save format:

- **Parity blocker:** every existing missile fixture replays through the mover path to a
  **byte-identical** final hash before the legacy missile code is removed (`T-MOV-MIG-1..3`).
- **Save upgrade:** old saves containing missile rows upgrade to mover rows and reach the
  same hash as a fresh mover-path run (`T-MOV-MIG-2`).
- **Render parity:** mover-driven projectile positions match pre-migration screenshots within
  tolerance (`T-MOV-MIG-4`).

Until all four pass, the old missile path stays in place behind the new one.

## 4. Data-fingerprint gate (P4 abilities)

Ability spec files are content-hashed into the data fingerprint (`#208`-style), so two peers
with mismatched ability files fail the join/load fingerprint check rather than desyncing
silently. `tools/abilitycheck` must pass in preflight, and a fingerprint-mismatch test
confirms the rejection path.

## 5. Documentation gate

A subsystem is not done until:
- Its `spec.md`, `api.md`, and `test-plan.md` reflect the shipped code (no drift).
- The relevant `docs/prd/` sections that PRD2 superseded are annotated as superseded with a
  link to the PRD2 doc (so the older PRD stays internally consistent).
- For movers specifically, the master PRD's simulation chapter and `docs/prd/04-simulation`
  motion text are updated to point at [05-movers](../05-movers/) as the single motion model.

## 6. Definition of "robust" (the bar restated)

PRD2 calls a subsystem robust when it is, simultaneously: **deterministic, serializable,
zero-alloc at steady state, fixed-capacity, handle-safe, dual-surfaced, and composable.**
A subsystem that is fast and feature-complete but fails determinism or serialization is
**not** accepted — those properties are what let abilities, characters, and whole maps be
built on top without each one re-litigating correctness.
