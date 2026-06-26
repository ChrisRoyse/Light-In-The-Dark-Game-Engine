# Public API — Deduplication Policy ("complex version only")

> Expands [PRD §4.2 (Deduplication policy)](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule).
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Siblings** | [Architecture](architecture.md) · [Public API design](public-api-design.md) · [Execution model](execution-model.md) · [Naming & style](naming-and-style.md) |

---

## 1. The problem being solved

The WC3 scripting surface is 2,521 functions: **1,536 natives** in `common.j` plus **985 BJ
helpers** in `blizzard.j`. The BJ layer exists because the GUI trigger editor needed
one-function-per-trigger-action; it wraps natives with reordered parameters, baked-in defaults,
`bj_lastCreated*` side-channel globals, and occasionally real logic. Porting both layers verbatim
would double the API for zero added capability and ship WC3's accidents as our design.

The policy: every duplicated pair collapses into **exactly one canonical Go symbol — the most
general ("complex") version** — classified mechanically into rules D1–D5 by `tools/jassgen`
([PRD R-AST-4](../../PRD.md#6-asset--data-pipeline)). Functions that should not exist in LitD at
all are **tombstoned** (§7). The audit report (§8) proves nothing was silently dropped and nothing
exists twice.

All JASS signatures below are quoted verbatim from `repoes/war3-types/scripts/common.j` and
`scripts/blizzard.j`.

## 2. D1 — BJ is a pure passthrough: drop the BJ, native is canonical

The BJ adds nothing: same parameters, same order, direct call. The BJ disappears without a trace
beyond its manifest entry.

| # | JASS (blizzard.j) | Wraps (common.j) | Canonical Go |
|---|---|---|---|
| 1 | `function IssueTargetOrderBJ takes unit whichUnit, string order, widget targetWidget returns boolean` | `native IssueTargetOrder takes unit whichUnit, string order, widget targetWidget returns boolean` | `Unit.OrderTarget(ord Order, t Widget) bool` (further collapsed by D3, §4 ex. 3) |
| 2 | `function IssuePointOrderLocBJ takes unit whichUnit, string order, location whichLocation returns boolean` | `native IssuePointOrderLoc takes unit whichUnit, string order, location whichLocation returns boolean` | same `Unit.OrderPoint` family (D3) |
| 3 | `function IsUnitPausedBJ takes unit whichUnit returns boolean` | `native IsUnitPaused takes unit whichUnit returns boolean` (body: `return IsUnitPaused(whichUnit)`) | `Unit.Paused() bool` |
| 4 | `function DestroyTimerBJ takes timer whichTimer returns nothing` | `native DestroyTimer takes timer whichTimer returns nothing` | none needed — `Timer` is GC-managed (R-API-2); manifest maps it to `Timer.Stop()` |
| 5 | `function GetTransportUnitBJ takes nothing returns unit` | `native GetTransportUnit takes nothing returns unit` | `Event.TransportUnit() Unit` |
| 6 | `function ForGroupBJ takes group whichGroup, code callback returns nothing` | `native ForGroup takes group whichGroup, code callback returns nothing` (BJ adds only the `bj_wantDestroyGroup` bookkeeping flag — GUI-editor plumbing, not capability) | superseded entirely by slice returns, [R-EXEC-4](execution-model.md#5-collections-callback-enum--slices) |

Roughly a third of blizzard.j is D1; it is the bulk discount that makes the ~20-type API
([public-api-design.md](public-api-design.md)) possible.

## 3. D2 — BJ reorders or defaults parameters: drop the BJ, canonical takes the full set

The BJ exists to fit the GUI editor's argument-order or to hide parameters behind defaults. The
canonical Go function keeps the **full** parameter set; defaults live in options structs
(R-API-3), not in shadow functions.

| # | JASS (blizzard.j) | The "complex" native it hides | Canonical Go |
|---|---|---|---|
| 1 | `function PauseUnitBJ takes boolean pause, unit whichUnit returns nothing` — only swaps argument order over `native PauseUnit takes unit whichUnit, boolean flag returns nothing` | `PauseUnit` | `Unit.SetPaused(bool)` |
| 2 | `function GetUnitStateSwap takes unitstate whichState, unit whichUnit returns real` — argument swap over `constant native GetUnitState takes unit whichUnit, unitstate whichUnitState returns real` | `GetUnitState` | typed accessors per D5 (§6) |
| 3 | `function UnitDamageTargetBJ takes unit whichUnit, unit target, real amount, attacktype whichAttack, damagetype whichDamage returns boolean` — hard-codes `attack=true, ranged=false, WEAPON_TYPE_WHOKNOWS` | `native UnitDamageTarget takes unit whichUnit, widget target, real amount, boolean attack, boolean ranged, attacktype attackType, damagetype damageType, weapontype weaponType returns boolean` | `Unit.Damage(target Widget, amount float64, opts ...DamageOption)` — full power, defaults via options |
| 4 | `function TriggerRegisterTimerEventPeriodic takes trigger trig, real timeout returns event` and `...EventSingle` — each bakes one boolean of `native TriggerRegisterTimerEvent takes trigger whichTrigger, real timeout, boolean periodic returns event` | `TriggerRegisterTimerEvent` | `Game.Every(d, fn)` / `Game.After(d, fn)` — the boolean becomes two honestly-named methods on one implementation |
| 5 | `function CreateUnitAtLocSaveLast takes player id, integer unitid, location loc, real face returns unit` — wraps `native CreateUnitAtLoc` plus the `bj_lastCreatedUnit` side channel (and a hard-coded `'ugol'` special case) | `native CreateUnit takes player id, integer unitid, real x, real y, real face returns unit` | `Game.CreateUnit(owner, typ, pos, facing)` — return values replace `GetLastCreatedUnit()`-style globals (§7) |
| 6 | `function SetPlayerFlagBJ takes playerstate whichPlayerFlag, boolean flag, player whichPlayer returns nothing` — bool→int shim over `native SetPlayerState takes player whichPlayer, playerstate whichPlayerState, integer value returns nothing` | `SetPlayerState` | typed accessors per D5 |

## 4. D3 — native family differs only by type/arity: one canonical function on the most general form

`common.j` itself duplicates: x/y vs `location` variants, string vs id variants, counted vs
uncounted variants. Value-type `Vec2` (R-API-2) dissolves the entire `location` axis.

| # | JASS family (common.j) | Canonical Go |
|---|---|---|
| 1 | `native SetUnitX takes unit whichUnit, real newX returns nothing` · `native SetUnitY ...` · `native SetUnitPosition takes unit whichUnit, real newX, real newY returns nothing` · `native SetUnitPositionLoc takes unit whichUnit, location whichLocation returns nothing` (plus BJ `SetUnitPositionLocFacingBJ`, `SetUnitPositionLocFacingLocBJ`) | `Unit.SetPosition(Vec2)` only. Teleport-without-pathing-check (`SetUnitX/Y` semantics) is `PositionOption{SkipPathing: true}` |
| 2 | `native CreateUnit takes player id, integer unitid, real x, real y, real face returns unit` · `native CreateUnitAtLoc takes player id, integer unitid, location whichLocation, real face returns unit` (also `CreateUnitByName`) | `Game.CreateUnit(owner Player, typ UnitType, pos Vec2, facing Angle) Unit` |
| 3 | `native IssuePointOrder takes unit whichUnit, string order, real x, real y returns boolean` · `IssuePointOrderLoc` · `IssuePointOrderById` · `IssuePointOrderByIdLoc` (string×id ⨯ xy×loc = 4 natives; same again for target/immediate orders) | `Unit.OrderPoint(ord Order, p Vec2) bool` — `Order` is one typed value covering both string and id spellings |
| 4 | `native Location takes real x, real y returns location` · `native RemoveLocation ...` · `native GetLocationX ...` · `native GetLocationY ...` · `constant native GetUnitLoc takes unit whichUnit returns location` vs `constant native GetUnitX/GetUnitY` | the whole `location` type is gone: `Vec2{X, Y float64}` value type; `Unit.Position() Vec2`; no destroy call exists to forget |
| 5 | `native GroupEnumUnitsInRect takes group whichGroup, rect r, boolexpr filter returns nothing` · `native GroupEnumUnitsInRectCounted takes group whichGroup, rect r, boolexpr filter, integer countLimit returns nothing` (the `Counted` twin exists for every enum native) | `Game.UnitsIn(r Rect, filter func(Unit) bool) []Unit` — a count limit is a slice operation, not an API |
| 6 | `native DisplayTextToPlayer takes player toPlayer, real x, real y, string message returns nothing` · `native DisplayTimedTextToPlayer takes player toPlayer, real x, real y, real duration, string message returns nothing` (+ `DisplayTextToForce` BJs that re-route via `GetLocalPlayer`) | `Player.Print(msg string, opts ...PrintOption)` with `Duration` and `Offset` options; force variants via `Force.Print` delegating to the same implementation |

## 5. D4 — BJ adds real logic: keep once, in `litd/api/helpers`

Some BJ functions are genuine programs. Their *logic* is capability and is preserved — once, as
documented helpers layered strictly on top of the core API
([Architecture §1.1](architecture.md#11-litdapi--the-public-surface)), never shadowing it.

| # | JASS (blizzard.j) | What the logic is | LitD home |
|---|---|---|---|
| 1 | `function PolledWait takes real duration returns nothing` | game-time wait via timer polling (real waits drift with game speed) | `helpers.PolledWait(d Duration)` — suspends on the [cooperative scheduler](execution-model.md#3-waits-and-quantization) |
| 2 | `function TriggerRegisterAnyUnitEventBJ takes trigger trig, playerunitevent whichEvent returns nothing` | loops `TriggerRegisterPlayerUnitEvent` over all `bj_MAX_PLAYER_SLOTS` players | absorbed into core: `Game.OnEvent(EventUnitDeath, fn)` is any-player by default; per-player scoping is the option (`EventOption{Player: p}`) |
| 3 | `function CreateNUnitsAtLoc takes integer count, integer unitId, player whichPlayer, location loc, real face returns group` | creation loop + group collection | `helpers.CreateUnits(g, n, owner, typ, pos, facing) []Unit` |
| 4 | `function MeleeStartingUnitsHuman takes player whichPlayer, location startLoc, boolean doHeroes, boolean doCamera, boolean doPreload returns nothing` (and the Orc/Undead/NightElf/UnknownRace siblings + dispatcher `MeleeStartingUnits`) | the melee-mode game-setup program | `helpers/melee` package, data-driven from `data/` tables instead of five hard-coded race functions |
| 5 | `function GetUnitStatePercent takes unit whichUnit, unitstate whichState, unitstate whichMaxState returns real` (used by `GetUnitLifePercent` etc.) | ratio computation with null/zero guards | `Unit.LifePercent()` — small enough to live on the noun, backed by the one D5 state table |
| 6 | `function RandomDistReset takes nothing returns nothing` (+ `RandomDistAddItem`, `RandomDistChoose`) | weighted random distribution machinery | `helpers.WeightedChoice` drawing from the sim's seeded PRNG (R-SIM-2) |

Rule of placement: if the logic is generic plumbing a Go developer would otherwise write
(`PolledWait`, batch creation), it is a helper; if it encodes WC3 *game content* (melee starting
units), it is a helper in a clearly-content-flavored subpackage; if it is one expression deep, it
may be absorbed as a method on the noun. In every case it calls only public API.

## 6. D5 — getter/setter pairs across states: collapse onto typed accessors

JASS multiplexes dozens of properties through enum-keyed state functions, then blizzard.j adds
per-property wrappers back on top — duplication in both directions.

| # | JASS | Canonical Go |
|---|---|---|
| 1 | `constant native GetUnitState takes unit whichUnit, unitstate whichUnitState returns real` / `native SetUnitState takes unit whichUnit, unitstate whichUnitState, real newVal returns nothing` with `UNIT_STATE_LIFE`, `UNIT_STATE_MAX_LIFE`, `UNIT_STATE_MANA`, `UNIT_STATE_MAX_MANA` | `Unit.Life()`, `Unit.SetLife(v)`, `Unit.MaxLife()`, `Unit.Mana()`, `Unit.SetMana(v)`, `Unit.MaxMana()` — six typed accessors over **one** internal state table |
| 2 | `function SetUnitLifeBJ takes unit whichUnit, real newValue returns nothing` (body: `SetUnitState(whichUnit, UNIT_STATE_LIFE, RMaxBJ(0,newValue))`) and `SetUnitManaBJ` | folded into `Unit.SetLife` / `Unit.SetMana`; the clamp-at-zero is canonical behavior, documented in godoc |
| 3 | `function SetUnitLifePercentBJ takes unit whichUnit, real percent returns nothing` · `SetUnitManaPercentBJ` · `function GetUnitLifePercent takes unit whichUnit returns real` · `IsUnitDeadBJ` | `Unit.LifePercent()`, `Unit.SetLifePercent(p)`, `Unit.Dead()` — same one table |
| 4 | `constant native GetPlayerState takes player whichPlayer, playerstate whichPlayerState returns integer` / `native SetPlayerState ...` with `PLAYER_STATE_RESOURCE_GOLD`, `..._LUMBER`, `..._FOOD_CAP`, … (+ BJ wrappers `SetPlayerFlagBJ`, `GetPlayerTaxRateBJ`, …) | `Player.Gold()`, `Player.SetGold(n)`, `Player.Lumber()`, `Player.FoodCap()`, … over one player-state table |
| 5 | `native SetUnitAnimation takes unit whichUnit, string whichAnimation returns nothing` · `native QueueUnitAnimation ...` (+ index/rarity variants) | `Unit.PlayAnimation(clip AnimClip, opts ...AnimOption)` with `Queue: true` option — clip names contractual per [PRD R-AST-3](../../PRD.md#6-asset--data-pipeline) |
| 6 | `function StartTimerBJ ...` / `function CreateTimerBJ takes boolean periodic, real timeout returns timer` / `PauseTimerBJ` over `native CreateTimer`, `native TimerStart takes timer whichTimer, real timeout, boolean periodic, code handlerFunc returns nothing`, `PauseTimer`/`ResumeTimer` | `Game.After(d, fn)`, `Game.Every(d, fn)` returning a `Timer` with `Pause`/`Resume`/`Stop` — one timer implementation, no create/start split, no `bj_lastStartedTimer` |

The implementation rule behind D5: each noun keeps **one** state table in `litd/sim`; typed
accessors are generated thin views over it. Adding a property never adds a second storage or a
second code path.

## 7. Tombstone policy

A tombstone is an explicit manifest record stating that a source function maps to **no** Go
symbol, with a machine-readable reason. Tombstoning is the only sanctioned way to drop anything;
an unclassified function fails the M2 milestone gate.

Allowed reasons (closed enum):

| Reason | Meaning | Examples (verbatim signatures) |
|---|---|---|
| `superseded` | Capability fully delivered by a canonical symbol of different shape; nothing to port | `function GetLastCreatedUnit takes nothing returns unit` and the whole `bj_lastCreated*` side-channel family — Go return values replace them. `native RemoveLocation takes location whichLocation returns nothing` — no `location` type exists to remove. `constant native GetEnumUnit takes nothing returns unit` — slice iteration has no hidden current element ([R-EXEC-4](execution-model.md#5-collections-callback-enum--slices)) |
| `gameplay-irrelevant` | Engine-housekeeping with no LitD equivalent by design | `native Cheat takes string cheatStr returns nothing`; `native DoNotSaveReplay takes nothing returns nothing` (replays are core, not optional); `native Preload takes string filename returns nothing` / `native PreloadEnd takes real timeout returns nothing` — the asset pipeline ([Architecture §1.4](architecture.md#14-litdasset--the-content-pipeline)) owns loading |
| `deprecated` | Dead even in WC3 — stubs and editor artifacts | `function CommentString takes string commentString returns nothing` (empty body; GUI comment rows); compat shims in `compat.d.ts` |
| `v2` | Real capability deliberately deferred for a product reason; tombstone carries the target version | currently no members — the former examples flipped to scheduled canonical mappings: all `commonai` natives are a full v1 port at milestone M5.5 ([D-2026-06-11-6](../01-vision/decisions.md), [R-EXEC-3](execution-model.md#6-ai-domain-isolation)); `native SyncStoredInteger takes gamecache cache, string missionKey, string key returns nothing` and the gamecache sync family land with lockstep multiplayer at M7 ([D-2026-06-11-5](../01-vision/decisions.md)). The reason stays in the enum; deferral requires a product reason, never difficulty alone. *Revised 2026-06-11 per D-2026-06-11-6 and D-2026-06-11-5* |

Tombstones are permanent records: if a `v2` tombstone is later implemented, the manifest entry
flips to a mapping but retains its history. The JASS→Go migration table
([Naming & style §3](naming-and-style.md#3-the-jassgo-mapping-table)) renders tombstones with
their reason so a WC3 modder searching for `GetLastCreatedUnit` gets an answer, not a gap.

## 8. The audit report

The acceptance criterion from PRD §4.2, made concrete. `tools/jassgen` emits two artifacts:

1. **`api-manifest.json`** — one record per source function:

```json
{
  "source": "blizzard.j",
  "name": "SetUnitLifeBJ",
  "jassSignature": "function SetUnitLifeBJ takes unit whichUnit, real newValue returns nothing",
  "rule": "D5",
  "canonical": "litd/api.Unit.SetLife",
  "tombstone": null,
  "notes": "clamps to 0 per BJ body; clamp is canonical behavior"
}
```

   A tombstoned record sets `"canonical": null` and
   `"tombstone": {"reason": "superseded", "by": "litd/api.Game.CreateUnit return value", "since": "v1"}`.

2. **`audit-report.md`** — generated summary, CI-gated at M2 (classification) and M5
   (implementation):

```
LitD API audit — generated 2026-06-11 by jassgen vX
Source functions ......... 2,521  (common.j 1,536 + blizzard.j 985)
  Mapped (D1) ............   nnn → 0 Go symbols (dropped passthroughs)
  Mapped (D2) ............   nnn
  Mapped (D3) ............   nnn
  Mapped (D4) ............   nnn → litd/api/helpers
  Mapped (D5) ............   nnn
  Tombstoned .............   nnn  (superseded nnn, gameplay-irrelevant nn, deprecated nn, v2 nnn)
Canonical Go symbols ..... ~nnn across ~20 public types
VIOLATIONS
  Unclassified ........... 0      ← hard gate
  Mapped to 2+ symbols ... 0      ← hard gate
  Go symbol w/o source ... listed & justified (new-capability whitelist)
```

   The invariants the gate enforces:
   - **Totality:** every one of the 2,521 functions has exactly one record.
   - **Uniqueness:** each record names exactly one canonical symbol *or* one tombstone — never
     both, never neither, and (checked across the whole manifest) no two non-D-collapsed
     capabilities share a symbol by accident: many-to-one is legal only when the rule column
     explains it.
   - **Reverse closure (M5):** every exported symbol in `litd/api` (reflected from the compiled
     package) is either named by ≥ 1 manifest record or appears on the explicit new-capability
     whitelist (e.g. `Game.Headless`, snapshot hooks). No unaccounted public surface.

   In PRD terms: **no capability silently dropped, no symbol implemented twice** — proven by a
   generated artifact, not by review.

## 9. Process notes

- Classification is **mechanical first, human second**: `jassgen` auto-classifies D1 (body is a
  single call with identical args) and most D3 families (name-pattern + signature unification);
  D2/D4/D5 and all tombstones require a reviewed annotation in the manifest overrides file.
- The manifest is the single input for: Go stub generation, the audit report, the
  [JASS→Go mapping table](naming-and-style.md#3-the-jassgo-mapping-table), and the v1/M5
  [Lua binding generator](architecture.md#6-the-lua-binding-layer-v1-m5). One inventory,
  four consumers. *Revised 2026-06-11 per D-2026-06-11-8.*
- Disagreements about where something lands (core vs helper vs tombstone) are resolved by one
  question: *does dropping it lose power a Go developer cannot trivially recover?* If yes, core;
  if recoverable but commonly needed, helper; if no, tombstone.
