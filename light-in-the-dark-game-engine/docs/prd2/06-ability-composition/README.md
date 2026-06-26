# 06 — Ability & Character Composition

> **Requirement namespace:** `R-ABL-*`
> **Depends on:** all of [01](../01-timer-wheel/)–[05](../05-movers/)
> **Adds:** no new autonomous sim state beyond a declarative spec table + the primitives it
> instantiates (R-ABL-6)

---

## The thesis

> Abilities and characters should be **assembled from primitives**, not coded. An author —
> human or AI — builds a new ability by composing: *cast → spawn projectile → attach mover →
> on collision run effects / emit event → on complete clean up*. Each arrow is one of the
> five PRD2 primitives. The whole thing is **data**: serializable, hashable, diffable, and
> shippable as a drag-and-drop template.

This directly serves the user's directive:
- *"Abilities should be able to spawn a projectile and then have it move and collide with
  other objects using a mover"* → the ability model's core verb is exactly that
  ([ability-model.md](ability-model.md)).
- *"Provide some templates for custom-made abilities so they can be drag-and-dropped into a
  Map"* → the [template library](templates/README.md).
- *"All abilities/skills … need the solid architecture of movers, KV stores, timers, unit
  groups"* → an ability spec is literally a recipe over those four plus custom events.

## Why composition, not a hardcoded ability list

The existing `AbilityStore` and effect arena are powerful but author abilities are still
fundamentally *engine-authored*. To let creators (and AI agents) make abilities without
touching Go, the ability needs to be a **document** the engine interprets, whose only
vocabulary is the five primitives. Then:

- A new ability = a new spec file. No recompile, no engine change (R-ABL-4).
- An AI agent edits a template's numbers/strings and gets a working, deterministic ability
  (R-ABL-5).
- The ability is automatically serializable and hashable because every primitive it uses is
  (R-ABL-6).

## The composition pipeline

```
            ┌──────────── Ability Spec (data) ───────────┐
 cast input │ phases: [precast → cast → channel → end]   │
   ─────────►│ cost: mana/cooldown (timer)                │
            │ on cast:                                    │
            │   spawn projectile(s)   ← projectile entity │
            │   attach mover(s)       ← 05-movers         │
            │   target group fill     ← 02-unit-groups    │
            │ on mover collision:                         │
            │   run effect list       ← effect arena      │
            │   emit custom event     ← 04-custom-events  │
            │ on complete:                                │
            │   cleanup via timer/cont ← 01-timer-wheel   │
            │ per-instance params stored as KV ← 03-kv    │
            └─────────────────────────────────────────────┘
```

## Documents

- [ability-model.md](ability-model.md) — the declarative spec schema and how the engine
  executes it.
- [custom-ability-authoring.md](custom-ability-authoring.md) — how an author/AI builds and
  drops in a new ability; the editing workflow; validation.
- [templates/](templates/README.md) — the ready-to-use template library.
