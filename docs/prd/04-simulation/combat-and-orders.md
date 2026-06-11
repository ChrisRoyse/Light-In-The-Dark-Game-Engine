# Combat & Orders (R-SIM-3 systems, R-AST-1 data-driven)

**Parent:** [PRD §4.4, §5.1, §6](../../PRD.md) · **Related:** [Determinism](determinism.md) · [ECS Architecture](ecs-architecture.md) · [Tick & Scheduler](tick-and-scheduler.md) · [Pathfinding](pathfinding.md)

---

## 1. Design stance

Combat and orders are the systems where WC3's *feel* lives, and the systems most prone to hidden non-determinism (target selection, simultaneous damage, float cooldowns). Two principles govern everything below:

1. **Data-table-driven (R-AST-1):** every number — damage dice, armor coefficients, cooldowns, acquisition ranges, buff durations, projectile speeds — comes from the immutable JSON/TOML tables in `data/`, loaded once at startup. Code implements *mechanisms*; tables define *content*. Durations are stored in seconds in the tables for designer ergonomics and converted to integer tick counts at load ([no sub-tick timing exists](tick-and-scheduler.md), R-EXEC-5).
2. **Deterministic by ordering:** every selection among candidates (targets, queue entries, simultaneous deaths) has an explicitly specified total order, per [R-SIM-2](determinism.md). "Whichever the loop found first" is only acceptable when the loop's order is itself specified — which, via [dense-store iteration](ecs-architecture.md), it always is.

## 2. The WC3 order system

### 2.1 Orders as the universal verb

Everything a unit does is an **order**: move, attack, attack-move, patrol, hold, stop, smart, cast, build, harvest, follow. Orders are small value structs (order ID, target variant: none/point/entity, queue flag) drawn from the pooled order-entry store (R-GC-2; capacities in [ECS §2](ecs-architecture.md)). The public API surface is `Unit.Order(...)` / `Unit.OrderQueued(...)` per PRD §4.3, collapsing the JASS `IssueImmediateOrder`/`IssuePointOrder`/`IssueTargetOrder` triplet (§4.2 D3).

### 2.2 Smart orders

The right-click "smart" order is resolved at issue time, in the **CommandIngest phase**, by a deterministic decision table over the target:

| Right-click target | Resolves to |
|---|---|
| Ground point | `move` |
| Enemy unit/building | `attack` |
| Allied unit (if transport/shop semantics apply) | `board`/`interact`, else `follow` |
| Own building rally context | `setrally` |
| Resource (worker) | `harvest` |
| Item on ground | `pickup` (moves, then takes) |

The resolution table is data (R-AST-1 — a unit type can override smart behavior, e.g. workers vs fighters), and the resolved concrete order — not the raw click — is what enters the order queue, the replay command stream, and the [state hash](determinism.md).

### 2.3 The order queue

Each unit has a current order plus a FIFO queue (shift-queued orders, WC3 semantics), implemented as pooled intrusive list entries:

