# Issue Manifest V3

Source of truth: live GitHub open-issue inventory, refreshed by the Fable 5 full-spectrum audit.

- Refreshed at: 2026-07-09 (supersedes Manifest V2 / docs/issue-manifests/issue1476-manifest-v2.md).
- Open issue count: 260
- Exact-once rule: every currently-open issue appears once in one phase below.
- Delta from V2: 50 issues FSV-closed or superseded (Go/g3n-era supersession sweep #1475 + closed trust/MCP/perf work); 13 new audit issues filed (#1482-#1493) and slotted; 3 post-V2 issues (#1479/#1480/#1481) placed.
- Map doctrine: #944 is binding. #816 may implement editor/loading-preview only; the RPG runtime ships no built-in map/minimap/world-map/player-dot/GPS layer/free quest path.

## Next Dependency-Ready Issue

Phase 01 (Build Health, Trust, Backlog Hygiene) is the gate for everything after it. Start with the RED build baseline **#1493** (litd-editor suite 8/469 failing on ORT lens), then the trust/correctness cluster **#1483** (false-green sweep II), **#1484** (engine correctness), **#1463**, **#1464**. Do not advance to feature phases (04+) while Phase 01 trust issues are open — a green gate is a precondition for FSV on later work.

## Phase Summary

| Phase | Count | Purpose |
|---|---:|---|
| 00 | 20 | Control Plane: Doctrine, Context, Manifest |
| 01 | 13 | Build Health, Trust, And Backlog Hygiene |
| 02 | 10 | MCP And Calyx Foundation |
| 03 | 19 | AI Agent And Authoring Loop |
| 04 | 13 | Engine, Runtime, Render, And Performance |
| 05 | 49 | Editor, WC3 Parity, And Script Surface |
| 06 | 12 | UX, Onboarding, And Creator Loop |
| 07 | 20 | Story, Lore, And Narrative Tools |
| 08 | 37 | RPG Systems, Economy, And RP Enforcement |
| 09 | 57 | Content, Assets, And Audio Buildout |
| 10 | 8 | Multiplayer, Servers, Hub, And Ecosystem |
| 11 | 2 | Packaging, Release, And Final Gates |

## Phase 00 - Control Plane: Doctrine, Context, Manifest

- [#691](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/691) - [EPIC][MASTER] LitD → no-code creation platform + UGC marketplace (Calyx-backed, Windows, RunPod)
- [#777](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/777) - [DOCTRINE] Calyx usage in LitD — embedder diversity, provenance, grounding, FSV (READ FIRST, mirrors Calyx-Dev #863)
- [#778](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/778) - [MANDATE] Multi-embedder testing is binding — ≥10 diverse lenses (A35/A36/A37); <10 or redundant fails closed (mirrors Calyx-Dev #787)
- [#779](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/779) - [CONTEXT] Environment & ops — local Windows + RTX 5090, Calyx at C:\calyx, aiwonder source-of-truth
- [#780](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/780) - [CONTEXT][READ FIRST] START HERE — onboarding for fresh AI agents (read this + linked issues = full context)
- [#800](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/800) - [DOCTRINE] Engineering & coding standards for AI agents — FSV-against-reality, no-workarounds, <500-line files, Synapse, research-first
- [#884](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/884) - [DOCTRINE] Storytelling is the #1 goal — build games that tell amazing stories and bring worlds to life
- [#885](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/885) - [DOCTRINE] End-user experience — most fun/approachable builder ever; WC3-simple surface, deep underneath
- [#915](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/915) - [DOCTRINE] The engine IS an MCP server — every capability is an agent-controllable MCP tool (CORE)
- [#928](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/928) - [CONTEXT][PERF] DUAL target — Editor maxes the workstation; produced GAMES run on a $500-$1,000 player PC (scalable, Battle.net model)
- [#944](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/944) - [DOCTRINE][RPG] Acquired maps only — no built-in game map/minimap
- [#959](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/959) - [DOCTRINE][END GOAL] North-star: fully recreate Carrion Fields (visually, SP+MP) — the proof the engine can build any RPG
- [#995](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/995) - [DOCTRINE] Core game design — multiplayer, PvP, full-loot, roleplay-ENFORCED (the Carrion Fields model, day one)
- [#1071](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1071) - [DOCTRINE] Custom proprietary Rust game engine (wgpu) — DROP g3n; Editor and game strictly separate; engine-first
- [#1073](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1073) - [CONTEXT] Engine dependency & license policy — permissive (MIT/Apache) only; the approved crate stack
- [#1166](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1166) - [DOCTRINE] Two fast-feedback loops — agent (headless, verdicted) + human (instant, immersive), both Calyx-backed
- [#1268](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1268) - [P0][AGENT-OPS] Prevent Codex Bad Request interruptions during long agent runs
- [#1341](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1341) - [MANIFEST][COMPLETION] Production-ready definition of done — atomic task set (all FSV-accepted ⇒ app complete)
- [#1348](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1348) - [DOCTRINE][CALYX] Structured data is association-native — ingest ALL engine structured data into Calyx (single source of truth), capture every association, extract maximum intelligence, lower to the hot path
- [#1349](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1349) - [CONTEXT][CALYX] Structured-data lens taxonomy, DDA yield & capability map — the maximal diverse lens set and every Calyx capability mapped to structured game data

## Phase 01 - Build Health, Trust, And Backlog Hygiene

- [#542](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/542) - [DEBT] purge 3.5MB renderevidence binary from git history (operator-gated rewrite)
- [#713](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/713) - infra: drop non-Windows support — park Mac/Linux/WSL paths, keep Windows headless determinism
- [#1463](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1463) - [P1][SIM] Sim integrity: unbounded attack_events makes per-tick hashing O(all attacks ever); past-due orders silently pruned; determinism boundary guards same-thread only
- [#1464](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1464) - [P2] Silent-fallback sweep: agent identity remap, poisoned-mutex device-loss mask, swallowed exit-state write, zeroed ledger hashes, VFX evidence zeros, neutral matchup fallback, unverified package asset hashes
- [#1465](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1465) - [P1][DOCTRINE] Binding reference files untracked: prompts/ (incl. fsv.md, referenced by #800) and docs2/STRUCTURED_DATA_CALYX_INTELLIGENCE.md (spec behind #1348) exist only in one working tree
- [#1466](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1466) - [P2][DEBT] 33 files >=500 lines violate #800 modularization doctrine (worst: in_context_feedback.rs 1174, mcp_object_tools_store.rs 1025, human_loop.rs 949)
- [#1474](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1474) - [P1][DECISION] Rust-era custom-script escape hatch — no Lua/WASM runtime exists in the workspace; #803/#834/#769 still promise gopher-lua from the dropped Go engine
- [#1475](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1475) - [P1][BACKLOG] Supersession sweep: ~40 open Go/g3n-era issues describe the dropped architecture; #298 roadmap actively wrong; #928 self-contradictory; README appendix stale
- [#1483](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1483) - [P1][FSV] False-green sweep II — ~20 more harnesses compute a verdict then return Ok() (siblings of #1458 the fix missed)
- [#1484](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1484) - [P2][BUG] Four litd-engine correctness defects where an unguarded unwrap_or/overflow breaks fail-loud (sprite atlas panic, RewardLoyalty rank fabrication, con-loss misreport, activation floor)
- [#1491](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1491) - [P2][DEBT] Dead code + duplication: 509 lines of orphaned editor-mcp forks (never compiled) + sha256 helper copy-pasted ~70x despite litd-hash
- [#1492](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1492) - [P2][DEBT] Build/config/observability hygiene: CWD-relative log path breaks packaged .exe, global jobs=1, clippy/cargo-deny never run, ~30 undocumented env vars, 151MB README PNGs
- [#1493](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1493) - [P1][BUILD] litd-editor lib suite RED (8/469 fail): CALYX_LENS_UNREACHABLE — CPU-explicit ONNX lens a38_qwen3_embedding_06b needs ORT_DYLIB_PATH (undocumented)

## Phase 02 - MCP And Calyx Foundation

- [#797](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/797) - authoring: Lodestar kernel / grounded "AGI search" in the editor (kernel_answer over the world corpus)
- [#798](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/798) - authoring: Oracle prediction / forecast in the editor (predict outcomes/balance, what-if, recurrence)
- [#799](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/799) - ops: Calyx alerting + anomaly/crash-pattern monitoring for editor + game (latency, OOD, agent anomalies)
- [#907](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/907) - calyx: replay & telemetry analytics loop — grounded balance (Assay) + desync forensics (Lodestar) + recurrence forecast (Oracle), offline/read-only
- [#908](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/908) - calyx: asset-library multi-modal semantic index — image/audio/structured/text lenses over GLB/textures/audio; find-similar, near-dup, budget-governance clustering
- [#1455](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1455) - [P0][CALYX][CONTENT] Ashen Reach content is hardcoded Rust in litd-engine (Infernal Deeps, mortality, Hall of the Remembered, soul weapon…) — side-store violation of #1348; migrate to editor-authored, Calyx-ingested, lowered data
- [#1479](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1479) - [P1][CALYX] Engine CPU-explicit content panel excludes sparse/ColBERT A37 families
- [#1487](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1487) - [P1][SECURITY] Editor MCP tools accept unconfined filesystem paths — arbitrary local read/write/clobber outside project root (confused-deputy via poisoned package/catalog)
- [#1488](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1488) - [P1][BUG][AI] Calyx MCP bridge classifies tool mutation by EXECUTING the tool — fail-open authn + real tool-body side effects every AI-panel frame
- [#1490](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1490) - [P2][BUG][MCP] litd loopback robustness gaps: no catch_unwind, no initialize handshake, unwritten loopback ledger, latest-only tool ledger, non-atomic publish

## Phase 03 - AI Agent And Authoring Loop

- [#717](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/717) - [EPIC] AI-assisted authoring (Calyx-backed) + lowering valve + Ward content gate (P1)
- [#718](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/718) - authoring: ingest world/asset corpus into Calyx as constellations (data tables, specs, Lua, asset meta)
- [#719](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/719) - authoring: wire editor + agents to Calyx MCP tools over the bridge (search/dedup/bits/guard/propose)
- [#720](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/720) - authoring: natural-language → ability spec (grounded draft → abilities/ → abilitycheck)
- [#721](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/721) - authoring: natural-language → unit (copy nearest unit + apply described deltas, WC3 Object-Editor style)
- [#722](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/722) - authoring: semantic search over abilities/units/worlds in the editor
- [#723](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/723) - authoring: content dedup (TCT cosine near-dup) across catalog; surface in editor + on upload
- [#724](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/724) - authoring: grounded balance analysis (Assay) — anchor playtest outcomes, measure stat→outcome bits
- [#725](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/725) - authoring: the lowering valve — Calyx artifact → validated fingerprinted litd/data TOML (the only runtime path)
- [#726](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/726) - authoring: Ward content gate — calibrate over trusted content; AI proposals must pass before lowering
- [#727](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/727) - authoring: provenance — every AI authoring action ledgered + reproducible; shown in editor history
- [#728](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/728) - authoring: AI-assisted ECA trigger suggestion (NL → Event/Condition/Action draft on the existing trigger model)
- [#729](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/729) - test: FSV — AI authors 'frost_bolt' from NL, Ward-gated, lowered, abilitycheck, cast in-sim deterministic
- [#781](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/781) - [EPIC] In-game AI agents — Calyx-driven living worlds (Ward-governed, deterministic-safe)
- [#980](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/980) - agents: AI agents AS player characters — Calyx agents play as PCs alongside human players + NPCs, full PvP participants under identical rules
- [#982](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/982) - rp/calyx: roleplay-QUALITY measurement engine — Calyx grounds & scores how well a player embodies their cabal/religion/god/role theme (from actions, dialogue, events)
- [#983](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/983) - rp: cabal/religion RANK advancement + power/ability unlocks — driven by measured RP-quality + AI leader approval (this is how you GROW)
- [#1454](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1454) - [P0][AI] No LLM call site exists in the workspace — DeepSeek agent service unbuilt; co-writer/NPC/Immortal agents are deterministic fixtures
- [#1486](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1486) - [P1][PERF][AI] AI/nav hot path re-hashes+re-indexes immutable navmesh per query, String-keyed A* open set, O(n) perception LOS, 3-5x self-verify per run

## Phase 04 - Engine, Runtime, Render, And Performance

- [#862](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/862) - render: no-code PBR material editor (metalness/roughness/albedo/normal/emissive) + live preview
- [#882](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/882) - render: text/label/floating-text/damage-number rendering
- [#1072](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1072) - [EPIC][ENGINE] Custom proprietary Rust game engine — wgpu renderer, ECS, assets, animation, physics, audio, VFX, deterministic sim (README-quality, replaces g3n)
- [#1085](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1085) - [EPIC][ENGINE] Renderer (wgpu) — Forward+ PBR, shadows, post-processing, particles, stylized shaders (README-quality)
- [#1098](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1098) - [EPIC][ENGINE] Asset pipeline — glTF/textures/audio import, content-addressed store, hot-reload, LOD
- [#1146](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1146) - [EPIC][ENGINE] Quality, performance & FULLY-OPERATIONAL acceptance — README-quality, FSV-in-reality
- [#1152](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1152) - engine/render: temporal upscaling + frame-gen (OPTIONAL, later) — DLSS 4 on the 5090 (dlss_wgpu/Vulkan) + open-source FSR/NIS/DLAA cross-vendor
- [#1461](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1461) - [P1][PERF][EDITOR] UI thread: blocking Calyx TCP probe + O(project) index rebuild + trigger-graph RON serialize + asset clones, all per frame; McpServer re-registered per call
- [#1470](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1470) - [P1][PERF][FSV] Validate a packaged game on real $500-$1,000 hardware — #1153 closed with 5090-only evidence; the DUAL target (#928) has never been tested on a min-spec machine
- [#1480](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1480) - [P2][PERF][DEBT] EngineError (112 String variants) returned by value at ~4,100 call sites incl. per-tick AI paths; font_sdf.rs compiled as two modules; default builds skip all CUDA kernels
- [#1481](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1481) - [P2][BUG][RENDER] Max-length shaped text run (4096 chars) fails to persist: write_shaped_text_json exceeds Calyx 1 MiB wide-column cap
- [#1482](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1482) - [ENGINE] Procedural geometry generation (wgpu-native meshes/doodads/terrain — no external DCC pipeline)
- [#1485](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1485) - [P1][PERF][EDITOR] Editor per-frame waste #1461 missed: full GPU drain + sleep-paced UI thread + per-frame render-evidence/info/inspector rebuilds (litd-game got these in #1460)

## Phase 05 - Editor, WC3 Parity, And Script Surface

- [#801](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/801) - [EPIC] WC3-parity: Terrain & World building (tiles, cliffs, water, doodads, destructibles, pathing, regions, cameras, weather)
- [#802](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/802) - [EPIC] WC3-parity: Object/Game-data Editor (units, abilities, heroes, items, shops, buffs, upgrades, tech tree, gameplay constants)
- [#803](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/803) - [EPIC] WC3-parity: Trigger / ECA system (events, conditions, actions, variables, Lua escape) — GUI no-code
- [#804](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/804) - [EPIC] WC3-parity: Runtime game systems (cinematics, dialogs, quests, leaderboards, multiboards, timers, fx, fog, weather, save/load)
- [#805](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/805) - [EPIC] WC3-parity: Play, AI opponents & distribute (campaign editor, computer AI, lobby, replays, templates, file format)
- [#806](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/806) - terrain: tile/tileset palette + custom tilesets
- [#807](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/807) - terrain: cliffs, ramps, height layers + runtime deformation
- [#808](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/808) - terrain: water (deep/shallow) + map boundary/environment
- [#809](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/809) - terrain: doodad placement brush (scenery) — variation/scale/rotation/density
- [#810](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/810) - terrain: destructibles (trees/gates/walls/breakables) placement + data
- [#811](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/811) - terrain: pathing (blockers, walkable bridges, custom pathing maps)
- [#812](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/812) - terrain: regions/rects (named areas) + enter/leave hooks
- [#813](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/813) - terrain: cameras (placement, named cameras, top-down default + cinematic)
- [#814](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/814) - terrain: lighting + day/night cycle
- [#815](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/815) - terrain: weather effects + global fog
- [#816](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/816) - terrain: minimap generation + loading screen + map preview
- [#817](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/817) - terrain: starting locations / player start points
- [#818](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/818) - object: unit data editor (stats/art/sound/combat/movement/techtree fields)
- [#819](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/819) - object: ability system (passive/active/autocast + base-ability templating)
- [#820](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/820) - object: hero system (STR/AGI/INT, XP/leveling, hero abilities, revive)
- [#821](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/821) - object: items + inventory + charges + item abilities
- [#822](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/822) - object: item recipes + shops/marketplaces (buy/sell/stock/drop tables)
- [#823](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/823) - object: buffs/effects editor
- [#824](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/824) - object: upgrades (stat upgrades + requirements)
- [#825](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/825) - object: tech tree / requirements / dependency-equivalents / build prerequisites
- [#826](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/826) - object: destructibles & doodads data editors
- [#827](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/827) - object: Gameplay Constants editor (XP curve, day length, hero defaults, drop rates)
- [#828](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/828) - object: Game Interface editor (UI text/colors/icons/messages/command-card)
- [#829](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/829) - object: raw-data view + copy-from-base + Lower* validation (lowering valve)
- [#833](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/833) - trigger: typed variables + arrays + variable editor
- [#834](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/834) - trigger: Lua custom-script escape + GUI<->Lua convert (lowering-safe)
- [#835](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/835) - trigger: organization (folders, enable/disable, initially-on, comments) + Object Manager cross-ref
- [#836](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/836) - game: players/forces/teams + alliances + shared vision/control
- [#837](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/837) - game: resources (gold/lumber/custom) + upkeep
- [#838](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/838) - game: victory/defeat conditions + game-over screen
- [#839](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/839) - game: quests + quest log
- [#840](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/840) - game: leaderboards + multiboards
- [#841](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/841) - game: timers + timer windows
- [#842](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/842) - game: dialogs + buttons + UI frames
- [#843](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/843) - game: floating text + text tags
- [#844](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/844) - game: special effects (attach models, point/unit fx)
- [#845](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/845) - game: cinematics (letterbox, transmissions, fades, subtitles, voice, camera sequences)
- [#846](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/846) - game: runtime cameras (apply/pan/rotate/lock; top-down default, first-person option)
- [#847](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/847) - game: fog of war / visibility modifiers
- [#848](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/848) - game: day/night + weather at runtime
- [#850](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/850) - game: save/load persistence (game-cache/hashtable equivalent)
- [#851](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/851) - game: unit groups / player groups / selection + order systems
- [#853](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/853) - play: single-player campaign editor (multi-map, shared object data, chapter flow)
- [#1489](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1489) - [P1][BUG][EDITOR] Editor command errors are log-only + mutate-then-save desync — UI shows changes that never hit disk, no user-visible error

## Phase 06 - UX, Onboarding, And Creator Loop

- [#711](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/711) - infra: Windows installer for game + editor (single package, code-signed, auto-update)
- [#898](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/898) - [EPIC] End-user experience & onboarding — WC3-simple, fun-first, the most approachable builder ever
- [#899](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/899) - ux: first-launch onboarding — pick a template, make one change, see it live, in under 5 minutes
- [#900](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/900) - ux: ONE help path — context-aware AI assistant ('How do I…?') powered by Calyx, no menu-of-help
- [#901](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/901) - ux: template gallery — opinionated starters to copy AND dissect (melee/MOBA/TD/ORPG/arena/survival/story)
- [#902](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/902) - ux: live-preview everywhere — instant author->see for materials/abilities/dialogue/cinematics/map
- [#903](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/903) - ux: reduce-text/add-visuals pass — icons, thumbnails, visual pickers over walls of text; kill jargon
- [#904](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/904) - ux: accessibility — color-contrast, scalable text, remappable input, colorblind-safe palettes
- [#905](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/905) - ux: user-journey audit harness — first-launch->template->edit->live->character+story->playtest->share
- [#906](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/906) - ux: fun-first feedback — delight/juice in the editor itself (motion, sound, instant gratification)
- [#1043](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1043) - world: dead-simple world creation + publish-to-server — make creating and hosting a world trivial (ties UX #898)
- [#1468](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1468) - [P1][GAME] New-player tutorial and teaching layer — zero issues cover teaching players; permadeath+full-loot+RP-enforced needs an academy before first exposure

## Phase 07 - Story, Lore, And Narrative Tools

- [#886](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/886) - [EPIC] Visual storytelling & narrative — direct beautiful emotional stories no-code (staging, mood, camera, dialogue, cinematics, environmental, emergent)
- [#887](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/887) - story: visual branching-dialogue editor (nodes/choices/conditions) — writer-friendly, no code (Ink/Yarn-class)
- [#888](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/888) - story: dialogue runtime + presentation (speech bubbles/portraits/dialogue wheel, text FX, voice)
- [#889](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/889) - story: Story Solver — verify narrative logic, find soft-locks/broken quest states/unreachable branches
- [#890](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/890) - story: character & relationship system (motivation/voice/arc + parasocial bonds, NPC memory)
- [#891](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/891) - story: quest & story-arc designer (acts/beats, branching with fold-back, consequence-delay, pacing)
- [#892](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/892) - story: cinematic/cutscene authoring (camera, transmissions, fades, subtitles, voice, timeline)
- [#893](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/893) - story: environmental storytelling tools (place lore props/notes/scenes; evoked/enacted/embedded/emergent)
- [#894](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/894) - story: lore/world bible — Calyx-grounded consistency + semantic search over the story corpus
- [#895](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/895) - story: AI co-writer (NL -> draft scenes/dialogue/quests, grounded + Ward-gated, lowered to data)
- [#896](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/896) - story: branching variables/state + save-persistent choices (choices that matter across sessions)
- [#897](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/897) - story: story/visual-novel + narrative-RPG starter templates (copy-and-tell-your-story)
- [#909](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/909) - story/visual: scene staging & composition — place/pose characters+props, blocking, framing, depth (no-code, drives the Rust/wgpu engine)
- [#910](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/910) - story/visual: mood & atmosphere — pick an emotion -> lighting/time-of-day/weather/fog/color-grade (no shader knowledge)
- [#911](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/911) - story/visual: camera as director — emotional shots/framing (close-up/wide/dolly/shake), shot timing + beat pacing
- [#912](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/912) - story/visual: character expression & emotion — portraits/poses/facial/body states tied to dialogue + story state
- [#914](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/914) - story/visual: visual-novel / scene-cinematic mode — fast lane for image+text+choice visual stories, zero 3D work
- [#931](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/931) - story/quest: epic multi-stage quest & quest-chain authoring — easy to build long branching epic questlines, no code
- [#932](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/932) - story/quest: quest & story templates — copy-and-go epic quest patterns (fetch, escort, boss, mystery, multi-act campaign)
- [#1048](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1048) - [EPIC] Lore and worldbuilding system — author, connect, and surface lore everywhere (lore IS the story)

## Phase 08 - RPG Systems, Economy, And RP Enforcement

- [#933](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/933) - [EPIC] Ultimate RPG creator — flagship specialization (classes, talents, itemization, factions, party, persistent ORPG) + full WC3 genre coverage
- [#934](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/934) - rpg: class & multi-class system — classes grant stats + skills; choose at creation, advance/respec
- [#935](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/935) - rpg: skill/talent trees — spend points per level; active/passive/upgrade nodes, prerequisites
- [#936](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/936) - rpg: equipment slots + set bonuses + durability (gear system)
- [#937](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/937) - rpg: loot tables & ARPG itemization — weighted/nested drops, rarity tiers, randomized affixes
- [#938](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/938) - rpg: factions & reputation system — player<->faction, NPC<->faction, alliances/rivalries, reputation-gated content
- [#939](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/939) - rpg: party & companion + summon/minion system — roster, companions that level/gear/skill, temporary minions
- [#940](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/940) - rpg: character creation — appearance/class/starting stats/background
- [#941](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/941) - rpg: interactables — chests/doors/levers/signs/containers + ownership & stealing
- [#942](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/942) - rpg: bestiary/codex + world flags/counters/gates (progression state)
- [#943](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/943) - rpg: persistent ORPG character save/progression — characters persist across sessions (save files or load-codes)
- [#945](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/945) - [EPIC] MUD-grade systemic depth — Carrion Fields benchmark (races, alignment/ethos, religions, cabals+warfare, edges/flaws, permadeath, survival, zone resets, PK, RP roles, persistent world)
- [#946](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/946) - rpg/mud: races & racial traits system — configurable races with stat mods, innate abilities, racial edges, size, vision, align/class restrictions
- [#947](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/947) - rpg/mud: alignment x ethos system — Good/Neutral/Evil x Lawful/Neutral/Chaotic, gating classes/factions/religions/abilities, drift from actions
- [#948](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/948) - rpg/mud: religions & deities — domains, worship/prayer, divine favor, priest empowerment (RP-gated communes/supplications)
- [#949](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/949) - rpg/mud: cabals/clans/guilds + items-of-power & raiding warfare — joinable factions with hideouts, power items, steal/strip mechanics, membership rules
- [#950](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/950) - rpg/mud: edges & flaws — perk/drawback system (general/racial/class), trade-offs chosen at creation/advancement
- [#951](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/951) - rpg/mud: class specializations & legacies — mid/high-level branching capstones (e.g. weapon spec, signature techniques)
- [#952](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/952) - rpg/mud: permadeath, risk, corpse-looting & aging/lifespan — configurable death consequences and character mortality
- [#953](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/953) - rpg/mud: survival needs — hunger/thirst/fatigue/encumbrance (+ temperature), sustenance items, camping/rest
- [#954](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/954) - rpg/mud: zone/area world model + resets/repop — areas of regions/rooms, timed mob/item resets, instancing
- [#955](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/955) - rpg/mud: PK/PvP rules & flagging — open/consensual/factional PvP, level ranges, safe zones, justice/bounty
- [#956](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/956) - rpg/mud: RP roles + RP-enforced progression + GM/Immortal live tools (human + Calyx-agent immortals)
- [#957](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/957) - [DECISION] rpg/mud: persistent authoritative online-world mode (always-on ORPG/MUD) vs lockstep match netplay
- [#958](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/958) - rpg/mud: social layer — emotes/socials + chat channels (say/whisper/faction/global/newbie)
- [#972](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/972) - [EPIC] Scarcity economy & full-loot PvP — finite-quantity rarity tiers, source-bound drops, full looting, anti-hoard decay, powerful perks
- [#973](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/973) - econ: item rarity tiers + GLOBAL scarcity caps — common(unlimited)/rare(~20-30)/epic(~10)/mythic(~5)/legendary(~1-2), world-quantity ledger
- [#974](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/974) - econ: source-bound finite drops — a capped item will NOT respawn on its source mob while an instance is held by a player
- [#975](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/975) - econ: full-loot PvP on death — killing a player drops/transfers ALL carried loot (corpse looting); configurable rules
- [#976](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/976) - econ: common-item re-gearing — unlimited common items so a player can always re-equip after a full-loot death
- [#977](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/977) - econ: powerful scarce-item perks + power-scaling — scarce items grant strong abilities (stone skin etc.); stacking many = insane power (and a big target)
- [#978](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/978) - econ: inactivity decay & anti-hoarding (MP, 7-day) — after 1 week offline, a character's rare+ items vanish and return to the world pool
- [#979](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/979) - rp: immortal RP-reward system — immortals reward good roleplay with tattoos, boons, titles, ultra/super powers (tracked, revocable)
- [#981](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/981) - econ: carrying capacity / encumbrance integration — weight limits gate what you can carry and loot
- [#996](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/996) - rp: bad-roleplay PUNISHMENT system — demotion, rank/power loss, faction expulsion, immortal sanction for breaking your role/theme
- [#997](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/997) - social: player relationship graph — friends, enemies, rivalries, parties, alliances (player + faction politics)
- [#998](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/998) - faction: ambition & LEADERSHIP — climb to LEAD your faction (become the Emperor, Scion High Chancellor, Moran high priest)

## Phase 09 - Content, Assets, And Audio Buildout

- [#782](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/782) - [EPIC] AI asset-generation pipeline — text/art → 3D → rig → animate → render-budget → Calyx-ingested
- [#789](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/789) - assets: 2D concept-art generation (Gemini via Claude image skill) -> versioned intake + provenance
- [#790](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/790) - assets: 2D -> 3D mesh generation integration (+ QA/retry for high-detail failures)
- [#791](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/791) - assets: auto-texture / PBR material the generated mesh
- [#792](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/792) - assets: auto-rig + skeletal animation generation (idle/move/attack/death) [OPEN PROBLEM]
- [#793](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/793) - assets: Blender automation via Synapse (cleanup/decimate/rig/export GLB by computer control)
- [#794](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/794) - assets: render conformance — LOD/atlas/PBR-map tuning to the Rust/wgpu engine on the RTX 5090 (max fidelity, real-time)
- [#795](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/795) - assets: ingest finished assets into Calyx (ingest_media + TCT-cosine dedup + provenance)
- [#796](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/796) - test: FSV — text -> animated 3D unit -> import by hash -> renders + animates deterministically in-game
- [#984](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/984) - [EPIC] Carrion Fields full content build — generate ALL races, classes, worlds, abilities, items, mobs, quests, factions (text MUD -> living Calyx world)
- [#985](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/985) - content: generate race specs — Ashen Reach canon names + Rust/wgpu models per race
- [#986](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/986) - content: generate all 17 classes — full skill/spell/supplication/song/power lists + specializations/legacies/paths/forms
- [#987](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/987) - content: build the world — 250+ areas as top-down visual regions (towns/wilderness/dungeons/lairs/cabal HQs) + connections, resets, recall
- [#988](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/988) - content: generate all abilities/spells/supplications/songs/cabal-powers — data + Rust/wgpu VFX for each
- [#989](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/989) - content: generate the full item set — common->legendary with scarcity caps + perks + Rust/wgpu visual models
- [#990](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/990) - content: build all 7 cabals + religions/deities — ideologies, ranks, powers, hideouts, items-of-power, AI leaders
- [#991](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/991) - content: generate all mobs/creatures/bosses — stats + Rust/wgpu models + Calyx AI agents (incl Ancient Dragons, Tiamat-class)
- [#992](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/992) - content: author all quests (300+) — questlines, objectives, rewards, puzzles
- [#993](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/993) - content: the AI Immortal pantheon — Calyx agents embodying the gods/staff running the live world (events, empowerment, RP rewards)
- [#1000](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1000) - content/race: Elf — full build spec (stat caps, restrictions, innate abilities, model)
- [#1001](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1001) - content/race: Half-Elf — full build spec (stat caps, restrictions, innate abilities, model)
- [#1002](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1002) - content/race: Ashkin mixed-line — full build spec (stat caps, restrictions, innate abilities, model)
- [#1003](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1003) - content/race: Ashkin deep-line — full build spec (stat caps, restrictions, innate abilities, model)
- [#1004](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1004) - content/race: Wood-Elf — full build spec (stat caps, restrictions, innate abilities, model)
- [#1005](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1005) - content/race: Dwarf — full build spec (stat caps, restrictions, innate abilities, model)
- [#1006](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1006) - content/race: Duergar — full build spec (stat caps, restrictions, innate abilities, model)
- [#1007](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1007) - content/race: Gnome — full build spec (stat caps, restrictions, innate abilities, model)
- [#1008](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1008) - content/race: Svirfnebli — full build spec (stat caps, restrictions, innate abilities, model)
- [#1009](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1009) - content/race: Orc — full build spec (stat caps, restrictions, innate abilities, model)
- [#1010](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1010) - content/race: Goblin — full build spec (stat caps, restrictions, innate abilities, model)
- [#1011](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1011) - content/race: Minotaur — full build spec (stat caps, restrictions, innate abilities, model)
- [#1012](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1012) - content/race: Storm Giant — full build spec (stat caps, restrictions, innate abilities, model)
- [#1013](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1013) - content/race: Fire Giant — full build spec (stat caps, restrictions, innate abilities, model)
- [#1014](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1014) - content/race: Frost Giant — full build spec (stat caps, restrictions, innate abilities, model)
- [#1015](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1015) - content/race: Cloud Giant — full build spec (stat caps, restrictions, innate abilities, model)
- [#1016](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1016) - content/race: Sylphkin — full build spec (stat caps, restrictions, innate abilities, model)
- [#1017](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1017) - content/race: Felisar — full build spec (stat caps, restrictions, innate abilities, model)
- [#1018](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1018) - content/race: Saur — full build spec (stat caps, restrictions, innate abilities, model)
- [#1019](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1019) - content/race: Centaur — full build spec (stat caps, restrictions, innate abilities, model)
- [#1020](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1020) - content/class: Warrior — full build spec (abilities by level, specializations, restrictions)
- [#1021](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1021) - content/class: Thief — full build spec (abilities by level, specializations, restrictions)
- [#1022](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1022) - content/class: Assassin — full build spec (abilities by level, specializations, restrictions)
- [#1023](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1023) - content/class: Ranger — full build spec (abilities by level, specializations, restrictions)
- [#1024](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1024) - content/class: Bard — full build spec (abilities by level, specializations, restrictions)
- [#1025](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1025) - content/class: Berserker — full build spec (abilities by level, specializations, restrictions)
- [#1026](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1026) - content/class: Raider — full build spec (abilities by level, specializations, restrictions)
- [#1027](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1027) - content/class: Paladin — full build spec (abilities by level, specializations, restrictions)
- [#1028](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1028) - content/class: Anti-Paladin — full build spec (abilities by level, specializations, restrictions)
- [#1029](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1029) - content/class: Shaman — full build spec (abilities by level, specializations, restrictions)
- [#1030](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1030) - content/class: Healer — full build spec (abilities by level, specializations, restrictions)
- [#1031](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1031) - content/class: Druid — full build spec (abilities by level, specializations, restrictions)
- [#1032](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1032) - content/class: Invoker — full build spec (abilities by level, specializations, restrictions)
- [#1033](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1033) - content/class: Transmuter — full build spec (abilities by level, specializations, restrictions)
- [#1034](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1034) - content/class: Conjurer — full build spec (abilities by level, specializations, restrictions)
- [#1035](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1035) - content/class: Necromancer — full build spec (abilities by level, specializations, restrictions)
- [#1036](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1036) - content/class: Shapeshifter — full build spec (abilities by level, specializations, restrictions)
- [#1469](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1469) - [P1][CONTENT][AUDIO] Ashen Reach audio content (music/ambience/SFX) has no tracked task; asset-gen pipeline (#782) has no audio stage — 250+ areas are silent

## Phase 10 - Multiplayer, Servers, Hub, And Ecosystem

- [#83](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/83) - net: replay viewer (pause/speed/free-camera/perspective)
- [#85](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/85) - net: netplay FSV — 2-machine LAN match, identical final hashes
- [#1037](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1037) - [EPIC] Self-hosted servers and dead-simple multiplayer — player-run 200-500p worlds, connect via localhost/SSH/IP, admin moderation + per-server RP policy
- [#1044](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1044) - ops: server performance for 200-500 players — authoritative sim + agent budget on the host machine (no $500 limits, max the box)
- [#1155](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1155) - [EPIC] Game ecosystem (Battle.net-style) — publish/share/discover/play each others games; hub launcher, matchmaking, profiles, ladders
- [#1471](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1471) - [P1][SERVER] Persistent-world backup/snapshot/restore + scarcity-ledger integrity audit — unrecoverable world loss is untracked for player-run servers
- [#1472](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1472) - [P1][NET][ECON] Anti-cheat and economy-integrity enforcement: item-dupe detection against the global scarcity ledger, server-side input hardening, lockstep-hash divergence response
- [#1473](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1473) - [P2][ECOSYSTEM] Trust and safety for published games — report/takedown, package scanning at publish+install, publisher identity (orphaned by #754 not-planned closure)

## Phase 11 - Packaging, Release, And Final Gates

- [#994](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/994) - content: full-world FSV playtest — a complete CF-equivalent world playable end-to-end, single-player, local
- [#1150](https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1150) - engine: FULLY-OPERATIONAL acceptance — Editor (on Calyx) builds + runs Ashen Reach end-to-end, FSV-in-reality


