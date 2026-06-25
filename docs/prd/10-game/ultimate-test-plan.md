# Ultimate Final Test — AI-vs-AI full-match engine validation

**Status:** planning (no code yet). **Owner:** TBD. **Date opened:** 2026-06-24.
**Relates to:** #210 (headless full-match replay), #212 (FSV acceptance vs M5.5 AI), #204 (mid-game save/load, shipped), #325 (M6 gate), #304 (hero progression).

## 1. Thesis

One closed loop validates the whole engine:

> declarative match spec → two autonomous CPU players driven **only through the public API/Lua** → deterministic terminal result → recorded → replayed bit-identical.

If the loop holds it transitively exercises: sim core, public Go API, Lua bindings, melee AI, record/replay, save/load, victory resolution, hero progression. That is the acceptance proof, executed per `prompts/fsv.md`.

## 2. Decisions (locked 2026-06-24)

| # | Decision | Rationale |
|---|---|---|
| D1 | First map = **new minimal melee `worlds/firstclash/`** | fastest path to a *terminating* AI-vs-AI run; firstflame deferred to Phase 6 / #212 |
| D2 | Per-race start logic = **Lua `melee/<race>.lua`**, run on game load, swappable | the "custom-game/mod" layer; binding melee verbs to Lua doubles as API validation |
| D3 | Runner = **new `cmd/matchfsv`** | keeps `cmd/headless` synthetic-grid determinism gate untouched |
| D4 | Match never-ends backstop = **20 in-game-min timeout = 24,000 ticks**, score-decide → winner | tick-counted (R-SIM-2), not wall-clock; yields a winner, exercises score logic |
| D5 | Hero present, AI-controlled, leveling; **hero death NOT auto-loss** | avoids trivial endings; last-standing/decisive-rule/timeout decides |
| D6 | Phase-1 stretch: **one Lua auto-cast hero ability** | validates ability API end-to-end |

## 3. Tick/time facts

- Tick = exactly **50 ms** (`litd/sim/step.go:9`); `TicksPerSecond = 1000/TickMS = 20` (`litd/data/loader.go:40`).
- **20 in-game minutes = 24,000 ticks.**
- "Sped up" is free headless: `cmd/headless` runs ungated (~1.9M ticks/s measured) → 24k ticks ≈ 13 ms wall. The 20-min rule is identical in headless and windowed because it counts ticks, not wall-clock. Windowed fast testing = `-speed N` / `-maxspeed` flag (Phase 6).

## 4. Current state (≈70% built)

