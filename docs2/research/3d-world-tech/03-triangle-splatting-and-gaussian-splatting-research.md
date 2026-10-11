# Research Report: Triangle Splatting, 3DGS State of the Art, and What It Means for a Rust/wgpu Engine Targeting $500 PCs

**Research date: 2026-07-12. Web research (Exa + web search).**

## Executive summary

The "new triangles and math" technology is **Triangle Splatting** (Held et al., 2025,
accepted 3DV 2026) and its successors **Triangle Splatting+** and **MeshSplatting** (CVPR
2026). It is the branch of the radiance-field/splatting family most relevant to us, precisely
because its output is *ordinary opaque triangles* that any rasterizer -- including our wgpu
renderer -- draws natively, with no per-frame depth sorting, no custom compute rasterizer, and
no CUDA. For our min-spec ($500 PC, iGPU/GTX 1650 class), triangle-family methods are
dramatically friendlier than 3D Gaussian Splatting. But the entire family shares one hard
limitation: these are **capture/optimization technologies for static scenes**, not authoring
technologies. They are a way to get photoreal *static environments* cheaply, not a polygon
replacement for a dynamic, interactive game world.

---

## 1. Triangle Splatting and its 2025-2026 successors

### What it is

Triangle Splatting ("Triangle Splatting for Real-Time Radiance Field Rendering," arXiv
2505.19175, 3DV 2026) is a differentiable renderer that optimizes a "triangle soup" --
millions of unstructured 3D triangles, each with vertices, color, opacity, and a smoothness
parameter -- directly from photos via gradient descent, the same way 3DGS optimizes Gaussians.
The key trick is a new window function built on the triangle's 2D signed distance field,
giving soft, differentiable edges during training that can be annealed to hard, opaque
triangles at the end.

- Project page: https://trianglesplatting.github.io/
- Paper: https://arxiv.org/abs/2505.19175
- Code: https://github.com/trianglesplatting/triangle-splatting

### How it differs from 3DGS and why triangles matter

- **3DGS** = millions of semi-transparent anisotropic ellipsoids that must be **depth-sorted
  every frame** and alpha-blended in a custom tile-based compute rasterizer. Unbounded
  support, fuzzy "surfaces," no native depth buffer.
- **Triangle Splatting** = when trained with opacity -> 1, sigma -> 0 (their
  `train_game_engine.py` path), the output is a plain colored mesh (.off/.ply). It renders
  through the **standard vertex/fragment pipeline with z-buffering -- no sorting, no compute
  shaders, no special format**. This is the whole point: it's rasterizer-native and
  game-engine compatible by construction.
- Quality: beats 3DGS/2DGS/3DCS on Mip-NeRF360 LPIPS (~10% better than 3DGS) and matches
  Zip-NeRF perceptual quality indoors while rendering ~500x faster.

### Reported performance (the headline for our min-spec)

From the paper (Garden scene, ~2M triangles, off-the-shelf mesh renderer):

- RTX 4090: **2,400+ FPS at 1280x720**, 1,900 FPS Full HD, 1,050 FPS 4K
- MacBook M4 (8 TFLOPS): 500 FPS HD, 370 Full HD
- RTX 5000 laptop, Windows: 570 HD / 380 FHD / 290 4K
- Stochastic (semi-transparent) variant on **Intel Iris Xe iGPU (2 TFLOPS)**: 33 FPS HD / 23
  FPS FHD on a 4.3M-triangle scene -- i.e., even the *expensive* variant is
  borderline-playable on an iGPU, and the fully opaque variant is far cheaper.

A GTX 1650 (~3 TFLOPS) sits between Iris Xe and the M4; the opaque-triangle path should
comfortably clear 60 FPS at 1080p for scenes in the low millions of triangles. Sources:
https://www.robots.ox.ac.uk/~vedaldi/assets/pubs/held26triangle.pdf, project page.

### Follow-ups (the ecosystem moved fast in late 2025-2026)

