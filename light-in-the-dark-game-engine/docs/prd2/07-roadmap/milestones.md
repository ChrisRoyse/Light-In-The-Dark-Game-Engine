# PRD2 Roadmap — Milestones & Dependency Order

> Build order follows the dependency lattice in
> [00-foundations/motivation.md §3](../00-foundations/motivation.md). Each milestone is
> shippable on its own and gated by its subsystem `test-plan.md`. The editor is explicitly
> **last** (per the directive: base-game systems first, editor absolutely last).

---

## Sequencing principle

Timers first (everything periodic/delayed needs them). Then groups, KV, and custom events
in parallel (mutually independent thin stores). Then movers (the big supersession, uses the
prior four for completion/payload/targeting). Then ability composition (pure assembly).
Editor integration trails the whole stack.

```
P1 Timers ─► P2 {Groups ∥ KV ∥ Custom Events} ─► P3 Movers ─► P4 Abilities ─► P5 Editor
```

---

## P1 — Serializable Timer Wheel  (closes #270)

**Deliverables**
- `litd/sim/timer.go` (`TimerStore`, wheel index), continuation registry integration.
- `AfterCont/LoopCont/CountCont` + owner-bound variants; `Game.After/Every` reclassified.
- Lua `After/Every/Times/CancelTimer/...` (save-safe via coroutine persister).
- `"timers"` sub-hash; save block; #270 closure→continuation migration.

**Exit gate:** [01/test-plan.md](../01-timer-wheel/test-plan.md) green; the
core single/loop/count determinism subtest in `preflight.sh --fast`; #270 marked closed.

**Why first:** highest leverage; unblocks spawners, cooldowns, channels, DoTs, and the
completion/teardown half of movers and abilities.

---

## P2 — Groups ∥ KV ∥ Custom Events  (parallelizable)

Three independent workstreams, each a thin SoA store + API + sub-hash. May proceed
concurrently once P1 lands.

**P2a — Unit Groups** (`litd/sim/unitgroup.go`)
- Group pool + shared membership arena; set algebra; query-fill; auto-prune.
- `"unitgroups"` sub-hash. Gate: [02/test-plan.md](../02-unit-groups/test-plan.md).

**P2b — KV Store** (`litd/sim/kv.go`)
- Tagged-union pairs; key/string interning; three scopes; sorted-array lookup.
- Retire `UserDataStore` with a back-compat shim + save upgrade.
- `"kv"` sub-hash. Gate: [03/test-plan.md](../03-keyvalue-store/test-plan.md).

**P2c — Custom Events** (`litd/sim/customevent.go`)
- Name→id registry; widen dispatch + `WaitEvent` to custom ids; KV-bag payloads.
- `"customevents"` sub-hash; registration-placement lint. Gate:
  [04/test-plan.md](../04-custom-events/test-plan.md).

**Exit gate:** all three test-plans green; each core subtest in `--fast`; each
determinism fixture in the FULL 10k step.

---

## P3 — Unified Parametric Movers  (the supersession)

**Deliverables**
- `litd/sim/mover.go` (`MoverStore`, all kinds), advance loop in phase 4.
- Collision/impact generalized from the missile swept code; already-hit ring; movement
  authority over units.
- **Missile migration:** refactor `MissileStore` flight into `Linear/Homing/Point` movers;
  retire `"missiles"` sub-hash; save-format bump + upgrade.
- `"movers"` sub-hash; spline waypoint arena; custom-step continuation registry.
- Go + Lua mover API.

**Exit gate:** [05/test-plan.md](../05-movers/test-plan.md) green, **including the
migration-parity suite** (`T-MOV-MIG-*`) as a release blocker before the old missile path is
deleted. Cross-platform fixed-point trig fixture passes.

**Risk note:** largest blast radius (touches render position sourcing, save format, the
determinism fingerprint). Land behind the parity suite; do not delete the missile path until
parity is byte-identical.

---

## P4 — Ability & Character Composition

**Deliverables**
- `AbilitySpec` schema + loader/validator (`litd/data` + `litd/sim`); op interpreter over
  primitives 1–5.
- `tools/abilitycheck` CLI; `abilitycheck` step in `preflight.sh`.
- The six shipped templates ([06/templates](../06-ability-composition/templates/)) as real,
  validated spec files under a sample world.
- Lua + Go authoring surface; unit/item ability references are data-only.

**Exit gate:** each template casts correctly under FSV (state JSON + screenshot); specs are
serializable/hashable; data fingerprint covers ability files; `abilitycheck` green in
preflight.

---

## P5 — Editor Integration  (LAST)

Only after P1–P4 are solid. The editor exposes the now-robust primitives:
- Timer/mover/group/KV/event inspectors in the debug/editor shell.
- Drag-and-drop ability template instantiation (copy a template into the map, edit
  parameters in a form, validate via `abilitycheck`).
- Visual mover preview (path trace) and ability test harness.

**Exit gate:** an author can drop a template, retune it, and play it without leaving the
editor; everything authored remains deterministic and save-safe.

> The editor adds **no new sim capability** — it is a surface over P1–P4. This ordering
> guarantees the base game is fully playable and scriptable before any editor work begins.

---

## Dependency summary

| Milestone | Depends on | Supersedes / closes |
|-----------|-----------|---------------------|
| P1 Timers | — | #270; Go-closure timer posture |
| P2a Groups | P1 (for auto-prune timing only) | transient `QueryEnumUnits`-only grouping |
| P2b KV | P1 (KV may hold TimerIDs) | `UserDataStore` |
| P2c Events | P1 (wait integration) | fixed `EventKind` registry |
| P3 Movers | P1–P2 | straight-line `MissileStore` motion |
| P4 Abilities | P1–P3 | engine-only ability authoring |
| P5 Editor | P1–P4 | — |
