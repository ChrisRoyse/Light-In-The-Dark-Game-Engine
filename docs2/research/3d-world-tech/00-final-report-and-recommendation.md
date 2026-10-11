# 3D World Technology: Findings & Recommendation

**Light in the Dark - Engine Technology Review - 2026-07-12**

Codebase + 363 open issues reviewed, plus web research across traditional rendering, the
triangle-splatting / Gaussian-splatting family, and diffusion world models -- all evaluated
against the binding constraint: **60 fps @ 1080p on a $500 Windows PC** (GTX 1650 / RX 6400 /
Iris Xe class), in Rust + wgpu + WGSL.

Companion documents in this folder:

- `01-repo-and-issues-briefing.md` -- state & direction of the codebase and open issues
- `02-traditional-3d-rendering-research.md` -- rasterization tech feasible on min-spec
- `03-triangle-splatting-and-gaussian-splatting-research.md` -- the "new triangles and math"
- `04-diffusion-world-models-and-neural-rendering-research.md` -- diffusion technology

---

## Bottom line

Stay the course on the rasterized stylized-PBR renderer you already have -- it is the correct
architecture for the $500-PC target -- and spend the next quarter closing the **world layer**
gap (terrain runtime, GPU-driven culling, LOD pipeline, temporal upscaling), because that is
what the Thera world-mirror epics are blocked on. The exciting new tech splits cleanly:
**triangle splatting** is a promising *capture* tool worth one contained experiment;
**diffusion world models** are datacenter-only and wrong for a deterministic multiplayer RPG --
but **generative AI as a dev-time asset pipeline is ready now** and fits the "create high,
play low" model perfectly.

## 1. Where the engine actually is

The renderer is further along than "early engine" suggests. Already implemented and
FSV-verified in `litd-engine`: Forward+/clustered lighting with day-night and weather,
Cook-Torrance PBR with IBL texture slots, cascaded + point + spot shadow maps, an HDR post
chain (bloom, tonemap, color grade), instancing with frustum + HZB occlusion culling, GPU
particles/VFX, full skeletal + morph animation, a stylized WGSL shader preset system, SDF
text, and a real wgpu editor viewport. A quality-scaling system (`performance_min_spec.rs`)
and per-frame allocation budget exist.

