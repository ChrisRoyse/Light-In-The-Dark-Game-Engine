# Light in the Dark — Expanded PRD

This directory expands the master [PRD](../PRD.md) into detailed specifications. The master PRD is the source of truth; these documents elaborate, never contradict. Requirement IDs (R-SIM-*, R-RND-*, R-GC-*, R-EXEC-*, R-FSV-*, …) are defined in the master PRD and expanded here.

**Product purpose (PRD §1.0):** a world-building and idea-explanation platform — beginner-simple authoring surface over the complete, untruncated WC3 API capability set, authorable by humans and AI coding agents alike.

**Verification:** all milestone/task acceptance follows the Full State Verification protocol ([`prompts/fsv.md`](../../prompts/fsv.md), PRD §5.5) — evidence from the source of truth (screenshots, state dumps, logs), never return values alone.

## Index

### 01 — Vision
- [Overview](01-vision/overview.md) — what the engine is/isn't, target users, source-material clarification
- [Goals and Non-Goals](01-vision/goals-and-non-goals.md) — G1–G7 with measurable success criteria
- [Risks and Open Questions](01-vision/risks-and-open-questions.md) — detection signals, trigger points, recommended defaults
- [Decisions](01-vision/decisions.md) — dated decision record (2026-06-11: Q1–Q4 settled; multiplayer committed as lockstep M7)

### 02 — Research
- [Rendering Dimensionality](02-research/rendering-dimensionality.md) — 2D vs 2.5D vs low-poly 3D; memory math; decision
- [Model Format Selection](02-research/model-format-selection.md) — glTF/GLB core profile; G3N loader gaps; fallback plan
- [Asset Sources](02-research/asset-sources.md) — CC0 pack inventory, license verification, gap analysis
- [G3N Evaluation](02-research/g3n-evaluation.md) — source-grounded engine capability audit; RTS gaps; fork strategy

### 03 — Public API
- [Architecture](03-api/architecture.md) — api/sim/render/asset layering, import rules, headless mode
- [Deduplication Policy](03-api/deduplication-policy.md) — D1–D5 rules with worked JASS examples; audit report
- [Public API Design](03-api/public-api-design.md) — the ~20 public types; R-API-1..6; options structs
- [Execution Model](03-api/execution-model.md) — deterministic cooperative scheduler; events; waits; AI isolation
- [Naming and Style](03-api/naming-and-style.md) — Go conventions, JASS→Go mapping table, versioning
- [JASS API Category Mapping](03-api/jass-mapping/README.md) — all 2,521 functions across 18 categories

### 04 — Simulation
- [Determinism](04-simulation/determinism.md) — fixed-point vs float; Go hazards; M1 spike; state hashing
- [ECS Architecture](04-simulation/ecs-architecture.md) — SoA stores, fixed capacity, entity IDs, zero-alloc iteration
- [Tick and Scheduler](04-simulation/tick-and-scheduler.md) — 20 Hz loop, render interpolation, coroutine scheduler
- [Pathfinding](04-simulation/pathfinding.md) — pathing grid, A*/HPA*, deterministic tie-breaking
- [Combat and Orders](04-simulation/combat-and-orders.md) — order system, attack cycle, damage tables

### 05 — Rendering
- [Camera and Culling](05-rendering/camera-and-culling.md) — locked RTS camera, frustum tuning
- [Batching and Draw Calls](05-rendering/batching-and-draw-calls.md) — ≤300-call budget, instancing plan
- [Materials and Lighting](05-rendering/materials-and-lighting.md) — atlas strategy, presets, light caps
- [Fog of War, Minimap, Selection](05-rendering/fog-of-war-minimap-selection.md) — custom render features
- [Terrain](05-rendering/terrain.md) — heightmap vs tiles analysis and recommendation

### 06 — Assets
- [Pipeline](06-assets/pipeline.md) — CC0 source → Blender normalization → core-GLB → validation
- [Validation and Data](06-assets/validation-and-data.md) — assetcheck CLI, game-data tables

### 07 — Platform
- [Audio](07-platform/audio.md) — OpenAL, .ogg policy, voice budget
- [UI and HUD](07-platform/ui-and-hud.md) — WC3 HUD layout on G3N widgets
- [Input](07-platform/input.md) — selection, control groups, hotkeys, command-stream encoding

### 08 — Performance
- [Budgets and Benchmarks](08-performance/budgets-and-benchmarks.md) — methodology, CI harness, reference machine
- [GC Discipline](08-performance/gc-discipline.md) — zero-alloc hot paths, hazard catalog, pooling

### 09 — Roadmap
- [Milestones](09-roadmap/milestones.md) — M0–M6 detail (see master PRD §7 for M0.5 "First Light" demo)
- [Tooling](09-roadmap/tooling.md) — jassgen, api-manifest.json schema, assetcheck, CI benchmarks
