# Platform — Input

> Expands [PRD §5.4 (Audio, UI, input)](../../PRD.md#54-audio-ui-input), requirement **R-INP-1**.
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Parent requirement** | R-INP-1: WC3-grade input model — drag-select, control groups 0–9, smart/right-click orders, hotkeys, edge-pan + middle-drag camera |

---

## 1. Scope and the one structural rule

Input is where the determinism guarantee (G5,
[PRD §5.1](../../PRD.md#51-simulation-core-deterministic)) is either won or lost. The
structural rule of this spec, stated up front because everything else serves it:

> **The simulation never sees the mouse or keyboard. It sees only a serialized command
> stream.** Every gesture — drag-select, right-click, hotkey, command-card click — resolves
> in the presentation layer into a versioned, byte-encoded command record. The sim consumes
> ordered command records per tick and nothing else.

Raw input handling, selection state, camera state, and hotkey resolution are all
**client-side presentation state** outside the determinism boundary. Selection itself is
*not* sim state: a player's current selection lives in the input layer, and only the
*orders issued to* selected entities cross into the sim (as commands carrying explicit
entity-ID lists). This is the same separation lockstep RTS netcode requires, which is why
§8 is the groundwork for replays and v2 multiplayer.

This document refines R-INP-1 into **R-INP-1.1 … R-INP-1.7**.

## 2. Selection: drag-select and click-select (R-INP-1.1)

- **R-INP-1.1:** Left-click selects; left-drag draws a screen-space marquee and selects on
  release. Selection resolution follows WC3-grade priority rules:

**Single click** (on an entity's selection footprint, with a small pixel tolerance):
selects exactly that entity — own, allied, enemy, or neutral. Enemy/neutral selection is
view-only: the command card shows no issuable orders.

**Drag marquee** — candidates are entities whose footprint intersects the marquee in screen
space, filtered by priority classes evaluated in order; the first non-empty class wins:

1. The local player's **units** (mobile entities). A marquee covering own units, own
   buildings, and enemy units selects only the own units.
2. The local player's **buildings**, only if the marquee contains no own units.
3. Nothing — a drag never selects enemy or neutral entities (parity with WC3).

Within class 1, non-combat "low-priority" unit types flagged in the data tables (R-AST-1;
e.g. workers in some game designs) are excluded when at least one normal-priority unit is in
the marquee — the data-table flag is the generalized form of WC3's selection-priority field.

**Selection cap:** default **12** (WC3 parity), configurable per game via `GameConfig`
(0 = unlimited). When a marquee exceeds the cap, the closest-to-marquee-center
normal-priority entities fill the selection. The cap is presentation policy, but commands
carry explicit ID lists (§8), so the sim is indifferent to it.

**Modifiers:**

| Gesture | Effect |
|---|---|
| `Shift` + click/drag | Add to selection (toggle on click: clicking an already-selected unit removes it) |
| `Ctrl` + click | Select all units of the clicked type currently on screen |
| Double-click | Same as `Ctrl`+click (type-select on screen) |
| `Tab` | Cycle active **subgroup** within a mixed selection (next unit type); the command card ([UI and HUD §2](./ui-and-hud.md)) follows the active subgroup |

Selection changes emit a UI sound and update the info panel/portrait; they generate **no
sim commands** by themselves.

## 3. Control groups 0–9 (R-INP-1.2)

- **R-INP-1.2:** Ten control groups, keys `0`–`9`, with WC3 semantics:

| Gesture | Effect |
|---|---|
| `Ctrl` + `N` | Assign current selection to group N (replaces previous contents) |
| `Shift` + `N` | Add current selection to group N |
| `N` | Recall group N as current selection |
| `N` `N` (double-tap, ≤ 300 ms) | Recall and center the camera on the group's centroid |

Groups are client-side state: stored as entity-ID lists in the input layer, pruned lazily
when dead/removed IDs are recalled. They are never serialized into the command stream —
only the orders issued to a recalled selection are. The control-group bar widget mirrors
this state ([UI and HUD §2](./ui-and-hud.md)). Entity IDs are generation-counted sim IDs,
so a recycled slot can never resurrect into an old group.

## 4. Smart orders: right-click resolution (R-INP-1.3)

- **R-INP-1.3:** Right-click issues the contextual "smart" order. Resolution is a pure
  function of (target under cursor, selection composition), evaluated in the input layer
  using read-only sim state, producing exactly one explicit order opcode. The table below is
  the v1 contract; rows are extensible through the data-driven ability system:

| Cursor target | Selection contains | Resolved order |
|---|---|---|
| Ground | anything mobile | `Move` (formation point spread applied sim-side) |
| Enemy unit/building | any combat-capable unit | `Attack` (target ID) |
| Own/allied unit | units | `Follow`; if target is a transport with capacity → `Board` |
| Resource node | at least one harvester | `Harvest` for harvesters, `Move` for the rest (split orders, one command per capability class) |
| Own damaged building/unit | at least one repairer | `Repair` for repairers, `Move` for the rest |
| Own building under construction | builder of that type | `ResumeConstruction` |
| Minimap point | anything mobile | Same resolution against the world location ([UI and HUD §2](./ui-and-hud.md), minimap frame) |

The resolved order is explicit in the command record — the sim never re-runs smart
resolution. This keeps the gesture→order mapping a client concern (re-skinnable, moddable)
while the sim's input vocabulary stays a small closed opcode set. Explicit-order hotkeys
(`A` attack-move + click, `M` move, `S` stop, `H` hold, `P` patrol) bypass smart resolution
and enter a targeting mode where the next click supplies the target; `Stop`/`Hold` need no
target and dispatch immediately.

Invalid orders (unreachable, fog-hidden, dead target at click time) are filtered
client-side with error feedback (UI sound + text, [Audio §4](./audio.md)); orders that
become invalid in flight are rejected by the sim deterministically (a no-op with an event,
per R-API-5's no-panic semantics).

## 5. Hotkeys and rebinding (R-INP-1.4)

- **R-INP-1.4:** Every command-card slot, control gesture, and camera control is driven by
  a **keymap**, not hard-coded keycodes.

- **Command card hotkeys are grid-positional.** The 4 × 3 command card
  ([UI and HUD §2](./ui-and-hud.md)) binds to a 12-key grid, default
  `QWER / ASDF / ZXCV` ("grid hotkeys"). Abilities declare their card *position* in the
  data tables (R-AST-1); the key follows the position. A "classic" profile that instead
  reads a per-ability hotkey field from the data tables is supported for WC3-style muscle
  memory — both reduce to "keymap entry → card slot → ability ID → command", so the sim and
  the command encoding are unaffected by profile choice.
- **Keymap file:** TOML under the user config directory, hot-reloadable at the menu, plus an
  in-game rebinding screen. Bindings are
  `action = [key, modifiers...]`; one physical key may appear in multiple *contexts*
  (game, targeting mode, replay viewer) but conflicts within a context are rejected at load
  with a clear error listing both actions.
- **Reserved keys:** `0`–`9` (+ `Ctrl`/`Shift` chords), `Shift` (queue, §7), `Tab`
  (subgroup), `Esc` (cancel) are rebindable but pre-bound in every profile; `F10` menu
  parity with WC3 is a default, not a rule.

The public API exposes the keymap read-only (`g.Input().Keymap()`) so games can render
hotkey labels on custom UI; mutation goes through the config file/screen only.

## 6. Camera controls (R-INP-1.5)

- **R-INP-1.5:** Camera control implements the WC3 feel against the locked RTS rig of
  R-RND-1 (fixed yaw, clamped pitch and zoom —
  [PRD §5.2](../../PRD.md#52-rendering-g3n-presentation-layer)):

| Control | Behavior |
|---|---|
| Edge pan | Cursor within 4 px of a screen edge pans at keymap-configurable speed (acceleration ramp over 300 ms); disabled when a modal dialog is open; windowed-mode edge behavior is a user setting |
| Arrow keys | Pan, same speed model |
| Middle-drag | Grab-and-drag pan (1:1 world tracking) |
| Mouse wheel | Zoom, clamped to the R-RND-1 range; zoom pivots on the cursor's ground point |
| `Backspace` / `F1` | Jump to (own main building) / (hero), WC3 parity defaults in the keymap |
| Double-tap group key | Center on group (§3) |
| Minimap click-drag | Continuous camera reposition ([UI and HUD §2](./ui-and-hud.md)) |

Camera state is pure client state — never serialized, never visible to the sim. Replays
therefore allow free camera movement during playback, which falls out of the architecture
rather than being a feature.

## 7. Order queueing: the Shift modifier (R-INP-1.6)

- **R-INP-1.6:** Holding `Shift` while issuing any order (right-click, explicit-order
  click, command-card ability, building rally) **appends** the order to each recipient's
  order queue instead of replacing it. Unmodified orders clear the queue and become the
  sole entry. This is encoded as a single `queued` flag bit in the command record (§8); the
  per-entity order queue itself is sim state, with entries drawn from preallocated pools
  per R-GC-2 ([GC Discipline §4](../08-performance/gc-discipline.md)). Queue depth is
  capped (default 16) sim-side; excess queued orders are deterministically dropped with an
  event. Waypoint paths, patrol routes, and build-queue chaining are all expressions of
  this one mechanism — no special cases.

## 8. The deterministic command stream (R-INP-1.7)

This is the load-bearing section: the contract between the input layer and
[PRD §5.1](../../PRD.md#51-simulation-core-deterministic)'s deterministic core, and the
groundwork G5 requires for replays and v2 lockstep netcode.

- **R-INP-1.7:** The sim's only input is an ordered stream of **command records**. The
  pipeline:

```
raw events (G3N window)                     presentation side
  → gesture recognition (drag, click, chord)
  → resolution (selection rules §2, smart orders §4, hotkeys §5)
  → CommandRecord encode                    ← determinism boundary
  → per-tick command queue, ordered (tick, playerID, sequence)
  → sim consumes at tick start              simulation side
```

**Record encoding.** Fixed little-endian binary, no maps, no floats:

| Field | Type | Notes |
|---|---|---|
| `version` | u8 | Encoding version; replays refuse mismatches |
| `tick` | u32 | Execution tick (assigned at enqueue: next unsimulated tick in v1 single-player; netcode in v2 will schedule tick+delay) |
| `playerID` | u8 | Issuing player |
| `seq` | u16 | Per-player sequence number within the tick — total order for same-tick commands |
| `opcode` | u8 | Closed set: `Move`, `Attack`, `Stop`, `Hold`, `Patrol`, `CastAbility`, `Train`, `Build`, `Cancel`, `Rally`, `Harvest`, `Repair`, `Board`, `Unload`, … (registry frozen per encoding version) |
| `flags` | u8 | Bit 0: `queued` (§7); remaining bits reserved |
| `payload` | opcode-specific | Entity-ID lists (u32 generation-counted IDs, explicit count), target ID or fixed-point world coordinates (the sim's native 16.16/32.32 representation per R-SIM-2 — coordinates are quantized at *encode* time so client float math never leaks in), ability/unit-type IDs (u16 data-table indices) |

Design consequences, each deliberate:

1. **Replays are free.** A replay file is the map ID, the PRNG seed, and the command
   stream. Replaying = re-simulating. The 10k-tick state-hash test of M1
   ([PRD §7](../../PRD.md#7-milestones)) runs exactly this way, and the headless CI
   benchmarks ([Budgets and Benchmarks §3](../08-performance/budgets-and-benchmarks.md))
   use recorded command streams as their workload definition.
2. **Netcode is an exchange problem, not a redesign.** v2 lockstep multiplayer exchanges
   the same records and schedules them at `tick + latency`; nothing about the sim's input
   contract changes ([PRD §2.2](../../PRD.md#22-non-goals-v1)).
3. **Cheating surface is bounded.** The sim validates every record (ownership of every
   listed entity ID, ability availability, tech requirements) deterministically; invalid
   records are rejected identically on every machine.
4. **No hidden inputs.** Pause, surrender, ally/vision changes, and chat-triggered cheats
   in dev builds are commands too. If it affects sim state, it is in the stream — the
   audit rule for every new feature.
5. **Zero-allocation encode path.** Records encode into a preallocated per-tick ring
   buffer; entity-ID lists copy into pooled scratch arrays
   ([GC Discipline §4](../08-performance/gc-discipline.md), R-GC-1/2 apply to the input
   layer's per-frame work).

The public API mirrors this: `Unit.Order(...)` and friends
([PRD §4.3](../../PRD.md#43-api-shape-handles--typed-objects)) construct the same command
records and enqueue them — scripted orders and player orders are indistinguishable to the
sim, which is what makes scripted scenarios replay-stable.

## 9. Acceptance criteria

1. Marquee priority, selection cap, modifier, and subgroup behavior verified by a scripted
   UI test suite against constructed scenes (own units + buildings + enemies in one
   marquee, cap overflow, type-select).
2. Smart-order resolution table covered by table-driven tests: every (target class ×
   selection composition) row produces the specified opcode.
3. A recorded 10k-tick command stream replays to a bit-identical state hash, headless, in
   CI (G5/R-SIM-2; the M1 test extended with full input vocabulary by M5).
4. Fuzzed command records (malformed payloads, foreign entity IDs, dead IDs) never panic
   the sim and produce deterministic rejection (R-API-5 semantics).
5. Input-layer per-frame work passes the zero-allocation gate
   ([GC Discipline §5](../08-performance/gc-discipline.md)) under continuous drag-select +
   order spam in the benchmark scene.
6. Keymap conflict detection and rebinding round-trip (save → reload → identical bindings)
   covered by unit tests.

## 10. Related documents

- [UI and HUD](./ui-and-hud.md) — command card, control-group bar, minimap interactions that feed this pipeline.
- [Audio](./audio.md) — gesture and error feedback sounds.
- [Budgets and Benchmarks](../08-performance/budgets-and-benchmarks.md) — command streams as benchmark workloads.
- [GC Discipline](../08-performance/gc-discipline.md) — pooled order-queue entries, zero-alloc encode path.
- [PRD §5.4](../../PRD.md#54-audio-ui-input) — parent requirement R-INP-1; [PRD §5.1](../../PRD.md#51-simulation-core-deterministic) — the determinism contract this feeds.
