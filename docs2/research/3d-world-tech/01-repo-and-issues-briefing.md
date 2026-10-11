# Briefing: State & Direction -- Light in the Dark Game Engine

**Research date: 2026-07-12. Source: codebase review + 363 open GitHub issues.**

## 0. What the repo is at a glance

A **proprietary, no-code, AI-native game engine written in Rust**, plus its two products: the
**Editor** (a Warcraft-III-World-Editor-class creation tool) and **Ashen Reach** (an original
multiplayer, full-loot, roleplay-enforced RPG built *with* the Editor as the proof the tool
works). The entire thing is built **on Calyx**, a vendored grounded-AI substrate that is the
backend/knowledge layer. The README is explicitly the *vision*; the authoritative state is
**363 open GitHub issues** (a "living backlog of 400+ atomic tasks").

## 1. Architecture & workspace layout

**Two Cargo workspaces.** Root workspace (`crates/*`, resolver 3, edition 2024,
`LicenseRef-Proprietary`) excludes `Calyx/`, which is its own workspace.

Root crates:

- `litd-engine` -- the reusable engine library (render/sim/assets/anim/physics/audio/VFX/AI).
  ~596 `.rs` files.
- `litd-editor` -- the product built on the engine (egui shell + WC3-style no-code modules);
  also an **MCP server**. ~624 `.rs` files.
- `litd-editor-mcp` -- MCP surface for the editor.
- `litd-game` -- the runtime host that plays a packaged `.litdgame` (main.rs, runtime_host,
  runtime_renderer, runtime_state, runtime_audio). Small (~9 top-level files) -- the game is
  authored data, not code.
- `litd-hash` -- content hashing.

**Calyx** (`Calyx/crates/calyx-*`, ~26 crates) is the AI/data substrate: `aster`
(storage/vault = the engine DB), `forge` (math/CUDA), `registry` (multimodal lenses),
`sextant` (search), `loom` (associations/TCT), `assay` (measured "signal bits" -> grounded
balance/analytics), `lodestar` (kernel / AGI-search), `oracle` (prediction/recurrence
forecasting), `ward` (out-of-distribution guard, fail-closed AI gate), `ledger` (provenance),
`anneal` (self-optimization), `mcp`/`web-api`, etc. Extensive design corpus in
`Calyx/docs/dbprdplans/` (31 numbered PRDs) and `Calyx/docs/implementation/` (staged build).

