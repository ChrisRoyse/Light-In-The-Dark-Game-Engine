# Custom Ability Authoring & Templates

> How a creator or AI agent builds a new ability and drops it into a map. The goal: a new
> ability in minutes by editing data, never engine code. Implements R-ABL-3, R-ABL-5.

---

## 1. The authoring loop

```
1. Pick the closest TEMPLATE (templates/)        ── e.g. "homing-bolt"
2. Copy it to your map's abilities/ folder       ── abilities/frost_bolt.toml
3. Edit the parameters                            ── speed, damage, effect, projectile model
4. Reference it from a unit/item                  ── unit.abilities += "frost_bolt"
5. Validate                                       ── `go run ./tools/abilitycheck abilities/`
6. Test in-engine                                 ── cast it; inspect state JSON + screenshot (FSV)
```

No build step. Ability specs are loaded as data; a map ships its `abilities/` folder inside
the world archive (`worldarchive`), alongside `assets/MANIFEST`.

## 2. Drag-and-drop semantics

A **template** is a self-contained ability spec plus the ids it references (projectile
visual id, effect-list id, mover kind, optional custom-event names). Dropping a template
into a map means:

- Copying its `.toml` (and any referenced effect-list/projectile-data fragments) into the
  map's data folders.
- The engine auto-registers the ability id at world load; the validator confirms every
  reference resolves.
- If the template uses a custom event or custom mover step, those are registered at world
  setup by the template's small companion stanza (declared, not coded).

Templates are designed so the **common edits are obvious and safe**: numbers (damage, speed,
range, radius, cooldown), enum swaps (mover kind, hit mask, effect type), and string swaps
(projectile model, effect-list id). Structural edits (adding an op) are possible but
optional.

## 3. Editing surface — what an author changes

| Want to change | Edit |
|----------------|------|
| How much it hurts | `effects.*.amount`, `type` |
| How far / fast it goes | `attach_mover.speed`, `range` |
| Straight vs homing vs orbit vs arc | `attach_mover.mover` (+ its params) |
| Single-target vs pierce vs AoE | `fx.pierce`, `fx.radius`, `effects.*.area`, `done` |
| What it looks like | `spawn_projectile.projectile`, indicator |
| Cooldown / mana | `cooldown`, `mana_cost` |
| Friendly-fire / target types | `fx.hit` mask |
| A follow-up after a delay | add an `after`/`times` op with a nested `run_effects` |

## 4. Validation (`tools/abilitycheck`)

A new CLI mirrors `assetcheck`/`apilint`:

- **Reference resolution:** every `projectile`, `mover`, `effects.*`, `emit_event` name,
  and `cont` resolves to a registered id.
- **Range/type checks:** numbers in range; enums valid; fixed-point convertible.
- **Determinism lint:** no float literals that lose precision; no nondeterministic op usage
  (e.g., custom-event registration must be in setup).
- **Zero-alloc budget:** the spec's worst-case primitive instantiation count fits the caps
  (e.g., a 100-bolt nova warns if it could exhaust `Caps.Movers`).
- Wired into `scripts/preflight.sh` as an `abilitycheck` step (analogous to `assetcheck`).

## 5. AI-agent authoring guidance

For an AI agent generating a new ability:

1. **Start from the nearest template** (the library is categorized by motion archetype).
2. **Only change documented parameters** unless adding a well-formed op.
3. **Never write a position integrator** — always use a mover kind. If the motion is exotic,
   use `MoveCustom` with a *registered* step, not an inline Lua closure.
4. **Use KV for per-instance state**, custom events for cross-system signals, timers for
   delays — never ad-hoc Lua tables/closures (nondeterministic, unserializable).
5. **Run `abilitycheck`**, then FSV: cast it and read the state JSON + screenshot.

A correct ability is almost always a *recombination* of: one projectile + one mover + one
effect list (+ optional group fill, timer follow-up, event emit). The agent's search space
is small and well-typed.

## 6. Example: authoring "Frost Bolt" from the homing template (5 edits)

```diff
  [ability]
- id          = "homing_bolt"
- name        = "Homing Bolt"
+ id          = "frost_bolt"
+ name        = "Frost Bolt"

  [[ability.on_cast]]
  op          = "spawn_projectile"
- projectile  = "arcane_orb"
+ projectile  = "frost_orb"

  [[ability.on_cast]]
  op          = "attach_mover"
  mover       = "homing"
- speed       = 22
+ speed       = 18
- fx = { hit="enemy", radius=48, pierce=1, done="expire", effects="bolt_hit" }
+ fx = { hit="enemy", radius=48, pierce=1, done="expire", effects="frost_hit" }

  [ability.effects.frost_hit]
  ops = [
-   { effect="damage", amount=180, type="magic" },
+   { effect="damage", amount=140, type="magic" },
+   { effect="buff",   buff="chilled", duration=3.0 },   # slow on hit
  ]
```

Five edits — a new name, a new model, a tuned speed, a new effect list, and an added slow
buff — produce a deterministic, serializable, save-safe new ability with zero engine
changes.
