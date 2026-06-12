# Public API — Shape and Type Inventory

> Expands [PRD §4.3 (API shape: handles → typed objects)](../../PRD.md#43-api-shape-handles--typed-objects).
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Siblings** | [Architecture](architecture.md) · [Deduplication policy](deduplication-policy.md) · [Execution model](execution-model.md) · [Naming & style](naming-and-style.md) |

---

## 1. From 60+ handle subtypes to ~20 nouns

`core/common.d.ts` declares the JASS handle hierarchy: 60+ subtypes of `handle`
(`unit`, `trigger`, `boolexpr`, `location`, `multiboard`, `camerasetup`, …). Most of these are not
*nouns* at all — they are workarounds for a language with no closures, no value types, no enums,
and no methods. The Go port sorts every handle subtype into one of four fates:

1. **Becomes a public type** — it names a real game-world or engine noun (`unit`, `player`,
   `timer`).
2. **Becomes a value type** — it exists only because JASS lacked structs (`location` → `Vec2`;
   facing reals → `Angle`).
3. **Becomes a Go language feature** — the trigger zoo (`trigger`, `event`, `triggercondition`,
   `triggeraction`, `boolexpr`, `filterfunc`, `conditionfunc`) collapses into `OnEvent` +
   closures (R-API-4); `group`/`force` enumeration collapses into slices
   ([R-EXEC-4](execution-model.md#5-collections-callback-enum--slices)).
4. **Becomes a constant/enum** — the dozens of flavor handles (`race`, `attacktype`,
   `playercolor`, `mapflag`, `gamespeed`, …) are typed constants, not objects.

## 2. The public type inventory

The target surface, ~20 types (final list frozen by the M2 API spec):

| # | Type | Kind | Absorbs (JASS handles / families) |
|---|---|---|---|
| 1 | `Game` | root object | `gamestate`, `gamespeed`, `gamedifficulty`, `mapflag`, `mapsetting`, `version`; creation natives; victory/defeat; `gamecache` (persisted via Go-side storage) |
| 2 | `Player` | noun | `player`, `playerstate`, `playercolor`, `playerslotstate`, `alliancetype`, `racepreference`, `startlocprio` |
| 3 | `Force` | collection noun | `force` (player group); kept as a noun (not a bare slice) because alliances/visibility are stateful per-force |
| 4 | `Unit` | noun | `unit`, `unitstate`, `unittype`, `unitpool`; order natives; animation natives |
| 5 | `Item` | noun | `item`, `itemtype`, `itempool` |
| 6 | `Destructable` | noun | `destructable` |
| 7 | `Widget` | interface | `widget` — the common surface of `Unit`/`Item`/`Destructable` (position, life, as order/damage target) |
| 8 | `Ability` | noun | `ability`; level/field accessors |
| 9 | `Buff` | noun | buff/aura state (JASS hides this in ability+unitstate corners) |
| 10 | `Order` | value | order strings *and* order ids (`string order` / `integer order` native twins, per [D3](deduplication-policy.md#4-d3--native-family-differs-only-by-typearity-one-canonical-function-on-the-most-general-form)) |
| 11 | `Timer` | noun | `timer`, `timerdialog` |
| 12 | `Event` | value (payload) | `event`, `eventid` + every `Get*` event-context native (`GetTriggerUnit`, `GetEnumUnit`, …) becomes a method on the payload |
| 13 | `Region` | noun | `region` (cell-based, enter/leave events) |
| 14 | `Rect` | value | `rect` |
| 15 | `Vec2` | value | `location` (and every `real x, real y` pair) |
| 16 | `Angle` | value | facing/orientation reals; degrees/radians confusion ends here |
| 17 | `Sound` | noun | `sound`, `soundtype`, `volumegroup` |
| 18 | `Effect` | noun | `effect`, `effecttype`, `lightning`, `weathereffect`, `texttag`, `minimapicon` — transient presentation objects |
| 19 | `Camera` | noun | `camerasetup`, `camerafield` (clamped per [PRD R-RND-1](../../PRD.md#52-rendering-g3n-presentation-layer)) |
| 20 | `Frame` (under `Game.UI()`) | noun | `dialog`, `button`, `leaderboard`, `multiboard`, `multiboarditem`, `quest`, `questitem`, `timerdialog` (display part), `trackable` — the WC3 frame-native capability per [PRD R-UI-1](../../PRD.md#54-audio-ui-input) |
| 21 | `Missile` | noun | *no JASS analogue (LitD extension)* — missiles are independent first-class sim objects ([Combat & Orders §3.5](../04-simulation/combat-and-orders.md)): spawnable (`g.SpawnMissile(opts)`), queryable, retargetable mid-flight; guidance + impact behaviors selected from the plugin mechanism registry |

Deliberately **absent**: `trigger`/`boolexpr`/`conditionfunc`/`filterfunc` (→ closures),
`group` (→ `[]Unit`), `location` (→ `Vec2`), `fogmodifier` (→ methods on `Player`/`Force`),
`terraindeformation` (cosmetic; `Effect`), `gamecache` (→ `Game.Storage()`), and all
`commonai` handles (full v1 port at milestone M5.5, but they live in the isolated AI domain
([R-EXEC-3](execution-model.md#6-ai-domain-isolation), [ai-natives](jass-mapping/ai-natives.md)),
not in the core type budget). *Revised 2026-06-11 per D-2026-06-11-6.*

Every type is a small copyable handle struct (entity id + generation + `*Game`); none holds
gameplay state itself ([Architecture §1.1](architecture.md#11-litdapi--the-public-surface)).

## 3. Design rules R-API-1…6

### 3.1 R-API-1 — methods on nouns, never free functions with handle params

```go
// JASS:  call SetUnitOwner(whichUnit, whichPlayer, true)
// LitD:
u.SetOwner(p, litd.WithColorChange())
```

The grouping rule is mechanical: the first handle parameter of the JASS native names the
receiver. Functions with no handle parameter (`CreateUnit`, map queries, global settings) hang
off `Game` — there are no package-level mutating functions, which is also what makes the API
sandbox-friendly for the [v1/M5 Lua binding](architecture.md#6-the-lua-binding-layer-v1-m5):
all authority flows from the `Game` value you were handed. *Revised 2026-06-11 per
D-2026-06-11-8.*

### 3.2 R-API-2 — value types for math; no heap `location`, no manual cleanup

JASS required `local location loc = GetUnitLoc(u)` … `call RemoveLocation(loc)` — a heap object
and a leak hazard per coordinate read. In LitD:

```go
pos := u.Position()              // Vec2, copied on the stack
u.SetPosition(pos.Add(litd.Vec2{X: 64}))
facing := litd.Deg(270)          // Angle value; litd.Rad(x) also exists
```

`Vec2`, `Angle`, `Rect`, `Order`, and `Event` payloads are plain value types: zero allocations in
hot paths (supports [PRD R-GC-3](../../PRD.md#531-go-garbage-collection-discipline)), trivially
serializable, and trivially bindable from Lua. Internally the sim pools whatever it needs; the
public API never shows a lifetime.

### 3.3 R-API-3 — options structs for the long tail

The "complex version only" rule ([deduplication policy](deduplication-policy.md)) means the
canonical function can have many knobs. They are expressed as variadic functional options (or a
single options struct where the call site benefits from naming everything), never as positional
parameter explosions and never as parallel function variants:

```go
// Canonical damage call — full power of the 8-parameter native UnitDamageTarget,
// with BJ-style defaults reachable by omission:
u.Damage(target, 50)                                  // attack=melee defaults
u.Damage(target, 50,
    litd.DamageRanged(),
    litd.DamageType(litd.DamageMagic),
    litd.WeaponType(litd.WeaponWhoKnows),
)
```

Conventions: option constructors live in `litd` next to the method they serve; zero options must
reproduce the most common BJ default (documented in godoc with the originating BJ name per
[Naming & style §3](naming-and-style.md#3-the-jassgo-mapping-table)); options are plain data —
they may not capture callbacks or mutate state when constructed.

### 3.4 R-API-4 — events replace the trigger zoo

The JASS pattern — `CreateTrigger`, `TriggerRegisterUnitEvent`, `TriggerAddCondition(Condition(
function ...))`, `TriggerAddAction` — becomes:

```go
sub := g.OnEvent(litd.EventUnitDeath, func(e litd.Event) {
    fmt.Println(e.Unit().Name(), "killed by", e.KillingUnit().Name())
})
defer sub.Cancel()   // replaces DestroyTrigger / DisableTrigger

// Scoped registration replaces the TriggerRegisterPlayerUnitEvent / AnyUnitEventBJ split:
g.OnEvent(litd.EventUnitDeath, handler, litd.ForPlayer(p))
```

Filters (the wait-free `boolexpr` role) are a distinct, deliberately weaker shape — they receive
read-only state and return `bool`, and cannot reach mutation or waits; purity is enforced by the
type they are handed ([Execution model §4](execution-model.md#4-filter-purity)). Dispatch and
resume ordering guarantees are specified in [Execution model §2–3](execution-model.md).

### 3.5 R-API-5 — error semantics and zero-value handles

WC3 scripts are crash-tolerant: calling a native on a null/removed unit silently does nothing.
LitD keeps those semantics, formalized:

- **No panics in hot paths, no error returns on gameplay verbs.** `u.SetLife(50)` on a dead or
  invalid handle is a no-op; `u.Life()` returns `0`; `u.Position()` returns the zero `Vec2`;
  methods returning handles return the zero-value handle, so chains degrade safely:
  `g.UnitsIn(r, nil)` over an empty region yields an empty slice, and
  `e.KillingUnit().Owner().Print(...)` on an environmental death prints nowhere instead of
  crashing the map.
- **Validity is queryable.** Every handle type has `Valid() bool` (and `IsZero()`); generation
  counters in the handle make stale references to recycled entity slots detectably invalid rather
  than aliased.
- **Debug mode asserts.** With `cfg.Debug = true`, any call on an invalid handle logs the call
  site and (optionally) panics — so development catches the bug WC3 would have swallowed, while
  shipped maps keep WC3's forgiveness.
- **Errors are reserved for setup.** Construction-time and pipeline operations that can genuinely
  fail (`litd.NewGame(cfg)`, `g.LoadMap(path)`) return `error` like normal Go. The split is:
  *setup returns errors; gameplay verbs never do.*

### 3.6 R-API-6 — zero G3N types in public signatures

No `litd/api` signature mentions `*core.Node`, `*camera.Camera`, or any other G3N type; camera,
effects, and UI are expressed in LitD's own vocabulary. Enforced by the API-surface lint from M2
and architecturally by the import rules
([Architecture §2](architecture.md#2-import-rules)). Consequences: headless builds drop the
renderer entirely, the renderer is replaceable, and the Lua binding never has to marshal a
foreign engine type.

## 4. The illustrative surface, annotated

The PRD §4.3 snippet, with the rules it exercises:

```go
g := litd.NewGame(cfg)                       // setup path: returns error (R-API-5)

p := g.Player(0)                             // noun off the root (R-API-1)
u := g.CreateUnit(p, "footman",              // D2/D3 canonical creator; return value
    litd.Vec2{X: 128, Y: 256}, litd.Deg(270))//   replaces GetLastCreatedUnit (tombstone)

u.SetLife(u.MaxLife() * 0.5)                 // D5 typed accessors over one state table
u.Order(litd.OrderAttackMove, target.Position()) // Order value type; Vec2 (R-API-2)

g.OnEvent(litd.EventUnitDeath, func(e litd.Event) {  // R-API-4
    fmt.Println(e.Unit().Name(), "died")
})

g.After(30*time.Second, func() { g.Defeat(p, "time out") }) // timer collapse (D5 §6 ex. 6)
```

`time.Duration` appears in signatures as the ergonomic unit; all durations quantize to 50 ms sim
ticks on entry ([R-EXEC-5](execution-model.md#3-waits-and-quantization)) — the type is stdlib,
the semantics are sim-time.

## 5. What "smallest possible" does *not* mean

Three boundaries, to prevent over-shrinking:

1. **No capability merging across nouns.** `Item` and `Destructable` stay distinct types even
   though both are `Widget`s — merging them would force callers into type switches WC3 never
   required.
2. **No cleverness over clarity.** Generics appear only where they remove real duplication
   (e.g. a single `Subscription` type); the API must read like the PRD snippet, not like a
   constraint puzzle.
3. **Helpers don't count against the budget.** `litd/api/helpers`
   ([D4](deduplication-policy.md#5-d4--bj-adds-real-logic-keep-once-in-helpers)) may grow freely;
   the ~20-type budget governs the *core* — the set a developer must understand to be fully
   powerful.

## 6. Acceptance criteria for this section

- M2 API spec enumerates the final type list with every method signature; count of core public
  types ≤ 22.
- Audit report ([Deduplication policy §8](deduplication-policy.md#8-the-audit-report)) maps all
  non-tombstoned source functions onto these types' methods, helpers included.
- API-surface lint: zero G3N types, zero exported fields on noun handles, zero `error` returns on
  gameplay verbs, `Valid()` present on every handle type.
- Example map from the M6 vertical slice compiles against the frozen v1 surface with no use of
  internal packages.