**Dependency direction (binding, doctrine #1071):** game -> editor -> engine -> Calyx ->
permissive crates. Never reversed. Editor and game are **strictly separate**; the Editor is
built FIRST; Ashen Reach is built entirely *within* the Editor (no engine-side game code).

**Rendering stack:** `wgpu = 27`, `winit 0.30`, `glam 0.29`, `egui 0.33`, `gltf 1.4`,
`rapier3d 0.34`, `kira` audio, `meshopt`, `basis-universal`/`ktx2` (texture compression),
`wasmtime`/`wasmparser` (the trigger/ECA "WASM escape" hatch). **Permissive-only license
policy (#1073): no GPL/copyleft.** Determinism preserved via fixed-point sim and a "lowering
valve" (#725) -- the hot sim tick and render frame consume Calyx-**lowered**, frozen,
fingerprinted data and never block on live Calyx calls.

**Telemetry / balance loop (Assay/Lodestar/Oracle):** Assay produces measured signal bits
from matches/replays; Lodestar is the kernel that answers over the corpus; Oracle forecasts
(balance what-if, crash/desync recurrence). This is the intended offline replay +
balance-tuning loop (issues #907/#1680).

**ECS/scene model:** custom archetype ECS per doctrine; scene/world content lives in Calyx
(Aster vault) as "constellations," lowered to frozen runtime formats.

## 2. Current 3D rendering capability (from source, not issues)

The renderer is **substantially built and real** (actual wgpu passes with FSV readback), not
a stub. The `crates/litd-engine/src/render/` tree is large and organized around a first-slice
`render/mod.rs` ("create/configure a wgpu surface, clear, present, with source-of-truth stats
for FSV readback") plus these implemented subsystems, each with a `*_gpu_wgpu.rs` (real GPU)
+ image-readback + tests:

- **PBR materials** (`material.rs`, `material_pbr.wgsl`): metallic-roughness,
  Cook-Torrance/GGX; texture slots BaseColor/MetallicRoughness/Normal/AO/Emissive/Irradiance/
  PrefilteredEnv/BrdfLut (i.e., IBL slots defined).
- **Lighting** (`lighting.rs`, `lighting_forward_plus.wgsl`): **Forward+/clustered** light
  assignment, ambient/lighting rig, **day-night cycle + weather lighting**
  (`lighting_daynight.rs`).
- **Shadows** (`shadow.rs`, `shadow.wgsl`): directional **cascaded shadow maps (CSM)**,
  **point cube shadows**, spot shadows -- with GPU render + tile readback.
- **Post** (`post.rs`, `post_gpu_chain.rs`, `post.wgsl`): HDR/tonemap/bloom/color-grade
  chain.
- **Batching/culling** (`batching.rs`, `batching_culling.wgsl`): instancing, frustum +
  **occlusion culling via an HZB/occlusion grid** (`occlusion_grid.rs`), deterministic
  material batching.
- **Stylized shader library** (`stylized.rs`, `stylized_effects.wgsl`): no-code preset
  library over a WGSL "shader system" with variant axes (`shader.rs`, `shader_system.wgsl`).
- **VFX/particles** (`render/vfx/`, `gpu_particles.wgsl`): GPU particles, cues, light atlas,
  workload budgeting.
- **Animation** (`animation/`, `skinning.wgsl`, `morphing.wgsl`): skeletal skinning (GPU),
  morph targets, blend trees, state machines, root motion, montages, gltf loader.
- **Sprites/billboards, screen-space effects (SSAO-class), primitives** (`primitive_mesh.rs`
  -- parametric mesh gen), **camera** incl. **cinematic camera**, **fonts** (SDF, rustybuzz
  shaping), and **UI/HUD/minimap/fog** (`ui_hud_minimap_fog.wgsl`).
- **Editor world viewport** (`editor_world.rs`): "real wgpu mesh/depth/fog pass for the
  center editor viewport" -- egui only composes chrome on top; it does not paint the world.
  Includes foliage instancing, layers, a reference scene, and visual presets.

**Quality scaling exists in code:** `performance_min_spec.rs` and `performance_quality.rs`
(+ tests) in the engine, and `runtime_renderer.rs`/`runtime_state.rs` in the game consume
tier settings. There is a **per-frame allocation budget** measured live on the 5090 (131,072
alloc/frame ceiling, ~4x observed peak) and a **perf overlay** (`ui_perf_overlay.rs`).

**What is NOT yet a full system:** a **terrain/world runtime** is only partly there. The
editor has terrain *authoring* (`terrain_advanced_tools.rs`, heightmap/pathing/foliage-brush
FSV examples, `asset_scene_composition_terrain.rs`), but the terrain **render** children of
epic #801 (tiles/cliffs/water/doodads/destructibles/pathing/regions/cameras/weather/
lighting-day-night/minimap/start-locations, #806-#817) are **all open**, and **procedural
geometry generation (#1482)** -- heightfield->mesh with per-tile LOD and splat -- is open.
Approximate style is **"forward" (Forward+/clustered)**, not deferred. Upscaling
(DLSS/FSR/TAA, #1152/#1091) is **not built** (explicitly "optional, later," needs motion
vectors + jitter first).

**Shader tech:** all WGSL, all first-party (16 `.wgsl` files). No external shader language or
DCC pipeline dependency.

## 3. Performance budget -- the DUAL target (issue #928)

**#928 is the binding budget doctrine, "create high, play low" (Battle.net model):**

| Machine | Target |
|---|---|
| **Creator / Editor** (dev box) | AMD Ryzen 9 9950X3D (16C/32T, AVX-512) - **RTX 5090 32 GB** - 128 GB DDR5 - Win 11. "Max it out" -- push the 5090 to its real-time ceiling at 1440p/4K; full lens roster resident for Calyx. |
| **Player / produced GAMES** (min-spec) | **~60 fps @ 1080p** on a **$500-$1,000 PC**: Ryzen 5 / **RX 6600 or GTX 1660-class** / 16 GB, scaling up to ultra on a 5090. |

Visual-fidelity standard is explicitly **best-in-class STYLIZED real-time PBR** (reference:
Sunderfolk, WC3 Reforged, League of Legends, Wayfinder, Ori) -- **NOT** Unreal-5
Nanite/Lumen/path-traced photorealism; marketing/README must not imply photoreal. The engine
must ship a **low->ultra graphics quality scale**; the stylized look must survive at the low
end.

**Critical open gap -- #1470 (P1 test-gap):** the DUAL target has **never been validated on
real min-spec hardware**. The environment has exactly one machine (the 5090 workstation);
#1153 (min-spec quality scaling) was closed COMPLETED but on 5090-only,
quality-scale-simulated evidence. Per the FSV-in-reality culture, that is "a hypothesis, not
evidence." #1470 requires acquiring a GTX1660/RX6600-class box and capturing p95 <= 16.6 ms @
1080p frame-time artifacts. **Any new rendering tech must be provable on that class of GPU,
not just the 5090.**

## 4. Open-issue landscape (363 open; sampled 300)

**Label distribution (top):** `agent:claude` 174 - `area:editor` 138 - `priority:p1` 107 -
`priority:p2` 57 - `area:ai` 29 - `area:game` 21 - **`area:render` 16** - `type:epic` 14 -
`priority:p0` 13 - `area:ui` 11 - `type:doctrine` 10. So the backlog is **editor- and
content-heavy**, with rendering a focused minority but foundational (both p0 engine epics
carry `area:render`).

**Major epics (14):** the master platform epic **#691** (no-code creation platform + UGC
marketplace); the engine mega-epic **#1072** (P0, "build-priority #1"); renderer **#1085**;
asset pipeline **#1098**; quality/acceptance **#1146**; the WC3-parity set **#801-#805**
(terrain, object editor, trigger/ECA, runtime systems, play/AI/distribute); content/RPG epics
**#984** (full CF content build), **#933** (ultimate RPG creator), **#945** (MUD-grade
depth), **#972** (scarcity economy/full-loot PvP), **#1037** (self-hosted 200-500p
multiplayer), **#1048** (lore), **#886** (visual storytelling), **#898** (onboarding/UX),
**#1155** (Battle.net-style ecosystem), plus new **#1597** (stealth/perception) and **#1598**
(World of Thera mirror).

**Themes:**

- **World/content authoring for Thera (dominant):** #1598 epic + ~60 `world:` children
  (#1632-#1671) -- mirror all ~250 CF areas / 23k rooms, the road spine, 8 cities, Underdark,
  seas, planes, dragon lairs, leveling ladders (good/evil/newbie), resets, PK geography, plus
  a renaming ("originalization") ledger.
- **Stealth/perception (#1597):** terrain+light tagging of geometry (#1601), footstep audio
  propagation (#1602), hide/invis/assassin/druid-treeform kits, concealment VFX (#1627) -- a
  whole substrate.
- **RPG systems:** combat core (#1673), vitals/movement (#1679), casting grammar (#1695),
  item data model/flags/consumables (#1682/#1685/#1693), balance framework (#1680),
  edges/flaws, zone resets.
- **Rendering/graphics direction (the key ones):**
  - **#1085** renderer epic -- Forward+ PBR, CSM, post, particles, stylized shaders, GPU
    culling, top-down/iso camera, UI/HUD/minimap.
  - **#1072 / #1071** -- the "build our own Rust/wgpu engine, DROP g3n" mega-epic + doctrine
    (g3n and the legacy Go engine were deleted 2026-06-26).
  - **#1482** procedural geometry (wgpu-native meshes/doodads/terrain, no external DCC).
  - **#1098** asset pipeline (glTF/textures/audio, content-addressed, hot-reload, **LOD**).
  - **#1152** temporal upscaling/frame-gen (DLSS4 opt-in + open-source FSR/NIS/DLAA) --
    optional, needs motion-vectors/TAA first.
  - **#928** DUAL budget; **#1470** min-spec real-hardware validation gap; **#1146**
    quality/acceptance.
  - **#801** terrain epic + #806-#817 terrain children (all open); **#862** no-code PBR
    material editor; **#882** text/label/damage-number rendering; **#944/ADR-944** "acquired
    maps only" (no built-in minimap in shipped RPG runtime).
- **Perf/tech-debt:** #1461/#1485 (per-frame editor waste -- Calyx TCP probe, index rebuilds,
  RON serialize per frame), #1466 (files >= 500 lines violating the ratchet), #1707
  (repo-wide search timeouts from 645k artifact files).

**Doctrine issues (the "constitution"):** #800 (engineering standards -- FSV-against-reality,
no-workarounds, **<500-line files**, Synapse, research-first), #1071 (Rust/wgpu engine, drop
g3n), #1166 (two fast-feedback loops: headless-verdicted agent + instant-immersive human),
#1348 (structured data is association-native -> ingest ALL engine data into Calyx, single
source of truth), #915 (the engine IS an MCP server), #884 (storytelling is #1 goal), #995
(multiplayer/PvP/full-loot/RP-enforced, the Carrion Fields model), #959 (north-star: fully
recreate Carrion Fields as proof), #1672 (MUD-to-3D translation methodology -- the canonical
room->space, combat-round->swing-timer, terrain-word->terrain-tag mapping), #1717 (ZERO paid
services -- everything must be free), #778/#777 (multi-embedder >= 10 lenses, Calyx
grounding).

