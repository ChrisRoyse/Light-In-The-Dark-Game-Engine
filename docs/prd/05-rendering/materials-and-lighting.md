# Materials and Lighting

> Expands [PRD §5.2](../../PRD.md#52-rendering-g3n-presentation-layer) requirements **R-RND-2** (triangle and atlas budgets), **R-RND-4** (gameplay lighting model), and **R-RND-5** (unlit low preset), grounded in the format constraints of [PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only) (R-FMT-1..3).
>
> Related: [Camera and Culling](./camera-and-culling.md) · [Batching and Draw Calls](./batching-and-draw-calls.md) · [Terrain](./terrain.md) · [Asset Pipeline](../06-assets/pipeline.md) · [Validation and Data](../06-assets/validation-and-data.md)

---

## 1. Philosophy: the camera pays for the materials

Every budget in this document is downstream of the locked camera ([Camera and Culling §1](./camera-and-culling.md)): the player never sees a model closer than Z_min, so per-model detail beyond a fixed screen size is waste; the view direction never changes, so lighting can be a single fixed sun without the scene ever looking flat from a "wrong" angle. The CC0 packs adopted in [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasyrts-packs-zero-cost) were selected because they already fit these budgets natively — the pipeline's job ([Asset Pipeline](../06-assets/pipeline.md)) is to *keep* them fitting, not to optimize them down.

## 2. R-RND-2: Triangle budgets

| Asset class | Budget (triangles) | Notes |
|---|---|---|
| Unit model | **≤ 1,500** | Including all attachments/props in the rig; Quaternius RTS units typically land at 500–1,200 |
| Building | **≤ 4,000** | Buildings are few, large on screen, and static (no skinning cost) |
| Doodad / prop | ≤ 500 (working figure) | Merged into terrain chunks ([Batching §3](./batching-and-draw-calls.md)); cheap individually, numerous collectively |
| Projectile / FX mesh | ≤ 200 | Most projectiles are billboards, not meshes |
| Terrain | budgeted per chunk, see [Terrain §3](./terrain.md) | — |

These are **hard validator gates**, not guidance: the asset-validation CLI ([Validation §2](../06-assets/validation-and-data.md), R-AST-2) counts triangles per GLB and rejects over-budget models at build time. Sizing rationale: worst case 500 visible units × 1,500 tris = 750k triangles plus terrain/buildings — comfortably within Intel UHD 620 vertex throughput at 30 FPS, which is exactly the worst-case floor in [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram). The binding constraint on the reference machine is draw-call CPU cost, not triangles ([Batching §1](./batching-and-draw-calls.md)); triangle budgets exist mostly to bound skinning cost (CPU/GPU vertex work scales with rigged vertex count) and memory.

There is **no LOD system in v1** — budgets are set for the closest clamp (Z_min) and the camera can't get closer. ([Camera §4.2](./camera-and-culling.md).)

## 3. Atlas texture strategy (the KayKit single-atlas pattern)

### 3.1 The pattern

KayKit packs texture 200+ models with **one shared 1024×1024 gradient atlas**: models are UV-mapped onto small flat-color/gradient swatches of the shared sheet rather than carrying individual texture maps. The pattern's virtues map one-to-one onto our requirements:

- **One texture bind for an entire faction/biome** → enables shared-material batching ([Batching §2](./batching-and-draw-calls.md)) and is the prerequisite for the ≤300 draw-call budget.
- **Resolution-independent look** — flat swatches survive brutal downsampling (KayKit documents the 1024² atlas downsampling cleanly to 128²), which gives us nearly-free quality presets (§3.3).
- **Tiny memory** — one 1024² RGBA texture ≈ 4 MB + mips, per faction/biome, against the 1.5 GB full-match RAM budget.

### 3.2 Standardization rule

**R-RND-2 (atlas clause): one shared 1024×1024 atlas per faction and per biome.** Concretely:

- Every unit/building model of a faction references the faction atlas and nothing else; every terrain tile/doodad of a biome references the biome atlas. The validator enforces "model's material references the declared atlas, and only it" ([Validation §2](../06-assets/validation-and-data.md)).
- Assets from packs that ship per-model textures are **re-baked onto the shared atlas in the Blender normalization step** ([Pipeline §3](../06-assets/pipeline.md)) before export.
- The atlas reserves a **team-color zone** (palette strip / mask convention) consumed by the team-color shader uniform — specification shared with [Batching §5](./batching-and-draw-calls.md) (R-RND-7) and fixed in [Pipeline §4](../06-assets/pipeline.md).
- Atlases are embedded in the GLB (binary container, no loose texture files to mismatch) and must be power-of-two for mipmapping.

### 3.3 Downsampling presets

Because the atlas is the only texture, the entire texture-quality axis is one knob:

| Preset | Atlas resolution | Notes |
|---|---|---|
| High | 1024² (authored) | Default on discrete GPUs |
| Medium | 512² | Default on the reference machine |
| Low | **256²** (R-RND-2 floor) | Pairs with the unlit material path (§5) |

Downsampling happens **at load time** from the single authored 1024² source (box-filtered mip selection — we simply upload from a lower mip level), so the shipped asset set contains one resolution and the binary-size budget (≤ 300 MB) is unaffected. No per-preset asset variants exist anywhere in the pipeline.

## 4. Material model: PBR by default, within core-glTF limits

Per R-FMT-1, assets carry only core glTF 2.0 PBR metallic-roughness materials, plus `KHR_materials_unlit` — the one extension G3N supports (`repoes/engine/loader/gltf/khr_materials_unlit.go`). No specular/IOR/clearcoat extensions ([g3n#296](https://github.com/g3n/engine/issues/296)); the validator rejects them.

At runtime, G3N maps these onto its material types (`repoes/engine/material/`: `physical.go` for PBR metallic-roughness, `standard.go`/`basic.go` for simpler paths). Project rules:

1. **Default (medium/high presets):** core PBR metallic-roughness with the shared atlas as base-color map. Metallic = 0, roughness ≈ 0.8–1.0 across the board — low-poly flat-shaded content gains nothing from metalness, and constant factors avoid needing metallic-roughness textures at all (base-color atlas is the only map).
2. **No normal maps, no occlusion maps, no emissive maps in v1.** Flat-shaded low-poly + single sun makes them invisible at RTS camera distance; omitting them halves sampler pressure and keeps GLBs small. Emissive *factors* (constant, textureless) are permitted for glowing FX materials.
3. **Alpha:** opaque everywhere except explicitly flagged FX/foliage materials (alpha-mask preferred over blend; blended materials are draw-order-sorted and capped — see overlay budget in [Batching §1](./batching-and-draw-calls.md)).
4. **One material instance per (atlas, preset) pair** at runtime, never per model — the batching contract from [Batching §2](./batching-and-draw-calls.md).

## 5. R-RND-5: The unlit low preset

The "low" graphics preset bypasses PBR entirely:

- All world materials swap to the **unlit path** (`KHR_materials_unlit` semantics: final color = base-color texture × vertex color × factor). G3N supports this natively in both the glTF loader and its basic material.
- Since CC0 low-poly art carries most of its shading *in the texture/vertex colors already*, the unlit look remains readable — the visual delta is loss of the sun's directional modeling, similar to pre-WC3 sprite-era flatness, and pairs acceptably with the orthographic camera fallback ([Camera §3](./camera-and-culling.md)).
- To keep depth cues, the low preset may apply a **baked top-down shading term**: a single precomputed scalar per vertex (normal·sun, computed once at load) multiplied into vertex color. This restores ~80% of the sun's contribution at zero per-frame lighting cost. Decided by eye during M4; it costs one load-time pass and nothing at runtime.
- Preset switching selects which shared material instance the batches bind — same atlas, same geometry, no asset duplication. Combined with the 256² atlas floor (§3.3), the low preset minimizes both ALU and bandwidth on Intel UHD-class GPUs.
- Fog-of-war dimming and team color must work identically in both paths; both are implemented as factors applied after the lit/unlit branch ([Batching §5](./batching-and-draw-calls.md), [Fog of War §2.4](./fog-of-war-minimap-selection.md)).

## 6. R-RND-4: Lighting model

### 6.1 Gameplay lighting: one sun + ambient, period

The persistent scene contains exactly **one directional light** (the sun) and **one ambient light** — G3N `light.Directional` + `light.Ambient`. Rules:

- Sun direction, color, and ambient level are **map properties** (set in map data, may be retuned per biome/time-of-day theme), constant during a match. No moving sun in v1.
- **No shadow mapping in v1.** Unit grounding is provided by blob-shadow decals (a cheap dark quad in the decal layer, sharing infrastructure with selection circles — [Fog of War §4](./fog-of-war-minimap-selection.md)). Shadow maps would add a full extra scene render pass and blow both the frame and draw-call budgets on the reference GPU.
- With one directional + ambient, G3N's standard/physical shaders stay in their cheapest lighting configuration; the per-fragment cost is a single N·L term.

### 6.2 VFX lights: hard cap of 8

Point and spot lights exist **only as spell/ability VFX** (the WC3 idiom: a fireball casts light, the world does not):

- **Hard cap: ≤ 8 active VFX point/spot lights**, engine-enforced. The render layer owns a fixed pool of 8 G3N light nodes (preallocated, R-GC-2 — lights are recycled, never created at runtime), and FX requests acquire from the pool.
- **Eviction policy** when a 9th light is requested: lowest priority evicted first; ties broken by shortest remaining lifetime, then by distance from screen center. Every VFX light carries a priority class in its FX definition (ultimate > standard spell > ambient flicker).
- All VFX lights have **mandatory finite lifetime and radius** (validator-checked in FX data tables, [Validation §3](../06-assets/validation-and-data.md)) — no permanent point lights can exist, so the steady-state scene always returns to sun+ambient.
- On the **low preset**, VFX lights are ignored entirely (unlit path has no light loop); FX retain readability through their emissive billboards.

### 6.3 Why the cap is 8

G3N's forward renderer uploads all active lights to every lit shader as uniform arrays — per-fragment cost scales with light count across *every* lit object, not just nearby ones. Eight is small enough that the worst-case lit-shader cost stays bounded on Intel UHD 620 while being more simultaneous dynamic lights than WC3 ever showed; M4 benchmarks include an 8-light spell-storm scene to validate.

## 7. Sampling, color space, and filtering policy

Small decisions that prevent large classes of visual bugs, fixed project-wide:

- **Color space:** base-color atlases are sRGB and sampled as sRGB; lighting math in linear; output through G3N's standard pipeline. The unlit path passes sRGB through untouched (authored colors are final colors).
- **Filtering:** trilinear with mipmaps on world materials. Anisotropic filtering off by default on the reference machine, exposed at high preset (the locked 34° pitch makes ground textures moderately oblique; tiles/swatch atlases tolerate it well).
- **Atlas bleed control:** mip generation stops at the level where atlas swatch padding would cross-bleed (padding rules fixed in the atlas authoring spec, [Pipeline §4](../06-assets/pipeline.md)); the loader clamps `maxLod` accordingly. With flat-swatch atlases this is cheap insurance rather than a real risk.
- **Texture units per draw:** worst case binds base atlas + fog texture ([Fog of War §2.4](./fog-of-war-minimap-selection.md)) — two samplers on the unlit path, well under any GL limit and friendly to UHD-class bandwidth.

## 8. Preset matrix (the complete graphics-settings surface)

The full user-facing graphics surface in v1 is intentionally tiny — three presets and one projection toggle:

| | Low | Medium | High |
|---|---|---|---|
| Material path | Unlit (+ optional baked sun term, §5) | PBR, 1 sun + ambient | PBR, 1 sun + ambient |
| Atlas resolution | 256² | 512² | 1024² |
| VFX lights | 0 (ignored) | ≤ 8 | ≤ 8 |
| Blob shadows | TBD (§9.3) | on | on |
| Anisotropic filtering | off | off | on |
| Projection | per separate toggle: perspective (default) / orthographic ([Camera §3](./camera-and-culling.md)) | | |

Everything else (draw-call discipline, culling, fog, decals) is always-on engineering, not a setting. Presets change *which shared materials bind* and *which mip uploads* — never which assets ship ([§3.3](#33-downsampling-presets)).

## 9. Acceptance and CI

- Validator gates (build time): triangle budgets, atlas-only texturing, core-glTF/unlit-only materials — [Validation and Data §2](../06-assets/validation-and-data.md).
- Benchmark gates (M4, CI per [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)): 60 FPS typical / 30 FPS worst case on the reference machine in all four combinations of {PBR, unlit} × {perspective, ortho}, plus the 8-light VFX stress scene.
- Memory gate: texture memory accounted in the 1.5 GB match budget; with the single-atlas rule this is dominated by render targets ([Fog of War §3](./fog-of-war-minimap-selection.md)), not assets.

## 9.1 Shader-source ownership

All custom shading in this document and its siblings concentrates into **one small patch set** on the vendored G3N shader sources (`repoes/engine/renderer/shaders`, generated through `shaman.go`):

- the team-color zone branch ([Batching §5](./batching-and-draw-calls.md)),
- the fog-of-war sample-and-multiply term ([Fog of War §2.4](./fog-of-war-minimap-selection.md)),
- the per-graphic scalar channel (hit-flash, fade), and
- the optional baked-sun term for the low preset (§5).

Each is a clearly delimited, individually toggleable block so the patched shaders stay rebase-able against upstream G3N (same hygiene rule as the instancing patch, [Batching §4.4](./batching-and-draw-calls.md)). No other shader forks are permitted in v1; a feature that seems to need one must come through a PRD amendment. This keeps the entire custom-GPU surface of the project auditable in a single directory diff.

## 10. Requirements traceability

| Requirement | Where satisfied |
|---|---|
| R-RND-2 (triangle budgets, atlas) | §2 budgets + validator gates; §3 atlas rule and 256² low floor |
| R-RND-4 (1 sun + ambient; ≤ 8 VFX lights) | §6.1, §6.2 pool/eviction, §6.3 rationale |
| R-RND-5 (unlit low preset) | §5; preset matrix §8 |
| R-FMT-1/2 (core glTF, unlit-only extension) | §4 material model; validator cross-reference ([Validation §2](../06-assets/validation-and-data.md)) |
| R-GC-2 (preallocated pools) | §6.2 fixed light pool |

## 11. Open items

1. Baked top-down shading term for the low preset (§5) — adopt if the pure-unlit look reads as too flat in M4 review.
2. Per-biome sun presets (warm/cool/night themes) — data-only change, no engine work; needs art direction pass.
3. Whether blob shadows ship in the low preset or are dropped for the absolute floor configuration — measure first.