- **Issue (unqueued):** clears the queue, interrupts the current order through its state machine's interrupt edge (§5), installs the new order.
- **Issue (queued):** appends; executes when the predecessor completes.
- **Completion/failure:** an order signals `done` (arrived, target dead, cast finished) or `failed` (unreachable, can't afford); either pops the next entry or, if the queue is empty, drops the unit to its **default order** — `stop` for most units, auto-acquire stance making it effectively "hold position until something enters acquisition range" (§3.1).
- Order transitions raise events (`EventOrderIssued`, `EventOrderDone`) in the [event flush phase](tick-and-scheduler.md), in deterministic order, so scripts can chain behavior exactly as WC3 triggers do.
- Order resolution for all units happens in the **Orders phase** (tick phase 3), iterating the order store in dense-row order.

## 3. The attack cycle

### 3.1 Acquisition

Two ranges from the unit table govern targeting (WC3 semantics):

- **Acquisition range** — how far an idle/attack-moving/holding unit looks for targets on its own initiative.
- **Attack range** — how far it can actually strike; chasing closes from acquisition to attack range.

Acquisition scans run in the **Combat phase** for units in an acquiring state, throttled to every N ticks (default 5; data-tunable) with per-unit phase offset derived from entity index — spreading the cost across ticks *deterministically* (offset is a function of state, not of load). Candidate filtering uses a coarse spatial bucket grid (cell size ≥ max acquisition range / 2, preallocated, rebuilt incrementally in the movement phase).

### 3.2 Deterministic target acquisition ordering

Among valid candidates (enemy, alive, visible, attackable by this weapon's targets-allowed mask — all table-driven flags), selection is by lexicographic tuple:

```
(threatClass, distanceSq, entityIndex)
```

1. **threatClass** — table-driven priority tiers (e.g. attacking-me > combat unit > worker > structure), reproducing WC3's "units that damage me get priority" behavior via a damage-memory field on the Combat component (last attacker, decaying over ticks).
2. **distanceSq** — fixed-point squared distance, computed in widened integers ([Determinism §2.1](determinism.md)); no square roots in the comparison path.
3. **entityIndex** — final total-order tie-break. Two byte-identical sims pick the same target, always.

The candidate iteration order (bucket scan order) does not affect the outcome because the comparison is a total order over the full candidate set — the loop keeps a running best under the tuple comparison. This is the pattern for *every* "pick one of several" decision in the engine (shove targets in [pathfinding §5](pathfinding.md), projectile hit resolution, item stacking).

### 3.3 Attack state machine

Per weapon, a tick-driven state machine with all durations as integer tick counts from the table:

```
IDLE → ACQUIRE → CHASE → WINDUP(damagePoint ticks) → FIRE → BACKSWING → COOLDOWN → ACQUIRE …
```

- **Cooldown** — full attack period in ticks; an integer "next attack tick" clock, never a float accumulator.
- **Damage point (windup)** — delay from animation start to damage/launch, the WC3 mechanic that makes attack-canceling and damage timing matter. If the target leaves range or dies during windup, the attack cancels (table flag controls whether cooldown is consumed — WC3-faithful default: it is not).
- **Backswing** — post-damage animation time during which a new order interrupts freely (orb-walking/animation-canceling works as in WC3, because the *sim* model carries the windup/backswing split; render merely plays clips cued to these states per the [interpolation contract](tick-and-scheduler.md)).

### 3.4 Instant vs projectile delivery

The weapon table's delivery field selects:

- **Instant (melee/hitscan):** at FIRE, a damage packet (value struct: source, target, amount-rolled, attack type, flags) is written to the deferred-damage buffer.
- **Projectile:** at FIRE, a projectile is allocated from the pool ([ECS §5](ecs-architecture.md)) carrying the packet plus speed/arc/homing from the table. Projectiles advance in the Projectiles phase; homing projectiles (WC3 default) track the target entity and deliver on arrival even if the source died; a target that becomes invalid mid-flight expires the projectile (table flag: or detonates AoE at last position). Damage is rolled **at launch** (WC3 semantics), so the PRNG call order stays anchored to the FIRE event regardless of flight time.

All damage packets from a tick — instant and arriving projectiles — are applied in a single deferred pass in deterministic buffer order, so mutual kills and overkill are well-defined ([ECS §6](ecs-architecture.md) deferred-effect rule).

## 4. Damage and armor tables

The WC3 damage model, fully table-driven (R-AST-1):

- **Attack types** × **armor types** → a percentage coefficient matrix in `data/combat/damage-table.json` (e.g. `piercing vs light: 150`, `piercing vs fortified: 35` — illustrative; LitD ships original balance data per PRD §2.2 non-goals). Coefficients are integer per-mille values; the multiply is fixed-point, no float percentages.
- **Damage dice:** weapon damage is `base + Ndice × roll(sides)` rolled on the [sim PRNG](determinism.md) at the FIRE event — one PRNG call site, deterministic call order.
- **Armor value reduction:** the WC3 formula `reduction = (armor × k) / (1 + armor × k)` with k = 0.06 is precomputed at load into a lookup table over the practical armor range (−20..100, table-driven bounds) in fixed-point — no runtime division, no float, and negative-armor amplification falls out of the same table.
- **Pipeline order (fixed, documented):** flat modifiers → attack-vs-armor coefficient → armor-value reduction → multiplicative buff modifiers (sorted by buff ID then instance index — deterministic) → final clamp. `EventUnitDamaged` fires with pre/post amounts; `EventUnitDeath` fires in the cleanup-feeding event flush if life reaches zero.

## 5. Buff and ability state machines

### 5.1 Abilities

Each ability instance (Ability component slot) is a table-defined state machine:

```
READY → TARGETING(client-side only) → PRECAST(turn+approach) → CASTPOINT(ticks)
      → EFFECT → [CHANNEL(ticks)] → BACKSWING → COOLDOWN(ticks) → READY
```

- Table fields: mana cost, cooldown ticks, cast point ticks, cast range, targets-allowed mask, effect ID, channel duration, AoE shape/size — the WC3 object-data analogue.
- **EFFECT** dispatches on effect ID to a registry of effect mechanisms (damage, heal, apply-buff, summon, teleport, …) implemented in code; everything parameterizing them is table data. New abilities are new table rows composing existing mechanisms — adding a *mechanism* is an engine change, adding an *ability* is not.
- Interrupts (new order, stun, silence) take edges defined per state: PRECAST/CASTPOINT cancel without cost; CHANNEL cancels with cost spent — WC3-faithful, table-overridable.
- Mana is fixed-point with per-tick regen increments; cooldowns are absolute "ready at tick T" integers (cheap, hash-friendly, no per-tick decrement loops).

### 5.2 Buffs

Buff instances live in their own pooled store ([ECS §5](ecs-architecture.md)): `(buffID, target, source, remainingTicks, stacks, periodicClock)`.

- **Table fields:** duration ticks, stacking rule (refresh / stack-count / independent / strongest-wins), periodic interval and periodic effect ID, stat modifiers (additive and per-mille multiplicative), flags (dispellable, persists-through-death, aura-child).
- **Periodic effects** (poison ticks, regen auras) fire when `tick % interval == phase`, phase fixed at application — deterministic and load-spread.
- **Auras** are a source-side buff that maintains child buffs on units in radius, re-evaluated on the same throttled cadence as acquisition scans, with the standard WC3 linger duration (child outlives aura range-exit by a table-driven grace period).
- **Stat resolution:** modifiers fold into cached derived stats (move speed, attack cooldown, armor) recomputed only when the buff set changes, in deterministic fold order (buff ID, then instance index). The cache is derived state — excluded from the hash only once the derivation is covered by tests, per the [hashing strategy](determinism.md).
- Expiry and dispel are processed in the cleanup phase, raising `EventBuffExpired` in deterministic store order.

## 6. Worked data-table example (R-AST-1)

To make "tables define content" concrete, an illustrative unit row and the mechanisms it touches:

```jsonc
// data/units/footman.json (illustrative values, original balance per PRD §2.2)
{
  "id": "footman",
  "life": 420, "regen": 0.25, "armor": 2, "armorType": "heavy",
  "moveSpeed": 270, "turnRate": 0.6, "collisionSize": 16, "pathing": "ground",
  "acquisitionRange": 600, "model": "units/footman.glb",
  "attacks": [{
    "type": "normal", "range": 90, "damageBase": 11, "dice": 1, "sides": 2,
    "cooldown": 1.35, "damagePoint": 0.5, "backswing": 0.5, "delivery": "instant",
    "targetsAllowed": ["ground", "structure"]
  }],
  "abilities": ["defend"]
}
```

At load: `cooldown 1.35 s` → 27 ticks, `damagePoint 0.5 s` → 10 ticks, `moveSpeed 270` → per-tick fixed-point displacement, `armorType "heavy"` → row index into the damage matrix, `collisionSize 16` → collision class for the [dilated grid layers](pathfinding.md). The loader rejects unknown fields, out-of-range values, and references to undefined abilities/effects — table errors are build/load failures, never runtime surprises. The loaded tables are immutable and shared; their content hash is part of the [replay header](determinism.md).

## 7. Acceptance hooks

- Table schema validation in CI (R-AST-1/R-AST-2 pipeline): unknown fields, missing required clips, out-of-range coefficients fail the build.
- Headless combat scenarios ([R-SIM-4](tick-and-scheduler.md)): scripted 500-unit engagements asserting final state hashes; mutual-kill, mid-flight-death, and interrupt edge cases are explicit fixtures.
- Zero allocations across order issue → projectile flight → death at steady state, enforced per R-GC-1/5 by the [ECS benchmarks](ecs-architecture.md).
