# Research: Model Format Selection — glTF 2.0 (GLB) vs OBJ vs COLLADA

> Expands [PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only).
> Related: [Rendering Dimensionality](rendering-dimensionality.md) · [Asset Sources](asset-sources.md) · [G3N Engine Evaluation](g3n-evaluation.md)

**Decision (restated from the PRD): all runtime model assets are glTF 2.0 binary (`.glb`), core profile only, with `KHR_materials_unlit` as the single permitted extension.** This document records the comparative analysis, the precise gaps in G3N's loader that motivate the core-profile-only rules R-FMT-1..3, and the contingency plan built around `qmuntal/gltf`.

## 0. Selection criteria

Ordered by weight, derived from the PRD goals:

1. **Skeletal animation support** — non-negotiable; every unit ships the R-AST-3 clip set (`Idle`/`Walk`/`Attack`/`Death`). A format that cannot carry rigs and clips is disqualified outright.
2. **Load performance on low-tier hardware** — G3's cold-start (≤5 s) and map-load (≤10 s) budgets (§5.3) are dominated by asset I/O and parse cost on a dual-core 2 GHz CPU.
3. **G3N loader maturity** — only formats with an existing in-engine loader qualify (G4: no new middleware); among those, the loader's correctness record matters.
4. **CC0 ecosystem availability** — every pack in [Asset Sources](asset-sources.md) must ship the format natively, or conversion becomes a permanent pipeline tax.
5. **Single-file delivery and hashability** — the asset validator (R-AST-2) and the `assets/_manifest.json` provenance scheme want one content-addressable file per model, not a primary file plus sidecars that can drift apart.
6. **Spec stewardship** — an actively maintained open standard (G4: no proprietary formats; the Non-Goals already exclude Blizzard MDX/MDL).

---

## 1. The candidates

G3N ships exactly three model loaders (`repoes/engine/loader/`): **glTF** (`loader/gltf`), **Wavefront OBJ** (`loader/obj`), and **COLLADA** (`loader/collada`). Only formats with an in-engine loader were considered — adding a fourth format loader from scratch (FBX, MDX) is out of scope by Non-Goal and G4.

### 1.1 Wavefront OBJ

A 1990s plain-text format: vertex/normal/UV lines plus an `.mtl` material sidecar.