## 5. Conclusions

### (a) What this project is trying to become

A **single-machine, no-code, AI-native creation platform** -- a modern reincarnation of the
Warcraft III World Editor -- where anyone (or an AI co-author driving the same tools via MCP)
can direct visually-told, emotionally-driven games, and where the world is populated by
**genuinely alive Calyx-grounded agents**. Its flagship proof and north-star is **Ashen
Reach**: an original, multiplayer (player-hosted 200-500p), full-loot, permadeath,
roleplay-enforced RPG that mirrors the systemic depth and entire world of the text MUD
*Carrion Fields*, translated faithfully into real-time 3D. The business model is **"create
high, play low"** (Battle.net): author on a workstation-class beast, distribute games
playable on a $500 PC, share/discover/matchmake in an ecosystem hub.

### (b) Where 3D rendering/world tech is vs where the issues say it must go

**Where it is:** a genuinely functional stylized-PBR **forward (Forward+/clustered)** wgpu
renderer already exists and is FSV-exercised -- PBR + IBL slots, CSM + point/spot shadows,
HDR/bloom/tonemap/color-grade post, instancing + HZB occlusion culling, GPU particles/VFX,
skeletal + morph animation, cinematic camera, SDF text, HUD/minimap, a WGSL stylized-shader
preset system, a live editor-world viewport, and a working quality-scale + per-frame budget
harness. **Where it must go:** the **world layer is the gap** -- there is no full
**terrain/heightfield runtime**, procedural geometry (#1482), per-tile terrain LOD/splat
(#801 children), water/cliffs/destructibles/weather/day-night as shipped systems, or the
~250-area Thera world graph. The renderer also lacks motion vectors/TAA and therefore any
temporal upscaling (#1152), and the **min-spec target is entirely unverified in reality
(#1470)**. Direction: finish the **terrain + procedural-geometry + asset-LOD pipeline**, wire
it to the world-mirror content epics, and prove it all at 60 fps@1080p on GTX1660/RX6600-
class hardware.

### (c) Constraints any new rendering tech must respect

1. **wgpu 27 + first-party WGSL only** -- no other GPU API or shader language; no external
   DCC/runtime dependency for geometry (#1482).
2. **Rust only**, permissive (MIT/Apache-2.0) crates only -- **no GPL/copyleft** (#1073), and
   **no paid services / no proprietary-blob hard dependency** (#1717; DLSS may only be an
   *opt-in* path, defaults must be native/open FSR).
3. **The DUAL budget (#928)** -- must scale from RTX 5090 ultra down to **$500 PC / RX 6600 /
   GTX 1660 / 16 GB at 60 fps@1080p**, stylized (not photoreal), and be **provable on real
   min-spec hardware (#1470)** -- not simulated on the 5090.
4. **Editor + game dual targets** -- engine code must serve both the live egui-composited
   editor viewport *and* the standalone `litd-game` runtime; strict layering (game -> editor
   -> engine -> Calyx, never reversed); no game code in the engine.
5. **Determinism / lowering valve** -- anything feeding the sim/render hot path must consume
   Calyx-lowered, frozen, fingerprinted, deterministically-seeded data (no wall-clock/RNG in
   lowered output); the frame must not block on live Calyx.
6. **Calyx single-source-of-truth (#1348)** -- all content/assets/meta live in the Aster vault
   as constellations; no side stores; assets content-addressed and Calyx-ingestible.
7. **FSV-in-reality culture (#800)** -- no capability counts as done without full-state
   verification against the real running app (Synapse-driven, GPU/pixel readback where
   visual), no false-greens, no workarounds.
8. **<500-line file ratchet (#800)** -- enforced by the commit gate; new render code must be
   split into small modules from the start (the existing render tree already follows the
   `foo.rs` + `foo_gpu.rs` + `foo_gpu_wgpu.rs` + `foo_tests.rs` decomposition pattern).
9. **The engine IS an MCP server (#915)** -- every new rendering/geometry capability should be
   exposed as an agent-callable MCP tool (e.g., generate-mesh).
10. **Stylized-PBR art-direction ceiling (#928/#1071)** -- target the README look (WC3
    Reforged / LoL / Ori class); do not adopt tech whose only justification is photoreal
    fidelity the budget can't sustain.
