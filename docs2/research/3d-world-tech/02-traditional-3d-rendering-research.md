# State of the Art (2025-2026): Rasterized 3D World Rendering Feasible on a $500 Windows PC, in Rust + wgpu

**Research date: 2026-07-12. Web research (Exa + web search).**

Min-spec framing used throughout: Intel Iris Xe / UHD 7xx / Arc iGPU, AMD 780M/880M, GTX
1650 / RX 6400 class, all DX12-capable via wgpu, ~1080p target, no hardware ray tracing
assumed. Verdicts are for a small team building a large persistent RPG world (terrain,
cities, dungeons, stealth).

---

## 1. Virtualized Geometry (Nanite-like, meshlets, visibility buffer, mesh shaders)

**What it is.** Nanite-style rendering: preprocess meshes into ~128-triangle meshlets
organized in a simplification DAG, do GPU-driven per-cluster frustum/occlusion culling and
continuous LOD selection, then rasterize into a 64-bit visibility buffer (depth in high 32
bits, cluster+triangle ID in low bits, resolved with `atomicMax`), with a compute-shader
software rasterizer for sub-pixel triangles and hardware raster for large ones (GDC 2024
Nanite talk: https://media.gdcvault.com/gdc2024/Slides/GDC+slide+presentations/Nanite+GPU+Driven+Materials.pdf).

**2025-2026 developments:**

- **Bevy virtual geometry (JMS55 et al.)** is the reference open implementation in the
  wgpu/WGSL world. Through 0.15-0.17 it gained METIS-based DAG building, per-meshlet
  compressed vertex data (unblocking streaming), a much faster software rasterizer, and
  BVH-based culling (0.17, by atlv24/SparkyPotato). It is *still experimental* as of Bevy
  0.18/0.19 (mid-2026): the 0.18 migration guide landed Dec 2025, and it was actually broken
  for two releases and only fixed again in March 2026 for 0.19 ("Fix Meshlets" PR #23307).
  Streaming is still not done. Sources:
  https://jms55.github.io/posts/2025-03-27-virtual-geometry-bevy-0-16/,
  https://jms55.github.io/posts/2025-09-03-bevy-fifth-birthday/,
  https://github.com/bevyengine/bevy/pull/23307,
  https://github.com/bevyengine/bevy/issues/11518.
- **Critically:** Bevy's `MeshletPlugin` requires `TEXTURE_INT64_ATOMIC` and **works only on
  Vulkan and Metal backends -- not DX12**
  (https://docs.rs/bevy/latest/bevy/pbr/experimental/meshlet/struct.MeshletPlugin.html).
  64-bit atomics on DX12 require SM 6.6-level hardware support; Intel's pre-Arc iGPUs don't
  expose 64-bit *typed* atomics at all, and even Arc/DG2 only exposes a subset via a vendor
  extension framework (https://github.com/GameTechDev/64-bit-Typed-Atomics-Extension). Docs
  confirm `SHADER_INT64_ATOMIC_*` and `TEXTURE_INT64_ATOMIC` need DX12 SM 6.6+
  (https://docs.rs/wgpu/latest/wgpu/struct.Features.html).
- **wgpu mesh shaders**: shipped as `EXPERIMENTAL_MESH_SHADER` in **wgpu v28.0.0 (Dec 18,
  2025)** -- full support on Vulkan, only *passthrough* (precompiled) shaders on DX12/Metal;
  WGSL support via `enable wgpu_mesh_shader;`
  (https://github.com/gfx-rs/wgpu/releases/tag/v28.0.0,
  https://github.com/gfx-rs/wgpu/issues/7197,
  https://github.com/gfx-rs/wgpu/blob/trunk/docs/api-specs/mesh_shading.md). Hardware mesh
  shader support also excludes Iris Xe/UHD iGPUs, GTX 1650 (Turing GTX lacks it on DX12), and
  RX 6400 has it but weakly. Mesh shaders are **not** a min-spec-viable foundation in 2026.
- **Nanite on weak hardware**: Epic itself acknowledges high fixed overhead on PC (multiple
  quarter-rate SV_Depth passes, no HiZ acceleration, empty indirect-draw chains) and ~50%
  occupancy on mid-tier GPUs
  (https://forums.unrealengine.com/t/nanite-sw-rasterize-high-register-use-dx12/2719076).
  Nanite's VisBuffer+BasePass costs several ms even on an RX 580-class GPU
  (https://crdg.jp/studio-tech-1/en/2025/05/29/3438/). Intel's own UE5 iGPU guide (June 2026)
  shows Nanite runs on modern iGPUs but needs software-VRS patches and careful budgeting
  (https://www.intel.com/content/www/us/en/developer/articles/technical/unreal-engine-optimization-chapter-2.html).
  Capcom's RE Engine (REAC 2025) instead ships a lighter meshlet pipeline using
  multi-draw-indirect + GPU occlusion culling without full Nanite-style SW raster
  (https://enginearchitecture.org/downloads/REAC_2025_Capcom.pdf).

**Effort:** JMS55's blog series makes clear this is a multi-person-year effort even with
meshoptimizer/METIS doing the heavy lifting, and it's still not production-ready in Bevy
after 2+ years.

**Verdict for min-spec: skip full virtualized geometry.** The 64-bit-atomic visibility
buffer path is blocked or fragile on exactly the worst-case hardware (Intel iGPUs on DX12),
mesh shaders are unavailable there, and the fixed per-frame overhead (1-3 ms+) eats an iGPU
frame budget. Take the *pieces* that are min-spec-friendly instead: meshlet-granularity
GPU-driven culling with ordinary indirect draws, aggressive precomputed LODs via
meshoptimizer, and Nanite-style two-phase occlusion culling. That's the
Capcom/Assassin's-Creed-style "GPU-driven, not virtualized" middle path, and it is well
within a small team's reach.

---

## 2. Global Illumination on Low-End

**Radiance Cascades (Sannikov / PoE2).** A probe hierarchy where near-field probes are dense
with few rays and far-field probes are sparse with many rays; cost is constant and
scene-independent, no RT hardware, no temporal accumulation needed. PoE2 shipped a
*screen-space* RC implementation (raymarching 4 rays at once into 128-bit occlusion bitmasks)
that runs on very modest GPUs -- 2D/flatland variants run at ~0.3 ms on a GTX 970
(https://github.com/Raikiri/RadianceCascadesPaper/blob/main/RadianceCascades.tex,
https://radiance.wiki/papers/sannikov-original,
https://80.lv/articles/radiance-cascades-new-approach-to-calculating-global-illumination).
2025-2026 developments: Sannikov demonstrated **sparse 3D world-space radiance cascades**
(Nov 2025, probes allocated only where visible) which attacks the main problem -- full 3D
probe grids' memory scaling (https://www.youtube.com/watch?v=TGLAxW0xVDU). Caveats: full 3D
world-space RC is still research-grade; PoE2's shipped version leans on its fixed top-down
camera. The formal treatment (Osborne & Sannikov 2024) and community wiki (radiance.wiki)
exist; wgpu/WGSL implementations exist but are mostly 2D (e.g.
https://github.com/nixon-voxell/bevy_radiance_cascades, wgpu compute-shader ports).
**Verdict: extremely promising, iGPU-friendly cost profile, but 3D world-space RC is a
research project for a small team, not a checkbox.** Screen-space RC is more tractable but
suffers standard screen-space limitations for a free-camera RPG.

**Baked/probe GI (DDGI-style volumes, baked offline) -- the proven low-end answer.** The
single most relevant 2025 data point is **Overwatch 2's REAC 2025 talk**: they evaluated
RTXGI/DDGI, found runtime tracing infeasible on low-end, and shipped **baked DDGI** --
artist-placed probe volumes, irradiance+visibility (Chebyshev) textures baked offline,
evaluated at half resolution + temporal upsample on low-end. Cost: ~330 us full-res / ~115 us
half-res on a GTX 1080; after optimization (RG11B10 targets, fp16, split loops) the real
penalty on Switch-class hardware is ~1 ms, ~0.5 ms on Xbox One-class
(https://enginearchitecture.org/downloads/REAC_2025_Overwatch.pdf). A largely static world
(cities, dungeons) with day/night can bake multiple lighting states or re-bake probes
incrementally/asynchronously on GPU. DDGI's leak-resistant probe interpolation math is public
(JCGT paper, RTXGI SDK shader code:
https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/Algorithms.md). **Effort:
weeks-to-a-couple-months for a competent graphics programmer; well-trodden.**

**Runtime DDGI (compute-traced, no RT cores):** Overwatch 2 explicitly found even the compute
fallback "not realistic" on low-end platforms. Skip.

**Screen-space GI:** cheap (sub-ms at reduced res, shipped on mobile/Switch-class hardware,
e.g. MF.SSGI runs on GLES3-class devices: https://github.com/MFrankfort84/mfssgi), good as a
*supplement* for dynamic near-field bounce and for stealth-relevant contact darkening, but
unstable off-screen and unsuitable as the sole GI.

**Verdict for min-spec:** **Baked DDGI-style probe volumes (irradiance + visibility) as the
backbone, ~1 ms budget at half-res; optional SSGI/SSAO layer on top; keep Radiance Cascades
as a v2 research track** (its screen-space form could later replace SSGI; sparse 3D RC could
eventually replace the bake). This combination is exactly what a 2025 AAA title shipped for
hardware older than the min-spec. Bonus for stealth gameplay: a probe grid gives a queryable
"light level at position" for AI visibility checks essentially for free.

---

## 3. Terrain & Large Worlds

**Modern consensus (2025-2026) for huge terrains on weak GPUs** is a GPU-driven
quadtree/CDLOD or clipmap with heightmap textures, geomorphing, and streaming -- not
tessellation shaders (unavailable in wgpu anyway) and not virtualized geometry.

- **Geometry clipmaps** (Losasso/Hoppe; GPU Gems 2 ch. 2): constant vertex budget (~500K
  verts for N=256, 8 rings) regardless of world size, toroidal texture updates, geomorph at
  ring boundaries. Still the simplest robust choice; a 2026 build log reconfirms it as the
  pragmatic browser/low-end choice
  (https://app.cinevva.com/blog/2026-02-25-open-world-browser-part-06-clipmaps,
  https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry).
- **CDLOD / GPU-driven quadtree (best fit):** subdivide an implicit quadtree per frame in a
  compute shader, cull tiles, emit **one indirect draw** for all tiles with uniform vertex
  grids, morph LOD boundaries. A 2025 thesis (Kurt Kuhnert lineage -- the same author behind
  the Rust/wgpu **bevy_terrain** crate) formalizes this as "UDLOD" plus a **chunked clipmap**
  (node atlas + clipped quadtree lookup) for data streaming with graceful fallback to coarser
  tiles when streaming lags, and solves f32 precision at planet scale via Taylor-series
  relative-to-viewer coordinates (https://d-nb.info/1383361673/34). This exists in Rust/wgpu
  today -- it can be cribbed or vendored.
- **Vertical error / adaptive density**: Witcher 3-style per-region max-error metric to cut
  triangles on flat ground (~2x raster savings) (summarized with benchmarks incl. iGPU in
  https://www.youtube.com/watch?v=KoAERjoWl0g).
- **Virtual texturing** for terrain splatting/megatexture: page table texture + physical
  atlas + feedback pass (render at 1/16-1/32 res, read back page requests) + LRU residency,
  Far Cry 4 "adaptive virtual texturing" style. A thorough 2026 explainer:
  https://www.shlom.dev/articles/how-virtual-texturing-work/ (note its honest caveat: VT is a
  *system*, worth it only when texture data truly exceeds VRAM -- for stylized art you may get
  away with per-tile splat maps + a small VT only for terrain albedo). wgpu supports
  everything needed (compute, storage textures, copies); there is no sparse-texture hardware
  path in wgpu, so it's the classic software indirection -- which is what most engines ship
  anyway.
- **GPU-driven culling for props/cities:** two-phase occlusion culling (render last frame's
  visible set -> build Hi-Z -> re-test everything against Hi-Z -> draw newly disoccluded) is
  now standard and implementable directly in wgpu compute (Bevy landed exactly this for its
  standard mesh pipeline in 0.16, PR #17413: https://github.com/bevyengine/bevy/pull/17413;
  walkthroughs:
  https://medium.com/@Lucmomber/two-pass-hierarchical-z-buffer-occlusion-culling-93171c5a9808,
  original: Aaltonen SIGGRAPH 2015
  https://advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf).
  This matters enormously for dense cities and dungeons and costs almost nothing on iGPUs
  (Hi-Z build is cheap; the win is skipped vertex+fragment work).

**Effort:** CDLOD terrain + streaming: 1-3 months. Two-phase occlusion + indirect draws: 2-6
weeks. Virtual texturing: 1-2 months if needed. All mature, well-documented techniques.

**Verdict:** **GPU-driven quadtree terrain (CDLOD/UDLOD) with chunked-clipmap streaming,
geomorphing, vertical-error LOD, and two-phase Hi-Z occlusion culling for everything else.**
This is precisely the sweet spot: modern, scalable to huge worlds, and light on iGPUs. Use
bevy_terrain/the Kuhnert thesis as the blueprint since it's already wgpu/WGSL-native.

---

## 4. Upscaling on Low-End

Temporal upscaling is arguably the *highest-leverage* min-spec feature: render at 720p,
present at 1080p, ~1.8x GPU headroom.

- **FSR 2 / FSR 3.1 upscaler (analytical, open source, MIT):** runs on any GPU with typed UAV
  loads + RGBA16 UNORM (all min-spec hardware qualifies), no ML hardware. FidelityFX SDK
  ships FSR 2.3.x and FSR 3.1.5 upscalers as portable HLSL
  (https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK,
  https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/f4c1da8e/docs/techniques/super-resolution-upscaler.md).
- **The headline: `wgpu-ffx`** -- a **Rust/wgpu-native port of the FSR 3 upscaler by Connor
  Fitzgerald (wgpu maintainer)**. It takes color/depth/motion vectors, handles Halton jitter,
  and dispatches entirely through wgpu -- a drop-in for a custom wgpu engine, no C++ SDK, no
  backend interop (https://github.com/cwfitzgerald/wgpu-ffx,
  https://docs.rs/wgpu-ffx/latest/wgpu_ffx/). This removes what used to be the biggest
  integration obstacle.
- **FSR 4 / "Redstone" (2025-2026):** ML-based, effectively RDNA4-class; falls back to FSR 3
  elsewhere (https://gpuopen.com/learn/ue-fsr/). Not a min-spec tool.
- **XeSS 2.x:** DP4a fallback path runs on non-Intel GPUs and is very good on Intel
  iGPUs/Arc; Rust bindings exist as of Oct 2025 (`xess-rs`, Traverse Research:
  https://github.com/Traverse-Research/xess-rs) but they bind the native DX12/Vulkan
  library -- awkward through wgpu (needs raw handle interop). Optional nicety for Intel
  hardware later.
- **Frame generation (FSR3 FG/XeSS-FG):** requires swapchain proxying and is
  latency-sensitive; poor fit for wgpu and for a stealth RPG at low base framerates. Skip.

**Effort:** With wgpu-ffx: days-to-weeks (the real work is producing correct motion vectors
and jittering the projection matrix -- which is also needed for TAA anyway).

**Verdict:** **Integrate wgpu-ffx (FSR 3 upscaler) early and design the renderer around
jittered rendering + motion vectors from day one.** It doubles the effective GPU budget on
iGPUs and also provides the anti-aliasing solution.

---

## 5. wgpu/WGSL Constraints on DX12 iGPUs (as of wgpu 28/29, mid-2026)

What can be counted on across the whole min-spec (DX12 backend):

- **Compute shaders, storage buffers/textures, indirect draw & dispatch** -- core WebGPU,
  everywhere.
- **`MULTI_DRAW_INDIRECT` and `MULTI_DRAW_INDIRECT_COUNT`** -- native features, supported on
  DX12 (ExecuteIndirect) and Vulkan (https://docs.rs/wgpu/latest/wgpu/struct.Features.html).
  Note 2025 fixes: `vertex_index`/`instance_index` builtins for indirect draws on DX12 landed
  via a validation-shader trick (https://github.com/gfx-rs/wgpu/pull/7535), and
  `multi_draw_indirect_count` had builtin gaps into late 2025
  (https://github.com/gfx-rs/wgpu/issues/7974) -- test on real hardware.
- **Bindless (`binding_array`)**: `TEXTURE_BINDING_ARRAY` + `PARTIALLY_BOUND_BINDING_ARRAY`
  now work on DX12 Resource Binding Tier 3 (most DX12 hardware including Xe/RDNA2) (wgpu
  changelog:
  https://github.com/gfx-rs/wgpu/blob/0f37714fb8ed117f85f53b4f61ac70d4ec481a46/CHANGELOG.md).
  Usable, but keep a non-bindless fallback for oldest UHD parts (Tier 2).
- **64-bit atomics (`SHADER_INT64_ATOMIC_*`, `TEXTURE_INT64_ATOMIC`)**: DX12 requires SM 6.6
  + `AtomicInt64OnTypedResourceSupported` -- **absent on Intel pre-Arc iGPUs, spotty
  elsewhere**; wgpu emulates image atomics via RG32Uint on some platforms but with
  restrictions (https://github.com/gfx-rs/wgpu/pull/5383,
  https://github.com/gfx-rs/wgpu/pull/5537). Treat as unavailable at min-spec -> no
  64-bit-atomic visibility buffer. (A 32-bit depth-only atomic prepass + separate ID pass is
  the known workaround if SW raster is ever wanted.)
- **Mesh shaders**: experimental, Vulkan-only for WGSL-authored shaders (see Section 1). Not
  min-spec.
- **Driver reality check:** Intel published a support article (July 2026) acknowledging that
  a valid WebGPU **indirect-draw workload hangs UHD 7xx / Iris Xe iGPUs**
  (DXGI_ERROR_DEVICE_REMOVED, command allocator failure) with no fix yet
  (https://www.intel.com/content/www/us/en/support/articles/000102979/graphics.html). Since
  GPU-driven rendering is the core strategy, (a) test on real Iris Xe/UHD hardware
  continuously, (b) keep a CPU-culled non-indirect fallback path behind a flag, and (c)
  consider wgpu's Vulkan backend as an alternative on Intel (Intel's Vulkan driver takes a
  different path). This is the single biggest practical risk in the whole plan.
- Also useful and broadly available on DX12: `INDIRECT_FIRST_INSTANCE`,
  `DEPTH_CLIP_CONTROL`, timestamp queries (for perf budget work), 16-bit float in shaders
  (`SHADER_F16`, big iGPU win per the Overwatch talk -- check per-adapter).

---

## Recommended Architecture (summary verdict)

| Area | Ship this | Skip / defer |
|---|---|---|
| Geometry | GPU-driven renderer: meshopt-generated LOD chains + meshlet/cluster frustum culling in compute + two-phase Hi-Z occlusion + `multi_draw_indirect(_count)`; CPU-culled fallback path for buggy iGPU drivers | Full Nanite-style SW-raster/64-bit visibility buffer; mesh shaders |
| GI | Baked DDGI-style probe volumes (irradiance+visibility, half-res eval + upsample, ~1 ms) + SSAO/light SSGI; probe grid doubles as stealth light-level query | Runtime-traced DDGI; 3D world-space Radiance Cascades (keep as research track -- watch Sannikov's sparse 3D RC) |
| Terrain | GPU-driven quadtree (CDLOD/UDLOD) + chunked-clipmap streaming + geomorph + vertical-error LOD; virtual texturing only if texture data demands it (bevy_terrain thesis as blueprint) | Hardware tessellation (not in wgpu), voxel/virtualized terrain |
| Upscaling | wgpu-ffx (FSR 3 upscaler) as the TAA+upscale solution, designed in from day one | FSR 4, frame generation, XeSS (optional later via xess-rs) |
| wgpu features | compute, indirect, MDI(+count), bindless-with-fallback, f16 where available | 64-bit atomics, mesh shaders |

This stack mirrors what actually shipped on low-end in 2025 (Overwatch 2's baked DDGI, RE
Engine's meshlet+MDI pipeline, PoE2's cascades philosophy of "constant cost, no RT hardware")
and everything in it is implementable by a small team in wgpu today -- with the one caveat
that Intel iGPU indirect-draw driver stability must be validated on hardware early and
continuously.