The open-issue landscape (363 issues, 14 epics) says the direction is content and world: the
Thera world mirror (#1598, ~60 area children), stealth/perception (#1597), RPG systems, and
the WC3-parity editor. The rendering gaps that block those are specific:

- **No terrain runtime.** Epic #801's render children (#806-#817: tiles, cliffs, water,
  doodads, destructibles, weather...) are all open; procedural geometry (#1482) is open.
  Authoring tools exist; the shipped world layer does not.
- **No motion vectors / TAA / upscaling** (#1152) -- the highest-leverage min-spec feature is
  unbuilt.
- **The $500-PC target has never been tested on real hardware** (#1470). All min-spec
  evidence to date is simulated on the RTX 5090. By the project's own FSV doctrine, the DUAL
  budget (#928) is currently a hypothesis.

Constraints any new tech must respect: wgpu 27 + first-party WGSL only, permissive licenses
only (no GPL, no paid services per #1717), deterministic sim fed by frozen Calyx-lowered
data, stylized-PBR art ceiling (explicitly *not* photoreal), <500-line files, and FSV against
the real running app.

## 2. Traditional rasterization: what 2025-26 says ships on low-end

The industry's own low-end shipping record (Overwatch 2, RE Engine, Path of Exile 2)
converges on a clear recipe, and everything in it is implementable in wgpu today.

| Technique | Verdict | Why |
|---|---|---|
| **GPU-driven culling** -- meshlet-granularity frustum culling in compute, two-phase Hi-Z occlusion, `multi_draw_indirect` | ADOPT | The "Capcom middle path": GPU-driven without full virtualization. Cheap on iGPUs; huge wins in dense cities/dungeons. wgpu supports MDI + count on DX12. Extends the HZB culling already present. |
| **Nanite-style virtual geometry** -- 64-bit-atomic visibility buffer, compute software rasterizer | SKIP | Requires 64-bit atomics absent on Intel pre-Arc iGPUs (Bevy's meshlets are Vulkan/Metal-only, not DX12); multi-ms fixed overhead eats the iGPU frame budget; multi-person-year effort still experimental in Bevy after 2+ years. Also solves a photoreal-density problem the stylized art ceiling doesn't have. |
| **Mesh shaders** | SKIP | Experimental in wgpu 28 (Vulkan-only for WGSL); absent on Iris Xe, GTX 1650. Not min-spec hardware. |
| **Baked probe GI** -- DDGI-style irradiance + visibility volumes, evaluated at half-res | ADOPT | Exactly what Overwatch 2 shipped in 2025 for hardware older than the min-spec (~0.5-1 ms). Bake day/night states offline. Bonus: the probe grid doubles as a queryable "light level at position" for stealth AI (#1597/#1601) essentially for free. |
| **Radiance Cascades** (Sannikov / PoE2) | WATCH | Constant-cost, no-RT-hardware GI with the right philosophy; but 3D world-space RC is still research-grade (Sannikov's sparse-3D demo is Nov 2025). Track as a v2 replacement for the bake. |
| **CDLOD / GPU-driven quadtree terrain** + chunked-clipmap streaming, geomorphing, vertical-error LOD | ADOPT | Modern consensus for huge worlds on weak GPUs; constant vertex budget regardless of world size. A Rust/wgpu-native blueprint exists (`bevy_terrain` / the Kuhnert UDLOD thesis). Directly answers epic #801 + #1482. |
| **FSR 3 upscaler via `wgpu-ffx`** | ADOPT | A Rust/wgpu-native port of the FSR 3.1 upscaler by a wgpu maintainer -- drop-in, MIT-licensed, pure shader (no tensor cores), satisfies the no-proprietary-blob doctrine. Render 720p -> present 1080p is roughly 1.8x GPU headroom, and it doubles as the TAA solution. Requires designing in jitter + motion vectors now. |
| **Virtual texturing** | WATCH | Only if terrain texture data actually exceeds VRAM; stylized art may get away with per-tile splat maps. |

**One flagged hardware risk:** Intel published a support advisory (July 2026) that valid
WebGPU indirect-draw workloads can hang UHD 7xx / Iris Xe iGPUs (device removed, no fix yet).
Since GPU-driven rendering is the core strategy, keep a CPU-culled non-indirect fallback path
behind a flag and test on real Intel iGPU hardware continuously -- which also resolves #1470.

## 3. "The new triangles and math": Triangle Splatting

The technology in question is **Triangle Splatting** (Held et al. 2025) and its 2026
successors **Triangle Splatting+** and **MeshSplatting**: instead of optimizing millions of
fuzzy 3D Gaussians from photos, they optimize millions of *plain opaque triangles*. The
output is an ordinary mesh that renders through the standard vertex/fragment pipeline with
z-buffering -- no per-frame sorting, no compute rasterizer, no CUDA at runtime. Performance is
startling: 2,400+ fps on a 4090, 220 fps at HD on a MacBook M4, and even the more expensive
stochastic variant runs on an Iris Xe iGPU. Triangle Splatting+ output even works directly
with a standard mesh collider (their Unity demo has walkable physics).

By contrast, classic **3D Gaussian Splatting** needs a per-frame GPU radix sort + alpha blend
and 200-650 MB VRAM per scene -- marginal at best on the min-spec. (If ever needed, Brush
(github.com/ArthurBrussee/brush) is an Apache-2.0 Rust/wgpu splat renderer to vendor rather
than rebuild.)

**The catch, and it's decisive:** the entire radiance-field family is a *capture* technology
for static scenes photographed in the real world. It cannot represent an authored fantasy
world, dynamic objects, destructibility, or day/night lighting (lighting is baked into the
representation; Triangle Splatting's per-triangle colors have no material model at all). No
shipped game uses splats as primary gameplay geometry as of mid-2026; the shipped uses are
backdrops, "skybox 2.0," and cinematic set pieces. There is also a licensing landmine: the
original Triangle Splatting trainer inherits Inria's non-commercial 3DGS license in part;
Triangle Splatting+ appears clean Apache-2.0 -- verify before commercial use.

**Verdict: EXPERIMENT.** Not a world representation for Ashen Reach -- the game is authored,
dynamic, and lit. But it is the first radiance-field tech whose output drops into the
existing mesh path at $500-PC speed. Worth one contained experiment (train a scene with TS+
on the 5090; the importer is days of work) for photoreal static set-dressing, backdrops, or
marketing captures -- and the field's direction (opaque, textured, budget-controlled
triangles) is worth tracking because it is converging on exactly what game engines want.

## 4. Diffusion technology

### Runtime diffusion world rendering -- NO

Every interactive world model in 2025-26 -- Google Genie 3 (~8 TPU v5 chips per session),
Decart Oasis 3 (cloud-only, ~$72/user-hour), Odyssey (H100 clusters, $1-2/user-hour),
Microsoft Muse/WHAMM (10 fps at 640x360), Matrix-Game 2.0 (the most efficient *open* model:
25 fps on a full H100) -- runs on datacenter hardware. A GTX 1650 is roughly 100-300x short on
compute and 20x short on memory, with no tensor path at all, against models whose coherence
horizon is still measured in minutes. Beyond compute, these models have no persistent game
state, no determinism (fatal for the multiplayer, replay/telemetry, and full-loot PvP
design), and no authored-content control. The labs themselves have pivoted their pitch to
agent training and AV simulation, not client runtime. Re-evaluate ~2028, and even then expect
"RTX 6090 feature," never "$500 PC feature."

### Neural rendering that does fit the min-spec -- two things

- **Neural Texture Compression, "inference on load" mode**: NVIDIA's RTXNTC decompresses to
  ordinary BCn textures at load time, needs only Shader Model 6, and is validated on GTX
  10-series / RX 6000 / Arc. 8-20x smaller texture shipping size. Worth prototyping once the
  asset pipeline (#1098) matures; check SDK redistribution terms.
- **Shader-based upscaling** (FSR 2/3 class -- already the Adopt above). DLSS, FSR 4, and
  neural radiance caching are all tensor-gated: skip at min-spec (DLSS opt-in only, per
  #1152's own framing).

### Generative AI as a dev-time asset pipeline -- ADOPT NOW

This is where diffusion actually pays off for this project, and it perfectly matches "create
high, play low": generation runs on the 5090 (or a hosted service), and what ships is
ordinary meshes and textures with zero runtime cost. Mature in 2026: **Meshy / Tripo**
(text/image -> PBR-textured props with clean commercial rights on paid tiers --
production-usable for stylized props and environment dressing with a Blender cleanup pass),
**Stable Diffusion / Flux** locally for textures and trim sheets, and **World Labs Marble**
($35/mo Pro: text/image -> downloadable 3D scene as GLB mesh or Gaussian splats) as an
environment-blockout tool. LLM-authored content data -- which the content-pipeline already
does for Thera areas -- is the quietly most practical technique of all. Two licensing traps to
avoid for shipped assets: self-hosted TRELLIS (texture stage depends on non-commercial
nvdiffrast) and Hunyuan3D (community license excludes EU/UK/KR sales). Note #1717's
zero-paid-services doctrine would need a carve-out or free-tier discipline for the hosted
tools.

## 5. Recommendation: how to proceed

Ordered by leverage; phases 1-2 directly unblock the Thera world epics and the min-spec
doctrine.

1. **Validate reality first (#1470).** Acquire one GTX 1650/1660-class or Iris Xe box
   (~$100-300 used) and stand up a real min-spec FSV rig capturing p95 frame-time at 1080p.
   Every claim below should be proven on it, per the project's own doctrine. This also
   surfaces the Intel indirect-draw driver risk immediately.
2. **Build the world layer with the proven low-end stack (epics #801, #1482, #1085).**
   CDLOD/quadtree terrain with chunked-clipmap streaming and geomorphing; meshopt LOD chains
   through the asset pipeline (#1098); extend HZB culling to two-phase occlusion +
   multi-draw-indirect with a CPU-culled fallback flag. This is 2-4 months of well-trodden
   work, not research.
3. **Design in jitter + motion vectors now, integrate `wgpu-ffx` (FSR 3) (#1152).** Cheapest
   ~1.8x GPU budget multiplier available; retrofitting motion vectors later is far more
   expensive than adding them while the terrain pass is being written.
4. **Ship baked probe-grid GI** (irradiance + visibility, day/night states baked, half-res
   evaluation) and expose the grid to stealth AI as the light-level query (#1597). Keep
   Radiance Cascades on the research shelf.
5. **Adopt the gen-AI asset pipeline at dev time**: Meshy/Tripo for stylized props into the
   glTF pipeline, SD/Flux for textures, Marble for blockout experiments. File issues under
   #1098/#984.
6. **Run one Triangle Splatting+ experiment** (a captured scene -> importer -> backdrop in
   the editor) to build hands-on knowledge of the tech most likely to matter in 2-3 years.
   Timebox it; verify the Apache-2.0 lineage.
7. **Skip, with written rationale in a doctrine issue:** Nanite-style virtual geometry, mesh
   shaders, runtime Gaussian splatting, runtime diffusion world models, DLSS/FSR4/NRC as
   defaults. Revisit diffusion runtime ~2028.

**Why this is the right call:** the 2025 shipping record is unambiguous -- the best-looking
games on weak hardware (Overwatch 2, PoE2, RE Engine titles) win with GPU-driven culling,
baked/probe GI, aggressive LOD, and temporal upscaling, not with virtualized geometry or
neural rendering. The stylized art direction (#928's explicit WC3-Reforged/LoL/Ori ceiling)
makes photoreal-density tech doubly unnecessary. The novel tech worth money today runs at
*dev time* on the 5090 -- exactly the asymmetry the "create high, play low" business model was
designed around.

## Appendix: key sources

- Triangle Splatting: https://trianglesplatting.github.io/ | TS+: https://arxiv.org/abs/2509.25122 | MeshSplatting: https://meshsplatting.github.io/ (CVPR 2026)
- Brush (Rust/wgpu 3DGS, Apache-2.0): https://github.com/ArthurBrussee/brush | SOG compression: https://developer.playcanvas.com/user-manual/gaussian-splatting/formats/sog/ | Splat-to-game pipeline: https://blog.playcanvas.com/turning-a-gaussian-splat-into-a-videogame/
- Overwatch 2 baked-DDGI (REAC 2025): https://enginearchitecture.org/downloads/REAC_2025_Overwatch.pdf | Capcom RE Engine meshlets: https://enginearchitecture.org/downloads/REAC_2025_Capcom.pdf | Radiance Cascades: https://radiance.wiki/papers/sannikov-original
- wgpu-ffx (FSR3 for wgpu): https://github.com/cwfitzgerald/wgpu-ffx | wgpu mesh shaders: https://github.com/gfx-rs/wgpu/releases/tag/v28.0.0 | Intel iGPU indirect-draw advisory: https://www.intel.com/content/www/us/en/support/articles/000102979/graphics.html
- Bevy virtual geometry status: https://jms55.github.io/posts/2025-03-27-virtual-geometry-bevy-0-16/ | CDLOD/UDLOD terrain thesis (Rust/wgpu): https://d-nb.info/1383361673/34
- Genie 3: https://deepmind.google/blog/genie-3-a-new-frontier-for-world-models/ | Oasis 3: https://techcrunch.com/2026/06/10/decarts-new-world-model-can-simulate-hours-of-photorealistic-driving-with-some-caveats/ | Matrix-Game 2.0: https://arxiv.org/html/2508.13009v1 | Muse/WHAMM: https://www.microsoft.com/en-us/research/blog/introducing-muse-our-first-generative-ai-model-designed-for-gameplay-ideation/
- RTXNTC (neural texture compression): https://github.com/NVIDIA-RTX/RTXNTC | World Labs Marble exports: https://docs.worldlabs.ai/marble/export/specs