- **Fatal gap: no animation, no rigging, no skinning.** OBJ cannot represent a skeleton or an animation clip at all ([Alpha3D comparison](https://www.alpha3d.io/kb/3d-modelling/gltf-vs-obj/)). LitD's contractual per-unit clip set (`Idle`/`Walk`/`Attack`/`Death`, R-AST-3) is unrepresentable. OBJ could only ever serve static doodads — and splitting the pipeline across two formats for no benefit fails G2's simplicity bar.
- **Performance:** ASCII text must be tokenized and parsed float-by-float; indices are per-attribute (position/normal/UV indexed independently) and must be de-duplicated and re-indexed into GPU layout at load time. The [KoreaScience benchmark](https://koreascience.kr/article/JAKO201909258119836.pdf) cited in the PRD places it well behind glTF on both load time and memory; [Threekit's analysis](https://www.threekit.com/blog/gltf-vs-fbx-which-format-should-i-use) measures text formats at roughly 5× the file size and >10× the parse time of GLB.
- **Verdict: rejected** as the primary format. Permitted nowhere in `assets/` to keep the validator (R-AST-2) single-format.

### 1.2 COLLADA (.dae)

Khronos's earlier XML-based interchange format (2004–2008 era).

- **Status: effectively legacy.** Khronos's own successor is glTF; tool export quality is wildly inconsistent (the format's flexibility means every DCC tool writes a different dialect), and modern pipelines treat `.dae` as an interchange/archival format, not a runtime delivery format.
- **Runtime cost:** XML parsing plus an indirection-heavy document model — the worst load-time profile of the three. COLLADA does support skinning and animation, but through a verbose representation that must be fully transformed before it resembles GPU data.
- **G3N support** exists (`loader/collada`) but is the least exercised of the three loaders.
- **Verdict: rejected.** Supported by some CC0 packs (KayKit ships `.dae` variants) but never the only option — every relevant pack also ships glTF ([Asset Sources](asset-sources.md)).

### 1.3 glTF 2.0 / GLB

Khronos's current runtime delivery standard ("the JPEG of 3D").

- **GPU-ready by design.** Mesh data lives in binary buffers laid out as the GPU consumes them — accessors map directly onto vertex buffer objects. G3N's loader does exactly this: `loadAttributes` feeds accessor byte ranges into `gls.VBO`s (`repoes/engine/loader/gltf/loader.go:546`, `addAttributeToVBO` at line 622) with no intermediate mesh representation.
- **Full feature coverage:** skinned meshes (skins, joints, inverse bind matrices), animation clips (translation/rotation/scale/weights channels with STEP/LINEAR/CUBICSPLINE interpolation), morph targets, PBR metallic-roughness materials, scene hierarchy, cameras — everything R-AST-3 and the render layer need, in one file.
- **GLB container:** a single binary file (JSON chunk + binary chunk) — one `open()` per asset, no sidecar texture/`.mtl` path resolution failures, trivially content-hashable for the asset validator.
- **Benchmarks:** fastest load and lowest memory among OBJ/FBX/STL/glTF in the KoreaScience study; ~5× smaller and >10× faster to parse than text formats per Threekit.
- **Ecosystem:** every CC0 pack in [Asset Sources](asset-sources.md) ships glTF; Blender exports it natively and round-trips it cleanly.
- **Verdict: adopted.** `.glb` specifically (not `.gltf` + sidecars) for single-file delivery.

| | OBJ | COLLADA | glTF 2.0 (GLB) |
|---|---|---|---|
| Skinning/rigging | none | yes (verbose XML) | yes (binary, GPU-layout) |
| Animation clips | none | yes | yes |
| Parse cost | high (text) | highest (XML) | lowest (binary chunk) |
| File size | large | largest | smallest |
| Single-file delivery | no (.mtl, textures) | no (textures) | yes (.glb) |
| Spec stewardship | abandoned de facto | legacy Khronos | active Khronos standard |
| G3N loader maturity | basic, adequate | least exercised | primary, but **partial** (§2) |
| CC0 pack availability | common | occasional | universal |

---

## 2. The constraint: G3N's glTF loader is partial

The decision is complicated by the fact that G3N implements a *subset* of glTF 2.0. The gaps below are verified directly against the vendored source (`repoes/engine/loader/gltf/`) and match the upstream report [g3n#296](https://github.com/g3n/engine/issues/296).

### 2.1 Extension support: exactly three, one of them glTF 1.0 legacy

`loader/gltf/gltf.go:22–25` declares four extension name constants; `LoadMaterial` (`loader.go:697`) dispatches on three:

| Extension | Status in vendored loader |
|---|---|
| `KHR_materials_unlit` | **Supported** (`khr_materials_unlit.go`) → maps to `material.Basic`. Deliberately used as LitD's low graphics preset (R-RND-5). |
| `KHR_materials_pbrSpecularGlossiness` | Supported (`khr_materials_pbr_specular_glossiness.go`) — but this extension is itself archived by Khronos; LitD does not use it. |
| `KHR_materials_common` | Supported (`khr_materials_common.go`) — a glTF **1.0** extension, flagged `// TODO ... remove?` in the source. Irrelevant to 2.0 assets. |
| `KHR_draco_mesh_compression` | **Constant declared, no decoder anywhere.** A Draco-compressed asset cannot be loaded. |
| Everything else (`KHR_materials_specular`, `_ior`, `_transmission`, `_clearcoat`, `_emissive_strength`, `_volume`, `KHR_texture_transform`, `KHR_mesh_quantization`, `EXT_mesh_gpu_instancing`, meshopt, …) | **Unsupported.** Any unrecognized extension key on a material makes `LoadMaterial` return `"unsupported extension:%s"` (`loader.go:723`) — a hard load failure. |

### 2.2 The loader does not honor `extensionsRequired`

`ParseJSONReader` carries the comment `// TODO Check for extensions used and extensions required` (`loader.go:61`). The `ExtensionsUsed`/`ExtensionsRequired` arrays are parsed into the document struct (`gltf.go:30–31`) but **never validated**. Consequences:

- An asset requiring an unsupported *material* extension fails late, with a per-material error, only when that material is reached.
- An asset requiring an unsupported *non-material* extension (e.g. `KHR_texture_transform`, Draco on a mesh primitive) may load **silently wrong** — the extension data is simply ignored, producing incorrect UVs or garbage geometry rather than an error.

This silent-corruption mode is the strongest argument for build-time validation rather than trusting runtime errors.

### 2.3 Other verified loader limitations

- **Single-primitive limit for skinned/animated meshes:** `"skinning/rigging meshes with more than a single primitive is not supported"` (`loader.go:208`) and the same restriction for animation targets (`loader.go:370`). Multi-material rigged characters must be merged to one primitive at asset-prep time.
- **CUBICSPLINE interpolation is a TODO:** `animation/channel.go:140` falls through without evaluating cubic-spline keys. Exporters must emit STEP or LINEAR keyframes.
- **Morph target weights/names from the file are TODOs** (`loader.go:496–497`); interleaved animation accessors are unhandled (`loader.go:376`).
- Mesh nodes are re-loaded rather than instanced on reuse (`loader.go:441`, `// TODO CLONE/REINSTANCE INSTEAD`) — relevant to load time, handled by LitD's own asset cache, see [G3N Evaluation §3](g3n-evaluation.md).

---

## 3. Mitigation: the core-profile-only rules (R-FMT-1..3)

Rather than patching the loader toward full spec coverage (open-ended work), LitD **constrains the assets** to the subset the loader handles correctly, and enforces the constraint mechanically:

- **R-FMT-1 — Core glTF 2.0 GLB only.** No KHR extensions except `KHR_materials_unlit`. Materials are core PBR metallic-roughness or unlit; textures use default UV mapping (no `KHR_texture_transform`); geometry is uncompressed float attributes (no quantization).
- **R-FMT-2 — Build-time validation, not runtime failure.** The asset-validation CLI (R-AST-2) parses every candidate `.glb` and **rejects** any file whose `extensionsUsed`/`extensionsRequired` arrays contain anything outside the allowlist `{KHR_materials_unlit}` — closing the silent-corruption hole from §2.2 before assets enter `assets/`. The same tool enforces the single-primitive rule for rigged meshes, STEP/LINEAR-only animation sampling, triangle budgets (R-RND-2), and clip-name contracts (R-AST-3).
- **R-FMT-3 — No Draco/Meshopt compression in v1.** G3N cannot decode it (§2.1), and the math doesn't justify a decoder: a ≤1,500-triangle unit is ~65 KB of geometry ([Rendering Dimensionality §2.2](rendering-dimensionality.md)); the entire 100-type roster's geometry fits in ~35 MB against a 300 MB binary+assets budget (§5.3). Compression would buy megabytes and cost a native decoder dependency — violating the pure-Go spirit and adding a determinism-irrelevant moving part.

The complete validator rule set, consolidated (each rule maps to a loader gap or PRD budget):

| Validator check | Guards against | Source of the constraint |
|---|---|---|
| `extensionsUsed`/`extensionsRequired` ⊆ `{KHR_materials_unlit}` | hard load failure (`loader.go:723`) or silent corruption (§2.2) | R-FMT-1 |
| No Draco/Meshopt-compressed buffer views | undecodable geometry | R-FMT-3, §2.1 |
| Rigged meshes: exactly 1 primitive | loader rejection at `loader.go:208` | §2.3 |
| Animation samplers: STEP or LINEAR only | CUBICSPLINE no-op (`channel.go:140`) | §2.3 |
| ≤4 joint influences per vertex | G3N `MaxBoneInfluencers = 4` (`graphic/rigged_mesh.go:16`) | [G3N Evaluation §5](g3n-evaluation.md) |
| Triangle count: units ≤1,500, buildings ≤4,000 | frame budget on Intel UHD 620 | R-RND-2 |
| Clip names: `Idle`,`Walk`,`Attack`,`Death` (+opt. `Spell`,`Portrait`) | missing-animation runtime fallbacks | R-AST-3 |
| Textures: PNG/JPEG only, ≤1024², referenced atlas | memory budget; loader image support (`loader.go:809`) | R-RND-2 |
| GLB container, glTF version 2.x | `"GLB version:%v not supported"` (`loader.go:97`) | R-FMT-1 |

Asset-prep workflow implied by these rules: CC0 source files (any format) → Blender import → cleanup/merge to single primitive → export GLB with extensions disabled, +Y up, LINEAR sampling → validator → `assets/`. The validator runs in CI on every change to `assets/`, so a regression in an updated pack download is caught at PR time, never at runtime (R-FMT-2's "build time, not runtime" principle).

---

## 4. Fallback plan: `qmuntal/gltf` feeding G3N directly

Risk register entry "G3N glTF gaps (skinning edge cases, extensions)" (PRD §8) is rated Medium/High. If core-profile assets still hit loader bugs (e.g. the `// TODO: BUG HERE` marker in attribute conversion at `loader.go:577`, accessor edge cases, or skinning defects discovered in M4), the fallback — suggested in [g3n#296](https://github.com/g3n/engine/issues/296) itself — is to **bypass G3N's parser while keeping G3N's renderer**:

1. **Parse with [`qmuntal/gltf`](https://github.com/qmuntal/gltf)** — a maintained, pure-Go (no cgo), spec-complete glTF 2.0 reader/writer with typed access to buffers, accessors, skins, animations, and extension payloads, plus a `modeler` package for decoding accessors into Go slices.
2. **Bridge into G3N types**, which are all public and constructible without the G3N loader:
   - accessors → `math32.ArrayF32`/`ArrayU32` → `geometry.NewGeometry()` + `gls.VBO` attributes;
   - materials → `material.NewPhysical()` / `material.NewBasic()`;
   - skins → `graphic.NewSkeleton()` + `AddBone(node, inverseBindMatrix)` (`repoes/engine/graphic/skeleton.go:29`) wrapped by `graphic.NewRiggedMesh(mesh)` (`graphic/rigged_mesh.go:26`);
   - animation samplers → `animation.NewAnimation()` with `Position/Rotation/Scale/MorphChannel`s (`repoes/engine/animation/channel.go`).
3. The bridge lives in `litd/asset`, so the swap is invisible above that layer (the public API has zero G3N types in signatures, R-API-6).

This fallback also lifts the single-primitive restriction (§2.3) if merging at asset-prep time ever proves insufficient, and — because the engine is vendored in `repoes/engine` — any *renderer-side* skinning fix can be patched in the fork independently of the parser choice ([G3N Evaluation §8](g3n-evaluation.md)).

**Decision gate:** the M0 asset-validation pass over the downloaded packs plus the M4 render-core milestone exercise the stock loader end-to-end; the fallback is triggered only if a core-profile asset class fails there.

**Future watch (post-v1, non-binding):** if asset volume ever makes R-FMT-3 a real cost, the revisit order is Meshopt (`EXT_meshopt_compression` — pure-Go decode is feasible) before Draco (C++ decoder, cgo); and if texture memory becomes the constraint, KTX2/BasisU (`KHR_texture_basisu`) is the standard path — all three require the §4 fallback parser first, since the stock loader rejects their extension names today (§2.1).

## 5. Sources

- [g3n#296 — partial glTF 2.0 extension support; qmuntal/gltf suggested](https://github.com/g3n/engine/issues/296)
- [KoreaScience — 3D file format performance study](https://koreascience.kr/article/JAKO201909258119836.pdf)
- [Threekit — glTF vs FBX](https://www.threekit.com/blog/gltf-vs-fbx-which-format-should-i-use)
- [Alpha3D — glTF vs OBJ](https://www.alpha3d.io/kb/3d-modelling/gltf-vs-obj/)
- [qmuntal/gltf — pure-Go glTF 2.0 library](https://github.com/qmuntal/gltf)
- Vendored G3N source: `repoes/engine/loader/gltf/{gltf.go,loader.go,khr_materials_unlit.go}`, `animation/channel.go`, `graphic/{rigged_mesh.go,skeleton.go}`