| Capability | State | Provenance |
|---|---|---|
| Deterministic melee AI, full Vigil-vs-Unbound match, 10k ticks bit-identical | ✅ | `litd/ai/melee/controller.go:56`; `litd/ai/determinism_test.go:308` |
| Per-race AI strategy = TOML data, not code | ✅ | `data/ai/vigil.toml`, `data/ai/unbound.toml`; `litd/ai/melee/strategy.go` |
| AI message-passing only (no sim poke, R-EXEC-3) | ✅ | `litd/ai/context.go:31`; `litd/api/ai_melee.go:28` |
| Match flow state machine | ✅ standalone | `litd/match/flow.go:21` |
| Melee setup helpers (resources/units/victory/`Standard`) | ✅ Go-only | `litd/api/helpers/melee/melee.go:106,127,183,242` |
| World load (dir + `.litdworld`: data TOML + `main.lua`) | ✅ | `litd/worldhost/worldhost.go:52` |
| ~410 Lua bindings (CreateUnit/Order/Train/OnEvent/Timer/Victory/AttachMeleeAI) | ✅ | `litd/luabind/register.go:38`; `litd/api/victory.go:46` |
| Replay record/verify `.litdreplay` v3, checkpoint trace, per-system bisection | ✅ | `litd/sim/replay.go:1`; `cmd/headless/main.go:102` |
| AI match → replay with NO controllers → identical (commands-only) | ✅ unit-test | `litd/ai/replay_ai_prod_test.go:26` (#404) |
| State hash + 30-system bisection | ✅ | `litd/statehash/*`; `litd/sim/hash.go:21` |
| Mid-match save/load → resume → hash-identical | ✅ SHIPPED #204 | `litd/savegame/savegame_test.go:101` |
| Headless runner (no GL; hash+dump+eventlog+replay) | ✅ synthetic grid only | `cmd/headless/main.go:35` |
| Hero data + `DefineHeroes`; `melee.Extra` carries hero | ✅ | `data/.../heroes.toml`; `melee.go:35` |

## 5. Gaps

| # | Gap | Evidence |
|---|---|---|
| G1 | `cmd/headless` builds synthetic 256-unit grid; never `worldhost.Load`, no AI attach, no victory poll | `cmd/headless/main.go:35`; `-map` refuses |
| G2 | No per-race start-game script concept | each `worlds/*/main.lua` self-contained |
| G3 | Melee setup verbs Go-only, **not Lua-bound** | absent from `bindings_gen.go` |
| G4 | `match.Setup` hardcodes 2 factions | `litd/match/flow.go:57` |
| G5 | AI-vs-AI termination not guaranteed (turtle stalemate) | `melee.go:183` last-standing only |
| G6 | Match flow not wired into `cmd/game` (no setup UI / phase screenshots) | `flow.go` standalone |
| G7 | AI cannot cast hero abilities / scout / expand (deferred M5.5+) | `litd/ai/captain.go:89` |

## 6. The mod / custom-game shape

```
worlds/firstclash/
  data/**.toml          # unit/tech/ability/hero tables (exists)
  match.toml            # NEW: custom-game descriptor (seed, victory, time_limit_ticks, players[])
  melee/vigil.lua       # NEW: race default start-game function (swappable)
  melee/unbound.lua     # NEW
  main.lua              # bootstrap: read match.toml → per player run melee/<race>.lua
```

`match.toml`: `seed`, `victory="beacon|hall|score"`, `time_limit_ticks=24000`, `players=[{slot,race,controller="cpu",difficulty,ai_strategy}]`.
A mod swaps `melee/<race>.lua` or overrides the binding in `main.lua` — zero engine edits. Default behavior reaches `melee.Standard()` logic *through Lua* so the run validates the public API.

## 7. The 20-minute timeout rule (deterministic score-decide)

One-shot timer at tick 24,000 in the victory script:

```
score(p) = ( buildings, armyValue, workers, gold+lumber )   # integer tuple
winner   = max score; ties → lower slot index (deterministic)
Game_Victory(winner); Game_Defeat(others)
exact tuple tie → both Left (terminal draw, rare)
```

Integer compare, slot-index tiebreak, no float/map-iteration → determinism-safe. Lives in the swappable victory script.

## 8. Hero integration (threads all phases)

| Where | What |
|---|---|
| Data | hero type per race in `heroes.toml`/`vigil.toml`/`unbound.toml`; `DefineHeroes` binds rules |
| Spawn | start-script puts hero in `melee.Extra` (`melee.go:35`) — Phase 1 |
| AI | hero marches with waves via Captain readiness (`litd/ai/captain.go:89`); gains XP/levels; no ability cast (G7) except D6 optional |
| Scoring | hero counts in `armyValue` tiebreak; **death non-decisive** (D5) |
| Evidence | dump hero level over time (must increase); item-pickup event if items seeded (#212, #304) |

## 9. Build order

| Phase | Deliverable | Closes | Provenance |
|---|---|---|---|
| 0 | `match.toml` schema + loader; generalize `match.Setup` past 2 factions | G4 | `litd/match/flow.go:57` |
| 1 | Bind melee verbs to Lua (`Game_MeleeStartingUnits`/`StartingResources`/`InstallVictoryDefeat`/`AttachMeleeAI`/`SpawnHero`); `main.lua` runs `melee/<race>.lua`; D6 optional ability | G2,G3 | `melee.go:242`; `litd/luabind/register.go:38` |
| 2 | `cmd/matchfsv`: `worldhost.Load` → start-scripts → AI attach → step-until-terminal (`Player.Result()`) → emit replay+hash+eventlog+dump | G1 | `cmd/headless/main.go:35`; `litd/campaignrun/campaignrun.go:146` |
| 3 | `worlds/firstclash/` decisive map + 20-min score-timeout, archived | G5 | `worlds/victory-destruction/main.lua` |
| 3.5 | Timeout rule wired + verified (forced-to-tick-24000 path fires) | D4 | §7 |
| 4 | #210 gate: record → replay no-controllers → hash+trace identical; induced-fault bisection; 3× local | #210 | `litd/ai/replay_ai_prod_test.go:26`; `cmd/desyncfsv/main.go:84` |
| 5 | Mid-match save/load → resume → hash-identical (real AI match + hero + suspended waves) | #204 | `litd/savegame/savegame_test.go:101` |
| 6 | Wire match flow into `cmd/game` + `-speed`/`-maxspeed`; firstflame per-phase screenshot+state evidence | #212, G6 | `cmd/game` autotest; `flow.go` |
| 7 | M6 evidence ledger, manual FSV | #325 | `prompts/fsv.md` |

**Phases 0–5 = the ultimate engine test. Phases 6–7 = formal M6 close, same runner reused.**

## 10. Per-phase FSV checklists

Source of truth = artifacts (hashes, state JSON, eventlog, screenshots), inspected by hand. Never trust exit codes alone (`prompts/fsv.md`).

**Phase 0** — valid `match.toml` loads; bad race / missing field → loud fail-closed (no coercion); `flow.Setup` carries N players. Evidence: parse test output.

**Phase 1** — headless load → both players: correct units, resources, AI attached, hero alive lvl 1; swap `melee/vigil.lua` → setup changes, zero engine edits. Evidence: printed roster pre/post swap.

**Phase 2** — two CPUs play to terminal ≤ safety cap; print winning slot, duration ticks, AI command counts (nonzero + increasing = AI played), hero final level. Evidence: stdout + artifacts.

**Phase 3 / 3.5** — both endings fire: (a) one match ends by elimination/decisive rule; (b) one forced to tick 24,000 → score-decide → winner printed; no stalemate. Evidence: two run logs, score tuples printed.

**Phase 4** — record AI match → replay no-controllers → final hash + full checkpoint trace identical. Induced fault: patch one PRNG draw → divergence detected + bisected to named system → revert. Run 3×. Evidence: hash table (3 runs), replay header dump, induced-fault bisection output.

**Phase 5** — save mid-match at tick N → load → resume → H1==H2 printed; hero level + in-flight wave identical pre/post; corrupt save → loud refusal. Evidence: hash pair, coroutine/wave dumps, refusal output.

**Phase 6** — per phase: PNG read + paired `state:` JSON cross-checked (beacon owner color vs JSON; hero level on screen vs JSON). Phases: cold start → economy → training → hero level-up + item → combat → beacon/timeout → victory screen. Evidence: phase table (phase→PNG→JSON→verdict).

**Phase 7** — one ledger row per M6 exit criterion (`milestones.md §10`): criterion → command → artifact inspected → pass. Any red → root-cause issue filed + fixed first.

## 11. Risks

- **G3 Lua binding (biggest unknown):** everything moddable hangs on binding melee verbs. De-risk in Phase 1 first.
- **G5 termination:** firstclash victory rule must force resolution; tick-cap + score-timeout = backstop, not the primary win.
- **G7 AI depth:** no hero-cast/scout/expand. Match terminates, depth shallow. Disclose in evidence, do not overstate.
- **Multi-OS deferred:** no-CI policy permanent (CLAUDE.md). Local 3× substitutes #210's cross-OS clause.
- **Map format (M5) deferred:** start positions from world data tables, not a terrain editor.
