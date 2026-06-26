# JASS API → Go Mapping — Category Index

Expanded PRD documentation for porting the WC3 JASS API surface to the Light in the
Dark canonical Go API. Governing document: [PRD §4.2 (dedup rules D1–D5) and §4.3
(API shape)](../../../PRD.md).

## Source surface

| Source file | Functions |
|---|---|
| `repoes/war3-types/scripts/common.j` | 1,534 natives (1,243 `native` + 291 `constant native`) |
| `repoes/war3-types/scripts/blizzard.j` | 985 BJ functions |
| `repoes/war3-types/scripts/common.ai` | 123 natives (full v1 port at M5.5 per D-2026-06-11-6, see [ai-natives](ai-natives.md)) |
| **Total v1 classification surface** | **2,642** (incl. 123 common.ai, milestone M5.5) |

*Revised 2026-06-11 per D-2026-06-11-6: common.ai joins the v1 classification surface
(milestone M5.5).*

Counts below come from a pattern-based grep survey (2026-06-11) and are
**approximate** — boundary functions (e.g. waygates, quests, hero items) are
assigned by judgment. The authoritative per-function classification is the
`jassgen` manifest (PRD R-AST-4, milestone M2); these documents are its design brief.

## Categories

| Category | common.j natives | blizzard.j BJs | Total | Dominant dedup rules |
|---|---:|---:|---:|---|
| [units](units.md) | ~235 | ~125 | ~360 | D1, D3 (Loc/XY), D5 (unitstate) |
| [ui-frames-and-dialogs](ui-frames-and-dialogs.md) | ~190 | ~88 | ~278 | D5 (frame get/set), D2 |
| [triggers-and-events](triggers-and-events.md) | ~130 | ~80 | ~210 | R-API-4 collapse, D5 (event responses) |
| [abilities-and-buffs](abilities-and-buffs.md) | ~140 | ~67 | ~207 | D5 (field reflection matrix), D3 |
| [game-state-and-melee](game-state-and-melee.md) | ~105 | ~72 | ~177 | D4 (melee library), tombstones |
| [players-and-forces](players-and-forces.md) | ~85 | ~90 | ~175 | D2 (alliance presets), force → slices |
| [effects-and-graphics](effects-and-graphics.md) | ~97 | ~78 | ~175 | D5 (Blz effect setters), D3 |
| [hashtable-and-gamecache](hashtable-and-gamecache.md) | ~130 | ~41 | ~171 | D3 (type matrix → generics), tombstones |
| [items](items.md) | ~93 | ~65 | ~158 | D1, D3 (Loc), D2 (slot/id) |
| [sound-and-music](sound-and-music.md) | ~49 | ~59 | ~108 | D2 (volume scales), D4 (transmissions) |
| [math-strings-conversion](math-strings-conversion.md) | ~55 | ~41 | ~96 | stdlib tombstones, D3 (geometry → Vec2) |
| [camera](camera.md) | ~51 | ~34 | ~85 | D2 (per-player guards), D5 (camera fields) |
| [groups-and-enumeration](groups-and-enumeration.md) | ~31 | ~38 | ~69 | R-EXEC-4 (callback-enum → slices) |
| [regions-rects-locations](regions-rects-locations.md) | ~39 | ~30 | ~69 | D3/R-API-2 (location → Vec2 value type) |
| [destructables-and-doodads](destructables-and-doodads.md) | ~33 | ~35 | ~68 | D4 (gates/elevators), D3 |
| [visibility-and-fog](visibility-and-fog.md) | ~35 | ~19 | ~54 | D3 (rect/radius), D5 (state triple) |
| [timers](timers.md) | ~20 | ~22 | ~42 | D2 (create+start), last-handle deletion |
| [ai-natives](ai-natives.md) — **full v1 port at M5.5** (D-2026-06-11-6) | ~8 (+123 common.ai) | ~1 | ~9 (+123) | canonical AI-domain mappings (R-EXEC-3) |
| **Sum** | **~1,526** | **~985** | **~2,511** | |

The ~8-native gap between the column sum (~1,526) and the true count (1,534) is
classification slack on boundary functions; the M2 manifest resolves every one
exactly, per the §4.2 acceptance criterion (each of the 2,642 functions either maps
to exactly one canonical Go symbol or is tombstoned with a reason).

## Cross-cutting conventions (apply to every category)

- **Nouns own methods** (R-API-1): `Unit.Kill()`, never `KillUnit(u)`.
- **`location` is dead** (R-API-2): every `...Loc` variant in every category is a D3 collapse onto `Vec2`. See [regions-rects-locations](regions-rects-locations.md).
- **`GetLastCreatedX`/`bj_lastCreated*` side channels are dead**: constructors return their object.
- **Callback enumeration is dead** (R-EXEC-4): slice/iterator returns. See [groups-and-enumeration](groups-and-enumeration.md).
- **The trigger zoo is dead** (R-API-4): one `OnEvent` + closures. See [triggers-and-events](triggers-and-events.md).
- **Per-player presentation** (camera, UI, text, fog display) takes the player as a receiver/parameter; the `GetLocalPlayer()` branch idiom is structurally eliminated. See [camera](camera.md) hazard 1.
- **Sim/render tagging**: each file's "Subsystem dependencies" section assigns natives to `litd/sim`, `litd/render`, or `litd/asset` per the §4.1 hard rule (sim never imports render).
- **D4 helpers are the dogfood gate**: `helpers`/`melee` packages must be implementable purely on the public API; if they can't, the canonical API is missing capability.