- **Triangle Splatting+** (arXiv 2509.25122,
  https://trianglesplatting2.github.io/trianglesplatting2/): fixes the two big flaws of v1 --
  adds **shared-vertex connectivity** (semi-connected mesh, not pure soup) and **enforces
  opaque triangles during training**, so there is no quality drop when moved into a game
  engine. State of the art in mesh-based novel view synthesis: 25.21 PSNR on Mip-NeRF360 with
  only 2M vertices (vs. MiLo's 24.09 at 7M). Trains in ~39 min (A100). Output files are
  **~30-50 MB per scene uncompressed** (Garden 43MB, Truck 30MB). They ship a **Unity demo
  with physics colliders and walkable scenes using the mesh directly** -- the mesh is solid
  enough for an off-the-shelf non-convex mesh collider. Code:
  https://github.com/trianglesplatting2/triangle-splatting2
- **MeshSplatting** (CVPR 2026, https://meshsplatting.github.io/, arXiv 2512.06818): same
  group; enforces full connectivity via restricted Delaunay triangulation -> **connected,
  colored, game-ready mesh**, 24.78 PSNR at 3M verts, ~100 MB, 48 min training, renders **220
  FPS at HD on a consumer MacBook M4**. Code: https://github.com/meshsplatting/mesh-splatting
- **DiffSoup** (CVPR 2026,
  https://en.papernotes.org/CVPR2026/3d_vision/diffsoup_direct_differentiable_rasterization_of_triangle_soup_for_extreme_radian/):
  "extreme simplification" -- **<20K textured triangles** with neural textures + binary
  opacity + a tiny deferred-shading MLP; beats 3DGS/TexturedGaussians at equal budgets, **146
  FPS on a MacBook laptop**, ~10x faster than CUDA 3DGS because there's no alpha sorting.
  Most interesting for *mobile/iGPU budget* scenes, but requires an MLP pass in the shader.
- Also in the family: **1000FPS+ opaque triangle optimization** (3DV 2026, doi
  10.1109/3dv69130.2026.00115) and **FLAT** (feedforward single-image -> triangle splats,
  arXiv 2606.24876).

### Tooling maturity and licensing

- Training code is PyTorch + a custom CUDA rasterizer -- you need an NVIDIA GPU **for the
  offline capture/training step only** (our RTX 5090 dev machine is fine). Runtime needs
  nothing special.
- **Licensing -- important nuance.** Both `triangle-splatting` and `triangle-splatting2` repos
  are **Apache-2.0** at the top level, BUT the original repo also carries `LICENSE_GS.md` --
  the **Inria/MPII non-commercial Gaussian-Splatting license** -- because the differentiable
  rasterizer descends from Inria's 3DGS code
  (https://github.com/trianglesplatting/triangle-splatting/blob/main/LICENSE_GS.md). Before
  shipping anything trained with their pipeline commercially, we'd need legal review of which
  components inherit the Inria terms (the trained *assets* from our own photos are generally
  considered ours; the concern is the training software). Triangle Splatting+'s repo shows
  only Apache-2.0. The 3DGS reference implementation itself is explicitly non-commercial
  without Inria consent -- a known industry pain point.
- Maturity: research-grade (repos ~1.1K/300 stars, 2 main contributors, active through late
  2025). No DCC-grade cleanup/editing tools yet like the splat ecosystem has (SuperSplat,
  Postshot). The v1 paper itself says game-engine visual quality wasn't the training target;
  TS+ fixes most of that.

---

## 2. 3D Gaussian Splatting state of the art, 2025-2026

### Compression -- solved problem

- **SOG (Self-Organizing Gaussians)**, adopted and open-sourced by PlayCanvas (Sept 2025):
  stores splat attributes as WebP images, **15-20x smaller than PLY** (a 1 GB church scene ->
  55 MB with no significant visible loss). Spec:
  https://developer.playcanvas.com/user-manual/gaussian-splatting/formats/sog/ ;
  https://blog.playcanvas.com/playcanvas-open-sources-sog-format-for-gaussian-splatting/.
  Other formats: SPZ (Niantic), .ksplat, compressed PLY; `splat-transform` CLI converts among
  all of them (https://github.com/playcanvas/splat-transform).

### LOD / streaming for large scenes -- working

- **Hierarchical 3DGS** (Kerbl et al., SIGGRAPH 2024) established the LOD-hierarchy approach;
  **CityGaussian/CityGaussianV2** (ECCV'24/ICLR'25, https://github.com/Linketic/CityGaussian)
  do divide-and-conquer training + block-wise LOD for km2-scale city scenes; **A LoD of
  Gaussians** (2025, arXiv 2507.01110) streams 60M+ Gaussians out-of-core on a single
  consumer GPU.
- Production version: PlayCanvas **Streamed SOG** -- automatic LOD chains (halving Gaussian
  count per level), chunked spatial streaming, runtime Gaussian budgets (2-4M desktop, 1-2M
  mobile). https://developer.playcanvas.com/user-manual/supersplat/streaming/. Also
  Nanite-style cluster-LOD UE plugins
  (https://github.com/TimChen1383/NanoGaussianSplatting).

### Dynamic scenes and relighting -- still research-grade

- 4DGS (volumetric video) is real but heavy: 4DGS-1K (arXiv 2503.16422) and Disentangled4DGS
  (343 FPS on RTX 3090) show speed is achievable, and UE plugins (MLSLabs) play back
  100K-Gaussian volumetric video at 100+ FPS -- but this is *captured playback*, not
  simulation. No "rig a splat" workflow exists in any production tool (aukimi production
  survey, https://aukimi.com/blog/gaussian-splatting-production-pipelines-2026).
- Relighting: lighting is baked into SH coefficients. Research (ROS-GS arXiv 2509.11275,
  LumiGauss, GaRe, Relightable 3D Gaussians) decomposes sun/sky/texture for outdoor
  relighting but nothing is production-ready. **Day/night cycles on a splat scene are
  effectively not supported today**; shipped projects fake it with lightness-probe grids for
  dynamic *objects* while the environment stays fixed.

### Engine integrations

- **Unity**: aras-p/UnityGaussianSplatting (canonical, MIT), gsplat-unity. 1M Gaussians is
  roughly 3-8 ms/frame on RTX 3070; 180-220 MB VRAM per 1M splats with full SH (60-80 MB at
  SH0). https://polyvia3d.com/guides/gaussian-splatting-unity-unreal
- **UE5**: no first-party support even in 5.7; Luma AI plugin (free) is the production path;
  community plugins (XVERSE XScene, MLSLabs -- min RTX 2060, Magnopus OKO ~7M splats @60fps).
  Splats bypass Lumen entirely -- no GI/reflection interaction with meshes.
  https://www.strayspark.studio/blog/gaussian-splatting-unreal-engine-5-capture-to-game-pipeline,
  https://www.magnopus.com/blog/how-i-ended-up-rendering-unlimited-gaussian-splats-in-unreal
- **Rust/wgpu -- directly relevant to us**: **Brush** by Arthur Brussee
  (https://github.com/ArthurBrussee/brush) is a full 3DGS **training + rendering engine in
  Rust/wgpu/WGSL** (Burn ML framework), **Apache-2.0**, 4.8K stars, 30 contributors, active
  (last push July 2026), runs on AMD/NVIDIA/Intel, Android, and in-browser via WebGPU. If we
  ever want splats in our engine, Brush's `brush-render` crate (WGSL compute kernels:
  projection, radix sort, tile rasterize) is the reference implementation in exactly our
  stack, license-clean.

### Can 3DGS work on iGPU-class hardware?

Marginally, with aggressive budgets. Mobile deployments cap at 200-500K splats for 30 FPS;
PlayCanvas's browser games run a 1-2M Gaussian budget on phones/Steam Deck; the per-frame GPU
radix sort + alpha blending is bandwidth-hungry -- the exact resource an iGPU lacks. VRAM is
the other killer: 200-650 MB for a room-scale scan vs 5-20 MB for an equivalent textured
mesh. On a $500 PC with shared memory, a hero splat environment would eat most of our frame
and memory budget. It's feasible for *one* modest showcase scene; it is not a general world
representation at our min-spec.

---

## 3. Other novel representations, 2025-2026

- **2DGS / surfels** (Huang et al., arXiv 2403.17888): flat oriented Gaussian disks; better
  geometry than 3DGS, standard route to mesh extraction via TSDF fusion -- but extracted
  meshes score badly for view synthesis (15.36 PSNR on Mip-NeRF360 vs TS+'s 25.21) and need
  post-processing + neural-field texturing. Superseded for game-export purposes by the
  triangle-splatting family.
- **Mesh-extraction pipelines**: GOF, RaDe-GS, SuGaR, **MiLo** (arXiv 2506.24096 --
  differentiable mesh-in-the-loop, best of the Gaussian->mesh methods) all produce
  7-33M-vertex meshes needing separate texturing passes; TS+/MeshSplatting beat them with
  2-3M verts and zero post-processing.
- **SVRaster -- Sparse Voxel Rasterization** (NVIDIA, CVPR 2025, https://svraster.github.io/):
  neural-free adaptive sparse voxels, no popping artifacts, Marching Cubes-compatible.
  Interesting, CUDA-based, research-grade.
- Also circulating: 3D Convex Splatting (CVPR 2025, same group's precursor), LinPrim (linear
  primitives), RadiantFoam, BBSplat (textured billboards), Textured Gaussians. None of these
  change our calculus.
- **"Polygon replacement" framing**: no credible 2025-2026 source claims these replace
  authored polygons. The consistent industry framing is "photogrammetry replacement" and
  "matte painting / skybox 2.0."

---

## 4. Critical assessment for GAME development

### What shipped products actually do

- **PlayCanvas FPS demo** (Apr 2026,
  https://blog.playcanvas.com/turning-a-gaussian-splat-into-a-videogame/, covered by Tom's
  Hardware): a real browser FPS inside a splat scan -- but note the recipe: the splat is
  **only the visuals**; collision comes from `splat-transform -K` voxelizing to a
  `.collision.glb`, navmesh from Recast over that proxy, and dynamic-object lighting from a
  pre-baked lightness-probe JSON grid. Every gameplay system runs on classic tech bolted
  underneath.
- Studios ship splats as: virtual-production backdrops, "skybox 2.0"/distant environments,
  cinematic set pieces, cultural-heritage walkthroughs (Yahoo/PC Gamer piece with scan artist
  Christoph Schindelar, June 2026 -- who notes splat game experiments run "really, really
  close" to acceptable on a Steam Deck). No major commercial game ships splats as primary
  gameplay geometry as of mid-2026.

### The four hard questions

1. **Authored content?** No. Every method here starts from *photographs of a real place*
   (or, with FLAT-type work, generative video priors). You cannot art-direct a fantasy
   dungeon in this pipeline except by building/photographing physical or rendered sets. For
   an authored game world, this is a capture tool, not an authoring tool.
2. **Dynamic objects / destructibility?** No. Splats and triangle soups are static;
   deformation/animation is research-only. Dynamic things are standard meshes composited on
   top.
3. **Day/night, dynamic lighting?** No. Lighting is baked into the representation. Triangle
   Splatting's per-triangle flat colors have *no material model at all* (no normals for
   lighting, no PBR), so a captured scene can't be relit either -- though because it IS a
   mesh, one could in principle bake lighting to vertex colors offline or run it through our
   lighting as unlit geometry.
4. **Collision?** Triangle Splatting+/MeshSplatting: **yes, directly** -- the opaque
   semi-connected mesh works with a standard mesh collider (their own Unity demo proves it).
   3DGS: only via derived voxel proxy meshes.

### Fit for our engine (Rust + wgpu, $500 PC min-spec)

| Tech | Hardware vs min-spec | wgpu/WGSL effort | Maturity | Verdict |
|---|---|---|---|---|
| **Triangle Splatting / TS+ / MeshSplatting** | Excellent -- opaque triangles, z-buffer, no sorting; iGPU-viable (M4 220 FPS HD; even stochastic variant runs on Iris Xe) | **Days, not months**: importer for their .ply/.off + our existing mesh path; vertex-color unlit shader. Training stays offline on the 5090 (their CUDA/PyTorch code) | Research-grade tooling, but output is a standard mesh so downstream tooling is *our normal pipeline* | **Best fit in the family.** Viable now for static photoreal set pieces / backdrops. Check LICENSE_GS.md inheritance before commercial use of the v1 trainer; TS+ repo is clean Apache-2.0 |
| **3DGS (SOG, LOD, etc.)** | Marginal on iGPU: per-frame GPU radix sort + alpha blending + 200-650 MB VRAM/scene | High: WGSL compute rasterizer + sorter (weeks-months) -- though Brush (Apache-2.0, Rust/wgpu) is a ready reference/dependency | Most mature ecosystem (SuperSplat, Postshot, SOG, streaming LOD) | Skip as a core renderer at our min-spec. If ever needed, vendor Brush's renderer rather than writing our own |
| **2DGS / mesh-extraction (MiLo etc.)** | Output meshes are huge (7-33M verts) and need neural texturing | Moderate-high | Superseded | Skip -- TS+/MeshSplatting dominate this niche |
| **DiffSoup-style (<20K tris + neural texture)** | Very good FPS-wise; needs small MLP in fragment path | Moderate (MLP deferred shading in WGSL) | Brand new (CVPR 2026) | Watch -- attractive for low-budget distant LODs of captured content |
| **SVRaster / other exotics** | Unproven off CUDA | High | Research | Ignore for now |

### Recommendation

The honest answer to "should we adopt the new triangle technology": **not as a world
representation -- our game is an authored, dynamic, lit world, which this family fundamentally
cannot represent yet.** But Triangle Splatting+ / MeshSplatting is genuinely the first
radiance-field tech whose output drops into a plain wgpu mesh renderer at $500-PC
performance. The pragmatic play is a small, contained experiment: train one captured scene
with TS+ on the dev machine, write the trivial importer, and evaluate it as (a) photoreal
static set-dressing/backdrops and (b) a future capture pipeline for reference environments --
while tracking MeshSplatting's "real-time viewer coming soon" and the DiffSoup
textured-triangle direction, which is where the field is clearly heading (opaque, textured,
budget-controlled triangles = exactly what game engines want).

### Key sources

- https://trianglesplatting.github.io/ ; https://arxiv.org/abs/2505.19175 ;
  https://github.com/trianglesplatting/triangle-splatting (+ LICENSE_GS.md)
- https://trianglesplatting2.github.io/trianglesplatting2/ ;
  https://arxiv.org/html/2509.25122v1 ;
  https://github.com/trianglesplatting2/triangle-splatting2
- https://meshsplatting.github.io/ ; https://arxiv.org/html/2512.06818v1
- DiffSoup (CVPR 2026):
  https://en.papernotes.org/CVPR2026/3d_vision/diffsoup_direct_differentiable_rasterization_of_triangle_soup_for_extreme_radian/
- Brush (Rust/wgpu 3DGS, Apache-2.0): https://github.com/ArthurBrussee/brush
- SOG format: https://developer.playcanvas.com/user-manual/gaussian-splatting/formats/sog/ ;
  streaming LOD: https://developer.playcanvas.com/user-manual/supersplat/streaming/
- Splat-to-game pipeline: https://blog.playcanvas.com/turning-a-gaussian-splat-into-a-videogame/ ;
  https://www.tomshardware.com/software/programming/developer-creates-a-basic-first-person-shooter-game-using-gaussian-splats-and-you-can-play-it-for-free-in-your-browser
- Engine-integration state 2026: https://polyvia3d.com/guides/gaussian-splatting-unity-unreal ;
  https://www.strayspark.studio/blog/gaussian-splatting-unreal-engine-5-capture-to-game-pipeline ;
  https://www.magnopus.com/blog/how-i-ended-up-rendering-unlimited-gaussian-splats-in-unreal ;
  https://aukimi.com/blog/gaussian-splatting-production-pipelines-2026
- Large scenes: https://github.com/Linketic/CityGaussian ; https://arxiv.org/pdf/2507.01110 ;
  2DGS: https://arxiv.org/html/2403.17888v1 ; MiLo: https://arxiv.org/html/2506.24096 ;
  SVRaster: https://svraster.github.io/
- Practitioner view incl. Steam Deck:
  https://tech.yahoo.com/gaming/articles/gaussian-splatting-could-game-changer-163000089.html
