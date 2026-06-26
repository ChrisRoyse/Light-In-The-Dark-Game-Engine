# Research: Rendering Dimensionality — 2D vs 2.5D vs Low-Poly 3D

> Expands [PRD §3.1](../../PRD.md#31-rendering-dimensionality-low-poly-3d-not-2d25d).
> Related: [Model Format Selection](model-format-selection.md) · [Asset Sources](asset-sources.md) · [G3N Engine Evaluation](g3n-evaluation.md)

**Decision (restated from the PRD): low-poly true 3D with a locked RTS camera — fixed yaw, pitch-clamped, zoom-clamped perspective projection.** This document records the full analysis behind that decision, including the memory arithmetic that rules out pre-rendered sprites on the 4 GB RAM reference machine.

---

## 1. The three candidate approaches

### 1.1 Pure 2D

All world content is hand-authored or pre-rendered bitmap art composited on a flat plane, with depth faked by draw order (painter's algorithm) and an isometric or top-down tile grid. This is the Starcraft 1 / Age of Empires II lineage.

- **Pros:** trivially cheap per-pixel GPU cost; art style is fully controlled per frame; no skinning, no lighting, no 3D math in the renderer.
- **Cons:** every visual variation — facing direction, animation frame, team color (if baked), upgrade tier, zoom level — multiplies texture memory. Rotating the camera, smooth zoom, height-based line of sight, and missile arcs in true 3D space are all either impossible or faked at great effort.

### 1.2 2.5D (3D terrain + sprite units, or billboarded sprites in a projected world)

A hybrid: 3D terrain mesh with pre-rendered unit sprites billboarded onto it, or fully pre-rendered scenes with 3D-consistent projection. This is the approach analyzed in the [Strike Tactics devblog](https://striketactics.net/devblog/3d-vs-2d-visuals-rts-games) cited by the PRD: render high-detail 3D models offline into sprite sheets, ship the sheets.

- **Pros:** offline rendering allows arbitrarily expensive visual quality (raytraced lighting, high poly counts) at zero runtime GPU cost per model.
- **Cons:** inherits all of 2D's memory multiplication (per direction × per animation × per frame), *plus* an enormous asset-pipeline cost — every model must be staged, lit, rendered from 8+ angles, packed, and re-rendered on any change. Units cannot be lit by in-world dynamic lights (spell glow, day/night), and sprite facing snaps between the 8–32 baked directions.

### 1.3 Low-poly true 3D with a locked RTS camera

Real-time rendering of low-triangle-count skinned meshes, with a perspective camera constrained to a WC3-style overhead view (~34° from vertical, fixed yaw, clamped zoom — [PRD R-RND-1](../../PRD.md#52-rendering-g3n-presentation-layer)). This is what Warcraft III itself did: per the [Hive Workshop analysis](https://www.hiveworkshop.com/threads/what-makes-wc3-graphically-so-heavy.263661/), WC3 is true 3D with very low-poly models for its era, and its performance problems came from *missing culling/LOD optimization*, not from being 3D.

- **Pros:** one mesh + one skeleton + N animation clips serves every facing direction, zoom level, and lighting condition; memory scales with model complexity, not with visual permutations; team color is a shader uniform (R-RND-7), not a baked texture variant; the locked camera makes 3D cost predictable (see §4).
- **Cons:** requires a real 3D pipeline (skinning, frustum culling, draw-call management) and a per-frame GPU cost that 2D avoids. Both are mitigated below.

---

## 2. Memory math: sprite sheets vs low-poly meshes

This is the decisive argument on the **4 GB RAM reference machine** (dual-core 2 GHz, Intel UHD 620 with *shared* system memory — every byte of texture memory competes with the OS, the Go heap, and the simulation). The PRD caps full-match RAM at **1.5 GB** (§5.3).

### 2.1 Pre-rendered sprite sheet cost model

Assumptions, deliberately favorable to sprites:

| Parameter | Value | Note |
|---|---|---|
| Directions | 8 | The classic minimum; facing visibly "snaps". Smooth-looking turns need 16–32. |
| Animation clips per unit | 5 | `Idle`, `Walk`, `Attack`, `Death`, `Spell` — the LitD contractual clip set (R-AST-3) |
| Frames per clip (average) | 10 | ~0.5–1 s clips at 10–20 sprite-fps |
| Frame size | 128×128 px | What a unit occupies on a 1080p screen at default RTS zoom; 64×64 looks dated at modern resolutions |
| Format | RGBA8 (4 B/px) | Alpha is required for non-rectangular units |

Per unit type:

```
8 directions × 5 clips × 10 frames            = 400 frames
400 × (128 × 128 × 4 B)                        = 400 × 65,536 B
                                               ≈ 26.2 MB raw RGBA per unit type
```

For a WC3-scale roster — roughly 100 distinct unit/hero/creep visual types across four factions:

```
100 unit types × 26.2 MB                       ≈ 2.62 GB
```

That alone is **1.75× the entire 1.5 GB match budget and 65% of total machine RAM**, before terrain, buildings, doodads, UI, audio, or the simulation exist. Mitigations don't save it:

- **GPU texture compression (BC3/DXT5, 8:1 over RGBA8):** 2.62 GB → ~330 MB for units only — survivable but still >10× the mesh approach (§2.2), with visible compression artifacts on clean-edged stylized art, and BC formats can't be assumed present on every GL driver tier we target.
- **Drop to 64×64 frames:** 4× reduction → ~655 MB raw. Now the game looks like 1998 *and* still spends ~44% of the match budget on unit sprites.
- **Mipmaps** (needed for clean zoom-out) add +33% to every figure above.
- **Baked team colors** multiply everything by player count unless a palette shader is written — at which point a shader pipeline exists anyway, conceding the main simplicity argument for 2D.
- **16 directions** (the minimum for turns that don't visibly snap) doubles everything.

There is also a non-memory cost the PRD calls out: the **asset pipeline**. Every model must be rendered offline from 8+ angles for every animation; every art revision re-runs the entire bake. For a $0-budget project consuming CC0 packs (see [Asset Sources](asset-sources.md)), this pipeline would be the single largest engineering line item — and the CC0 ecosystem ships *meshes*, not sprite sheets, so the bake tooling would have to be built from scratch.

### 2.2 Low-poly mesh cost model

Using the PRD asset budgets (R-RND-2: units ≤ 1,500 triangles, buildings ≤ 4,000, one shared 1024² atlas per faction/biome):

Per unit type, geometry:

```
~1,000 unique vertices × 56 B                  ≈ 56 KB
  (position 12 B + normal 12 B + UV 8 B + joints 4×u8 + weights 4×f32 ≈ 52–56 B;
   4 joints/weights per vertex matches G3N's MaxBoneInfluencers = 4,
   repoes/engine/graphic/rigged_mesh.go:16)
4,500 indices × 2 B (uint16)                   ≈ 9 KB
                                               ≈ 65 KB geometry
```

Per unit type, animation (30-bone skeleton, 5 clips, ~30 keyframes/channel):

```
30 bones × 3 channels (T/R/S) × 30 keys × ~20 B ≈ 54 KB per clip
5 clips                                         ≈ 270 KB
```

Texture: **shared**, not per-unit. One 1024×1024 RGBA atlas per faction = 4 MB (+33% mips ≈ 5.3 MB), amortized over every unit and building of that faction — the KayKit gradient-atlas pattern the PRD standardizes on (§5.2).

Totals:

```
Per unit type:    65 KB + 270 KB                ≈ 0.34 MB
100 unit types:   100 × 0.34 MB                 ≈ 34 MB
4 faction atlases: 4 × 5.3 MB                   ≈ 21 MB
Units + textures, entire roster                 ≈ 55 MB
```

### 2.3 Comparison

| | Sprite sheets (8-dir, 128², RGBA8) | Sprite sheets (BC3 compressed) | Low-poly meshes |
|---|---|---|---|
| Per unit type | ~26.2 MB | ~3.3 MB | ~0.34 MB |
| 100-type roster | ~2.6 GB | ~330 MB | ~55 MB (atlases included) |
| % of 1.5 GB match budget | 175% | 22% | **3.7%** |
| Facing directions | 8 (snapping) | 8 (snapping) | continuous |
| Dynamic lighting / spell glow | no | no | yes |
| Team colors | baked × N or palette shader | same | 1 uniform (R-RND-7) |
| New unit asset cost | model + rig + offline 8-angle bake | same | model + rig (CC0 packs ship this) |

The mesh approach is roughly **two orders of magnitude cheaper in memory** than raw sprites and ~6× cheaper than aggressively compressed sprites, while removing the direction-count, lighting, and pipeline penalties entirely. On a machine where RAM is the scarcest resource, this is not a close call.

---

## 3. Where the cost moves: GPU, and why the locked camera neutralizes it

3D shifts cost from memory to per-frame GPU work. The locked RTS camera (R-RND-1) is the reason this is affordable on an Intel UHD 620:

1. **Stable view frustum.** Fixed yaw and clamped pitch/zoom mean the visible world area is a known, bounded quad. The far plane can hug the visible map region exactly (R-RND-6) and frustum culling — which G3N performs by default in its render pass (`repoes/engine/renderer/renderer.go:123–128`, `classifyAndCull` at line 212; see [G3N Evaluation §4](g3n-evaluation.md)) — discards everything else cheaply. This directly fixes the failing the Hive Workshop thread identified in WC3 itself.
2. **Predictable overdraw.** A ~34°-from-vertical view of mostly-opaque terrain has near-constant, low depth complexity. There are no first-person corridors or skyboxes filling the screen with layered transparency.
3. **No close-up detail requirement.** The camera never approaches a unit, so 1,500 triangles, a 4-bone-per-vertex rig, and a shared 1024² atlas (downsampled to 256² on the low preset) are visually sufficient. No LOD chain is needed in v1.
4. **Cheap lighting.** One directional sun + ambient (R-RND-4), with the `KHR_materials_unlit` path as the low preset (R-RND-5), keeps fragment cost near the floor.

The remaining GPU risk is **draw-call count** with 500 units on screen, addressed by the ≤300 draw-call budget, shared-material batching, terrain chunk merging, and — if needed — a GPU-instancing patch to the vendored engine (R-RND-3; details in [G3N Evaluation §7](g3n-evaluation.md)).

---

## 4. Engine-fit and ecosystem arguments

- **G3N is a 3D engine.** Its 2D support is incidental — a `Sprite` graphic (`repoes/engine/graphic/sprite.go`) and a sprite-sheet texture animator (`repoes/engine/texture/animator.go`) — not a 2D pipeline with batched quad rendering, sorting layers, or 2D physics. Building a 2D RTS on it means fighting the engine while discarding its actual strengths: the hierarchical scene graph, lighting, skinned-mesh support (`graphic/rigged_mesh.go`, `graphic/skeleton.go`), and the glTF loader.
- **The free asset ecosystem is low-poly 3D.** Quaternius, KayKit, and Kenney ship hundreds of CC0, rigged, animated, glTF-format fantasy/RTS models ([Asset Sources](asset-sources.md)). There is no comparable CC0 library of 8-direction fantasy RTS sprite sheets. For a zero-asset-budget project (G4), this alone is nearly decisive.
- **WC3 fidelity.** The target experience — including height-varying terrain, flying units, missile arcs, and the JASS API's 3D-aware natives (unit fly height, terrain Z queries) — is naturally expressed in true 3D and would need systematic faking in 2D/2.5D.

## 5. Decision and consequences

**Adopted: low-poly true 3D, locked RTS perspective camera** (orthographic available behind a flag as a cheaper fallback, R-RND-1).

Consequences propagated into requirements:

| Consequence | Requirement |
|---|---|
| Triangle/texture budgets replace sprite-sheet budgets | R-RND-2 |
| Draw calls become the scarce resource | R-RND-3, instancing investigation in M3 |
| Camera controller is custom (G3N's `OrbitControl` is free-orbit, wrong shape) | M4 scope; [G3N Evaluation §5](g3n-evaluation.md) |
| Assets must be core-profile glTF with the contractual clip set | R-FMT-1..3, R-AST-3; [Model Format Selection](model-format-selection.md) |
| Render layer reads sim state and interpolates; dimensionality never leaks into the sim | §4.1 layering, R-SIM-1 |

## 6. Sources

- [Hive Workshop — What makes WC3 graphically so heavy](https://www.hiveworkshop.com/threads/what-makes-wc3-graphically-so-heavy.263661/)
- [Strike Tactics — 3D vs 2D visuals in RTS games](https://striketactics.net/devblog/3d-vs-2d-visuals-rts-games)
- [g3n#269 — frustum culling default behavior](https://github.com/g3n/engine/issues/269)
- Vendored G3N source: `repoes/engine/renderer/renderer.go`, `graphic/rigged_mesh.go`, `graphic/sprite.go`, `texture/animator.go`
