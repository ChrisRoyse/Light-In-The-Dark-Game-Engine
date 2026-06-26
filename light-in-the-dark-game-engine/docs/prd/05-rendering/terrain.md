# Terrain

> Expands [PRD §9.3](../../PRD.md#9-open-questions) (**decided 2026-06-11**: *heightmap + cliffs in v1*, [D-2026-06-11-7](../01-vision/decisions.md), supersedes D-2026-06-11-3) into a full option analysis, the adopted design, and the fallback criteria for the mid-M4 checkpoint. Terrain interacts with nearly every rendering requirement: chunk merging (**R-RND-3**), atlas texturing (**R-RND-2**), culling (**R-RND-6**), and the deterministic pathing grid (**R-SIM-5**).
>
> Related: [Camera and Culling](./camera-and-culling.md) · [Batching and Draw Calls](./batching-and-draw-calls.md) · [Materials and Lighting](./materials-and-lighting.md) · [Fog of War, Minimap, Selection](./fog-of-war-minimap-selection.md) · [Asset Pipeline](../06-assets/pipeline.md)

---

## 1. What terrain must provide (requirements, both options)

Whatever representation wins, terrain must deliver:

1. **A deterministic sim-side ground model** — height and walkability per pathing cell, in fixed-point/ordered math, identical headless and rendered (R-SIM-2/4/5). The *render mesh is derived from sim data*, never the reverse.
2. **WC3-grade gameplay topology** — discrete **cliff levels** (high ground/low ground with gameplay meaning: vision blocking for fog of war, ranged miss chance if ported), ramps connecting levels, unbuildable/unwalkable cells, water.
3. **Chunked static meshes** for the draw-call budget ([Batching §3](./batching-and-draw-calls.md)) and frustum culling ([Camera §4.2](./camera-and-culling.md)).
4. **Atlas-compatible texturing** within the one-atlas-per-biome rule ([Materials §3](./materials-and-lighting.md)).
5. **Ray-pickable surface** for click-orders and middle-drag ([Camera §2.3](./camera-and-culling.md)) — picking runs against the *sim's* height representation, not the render mesh, so picking is deterministic and headless-testable.

## 2. The two candidate representations

### 2.1 Option A — Heightmap mesh with WC3-style cliffs

WC3's actual model: a regular grid of height vertices producing a continuous rolling-terrain mesh, with **cliffs as discrete level transitions** stitched in as special geometry (near-vertical walls and ramp pieces) where adjacent cells differ in cliff level.

- **Geometry:** generated at map load from the map's height/cliff-level arrays. Rolling terrain is a displaced grid; cliff edges select wall/corner/ramp pieces from a small generated-or-modeled transition set (the marching-squares-style case table WC3 used for cliff tiles).
- **Texturing:** **splatting** — N ground textures (grass, dirt, rock, …) blended per-cell via a low-res blend-weight map sampled in the terrain shader. To stay inside the biome-atlas rule, the ground textures are *regions of the biome atlas* and the splat shader blends between atlas sub-tiles (with padding/clamping to prevent bleed), or — simpler fallback — blending is **baked into per-vertex colors / one baked ground texture per chunk** at map load, which costs load time but makes the runtime shader trivial and preset-friendly.
- **Pros:** WC3-faithful look and feel (smooth hills + hard cliffs); height variation is continuous and expressive; map data is compact (two small arrays); deformation (if ever wanted) is natural.
- **Cons:** cliff-transition geometry is the hard part — authoring or generating a clean, atlas-textured wall/ramp piece set is real art+code work the CC0 packs do **not** provide; splat shader is custom; visual quality depends on our own generated geometry rather than proven CC0 art.
- *Revised 2026-06-11 per D-2026-06-11-7/12:* the texture half of that gap (splat sets, cliff texture sets) is now covered by the generative asset pipeline (`tools/assetgen`, R-AST-5, [Pipeline §2.1](../06-assets/pipeline.md)); the transition-geometry work is accepted as planned M4 scope — per the owner's standing directive, features are not cut because they are hard.

### 2.2 Option B — Tile meshes (KayKit style)

The map is a grid of **pre-modeled tile meshes** snapped together — KayKit's Medieval Hexagon pack is exactly this (200+ CC0 tiles: flat, slope, cliff, ramp, river, coast — [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasyrts-packs-zero-cost)), and square-grid equivalents exist (Kenney Hexagon/Castle kits).

- **Geometry:** each map cell references a tile model + rotation; at load, tile meshes are merged per chunk ([Batching §3](./batching-and-draw-calls.md)) into static chunk meshes — after merging, runtime cost is identical to Option A.
- **Texturing:** free — tiles are already mapped onto the single biome atlas; no splatting, no custom shader beyond the shared fog/team terms.
- **Height model:** quantized — heights exist at tile-set granularity (flat / slope / cliff levels), not per-vertex. Cliffs and ramps are *first-class authored tiles*, which is precisely the asset-availability argument from §9.3.
- **Pros:** near-zero art risk (proven CC0 content, looks good immediately); zero custom terrain shader; cliff/ramp problem already solved by the tile set; tile palette = readable, deterministic map data; fastest path to M4.
- **Cons:** terrain reads as "boardgame-quantized" rather than WC3-organic; hex grids complicate a WC3-style square pathing/building grid (see §5 — **square-grid tile sets strongly preferred** over hex for this reason); expressiveness limited to the tile palette; large maps repeat visibly without variant tiles.

## 3. Chunking (common to both options)

- Default **chunk = 16×16 terrain cells**. For a 128×128 map: 64 chunks, of which the locked camera's ground footprint at Z_max intersects ≈ 20–30 ([Camera §4.2](./camera-and-culling.md)) — comfortably inside the ≤ 40-call terrain sub-budget of [Batching §1](./batching-and-draw-calls.md) with one draw per visible chunk.
- Chunk meshes are baked at map load (alloc-permitted), include merged static doodads ([Batching §3](./batching-and-draw-calls.md)), carry a precomputed static AABB for G3N's frustum test, and share the single biome-atlas terrain material.
- Triangle budget per chunk: ≤ ~8k (Option A: 2 tris/cell + cliff pieces; Option B: tile meshes are 10–100 tris each) — worst case ~30 visible chunks × 8k = 240k terrain triangles, minor next to the unit budget ([Materials §2](./materials-and-lighting.md)).
- Destructible-doodad rebuild per chunk follows [Batching §3](./batching-and-draw-calls.md).

## 4. Texturing decision: splatting vs tile atlas

This sub-question collapses with the main one:

| | Option A (heightmap) | Option B (tiles) |
|---|---|---|
| Technique | Atlas-region splatting or load-time baked blends | Direct tile UVs into biome atlas |
| Custom shader work | Splat blend (+ atlas-bleed handling) | None beyond shared fog/team terms |
| Low-preset behavior | Baked variant required (splat ALU on UHD 620 is unwelcome) | Identical to high preset |
| Minimap base | Render from splat/bake ([Fog of War §3.1](./fog-of-war-minimap-selection.md)) | Trivially rasterizable from tile palette (clean CPU fallback) |

Option A is chosen (§6): the **load-time baked** variant is the default texturing path (runtime splatting only if memory says otherwise). The splat and cliff texture sets themselves are produced by the generative pipeline (`tools/assetgen`, R-AST-5 / D-2026-06-11-12, [Pipeline §2.1](../06-assets/pipeline.md)) and validated like any other asset. *Revised 2026-06-11 per D-2026-06-11-7.*

## 5. Pathing-grid alignment with the sim

The pathing grid is the sim's authoritative spatial structure (R-SIM-5, deterministic A*/flow-field) and the fog grid derives from it ([Fog of War §2.1](./fog-of-war-minimap-selection.md)). Alignment rules:

1. **Square pathing grid, always** — WC3 semantics (building footprints, collision sizes, formation movement) assume square cells. This holds *regardless* of render representation; if a hex tile set were used visually, pathing would still be square, creating a permanent visual/logical mismatch — a further argument for square tile sets under Option B.
2. **Resolution:** WC3 uses 4×4 pathing sub-cells per terrain cell; we adopt the same (pathing cell = terrain cell / 4). Walkability, buildability, and cliff level are per-pathing-cell bitfields in map data, fixed at load (mutable only by deterministic gameplay events, e.g. tree death clearing cells).
3. **Height for the sim** is a pure function of map data (height array for A; tile palette + per-tile height profile for B), in fixed-point. The render mesh is generated *from* this function, so a unit standing at sim-height H renders standing on the visible ground by construction. Picking ([§1.5](#1-what-terrain-must-provide-requirements-both-options)) ray-marches this same function.
4. **Cliff levels are sim data** consumed by: pathing (cliff edges unwalkable except ramps), fog-of-war line of sight (higher ground sees over lower — [Fog of War §2.1](./fog-of-war-minimap-selection.md)), and the renderer (which merely draws the corresponding geometry).

## 5.1 Map data formats (representation-dependent surface, shared core)

Map terrain data lives under `data/maps/<map>/` ([Validation §3.2](../06-assets/validation-and-data.md)) and is part of the R-AST-1 data system — loaded once, immutable, hashed into the match fingerprint:

- **Shared core (both options):** map dimensions, biome (selects the atlas — [Materials §3](../05-rendering/materials-and-lighting.md)), per-pathing-cell walk/build/water bitfields, cliff-level array, start locations, doodad placements (`doodads.toml`: asset ID + cell + rotation + destructible flag).
- **Option A adds:** the height array (fixed-point heights per vertex) and the splat/blend-weight map.
- **Option B adds:** the tile-palette grid (`terrain.grid`: tile ID + rotation per cell) and the tile catalog (`tiles.toml`: per-tile height profile, walkability stencil, ramp/cliff classification — this catalog is what makes sim height/walkability a pure function of tile data per §5.3).
- The data validator cross-checks coherence: every referenced tile/doodad asset exists and is validated; cliff-level array consistent with ramp tiles; start locations on buildable ground.

Whether `terrain.grid` is text or packed binary is an open item resolved with the representation choice (text favored for tile palettes — diffable, hand-authorable per criterion 3 below).

## 5.2 Water

Both options treat water identically and minimally in v1:

- **Sim:** water is a per-cell flag (unwalkable for ground units, relevant to amphibious/flying classes) plus a water level per region — pure data, no fluid simulation.
- **Render:** a flat translucent plane per water region at water level, sharing the biome atlas's water swatch, with a cheap two-layer UV scroll for movement. One draw call per visible water region, counted in the terrain sub-budget. No reflections, no refraction, no screen-space effects — WC3-era water on UHD 620 terms.
- Option B tile sets ship shore/river tiles that make water edges look authored for free; Option A requires a shoreline blend in the splat pass (one more argument tallied in §6).

## 6. Decision and mid-M4 fallback criteria

*Revised 2026-06-11 per D-2026-06-11-7 (supersedes D-2026-06-11-3): this section's recommendation is the reverse of the original draft. The owner's standing directive — features are not cut or deferred because they are hard — plus the generative asset pipeline (D-2026-06-11-12) closing the texture-sourcing gap, flips the asset-availability argument that originally favored tiles.*

### 6.1 Decided: **Option A — heightmap mesh with WC3-style cliffs** is the v1 M4 terrain, with the sim designed representation-agnostic

Rationale, in order of weight:

1. **WC3 fidelity is the product bar.** Smooth rolling terrain plus hard cliff levels and ramps is what the engine's reference look demands; the quantized tile aesthetic was always the compromise, and the compromise is no longer forced.
2. **The asset gap that drove the original recommendation is closed.** Option A's hard sourcing problem — splat textures and cliff texture sets with no artist budget — is covered by the generative pipeline (`tools/assetgen`, R-AST-5): generated at asset-build time, hand-curated, committed with provenance ([Pipeline §2.1](../06-assets/pipeline.md)). The cliff-transition *geometry* (the marching-squares wall/ramp case table, §2.1) is accepted as planned M4 engineering work.
3. **Performance is a wash** after chunk merging — both options render as static atlas-textured chunks; the load-time baked texturing path (§4) keeps the low preset clean ([Materials §5](./materials-and-lighting.md)).
4. **Reversibility is preserved in both directions.** Because the sim consumes only the abstract ground model (§5: height/walkability/cliff-level as functions of map data), tile-mesh rendering (Option B, §2.2 — kept fully documented above) remains available behind the same abstraction, as the M4 fallback (§6.2) or a later alternative renderer, without touching gameplay, replays, or the API. Map data formats are versioned accordingly.

### 6.2 Fallback checkpoint (mid-M4) — Option A stands unless these flip it

*Revised 2026-06-11 per D-2026-06-11-7.* The five criteria are retained from the original draft but now guard the **opposite direction**: v1 falls back to square-grid tile meshes only if the heightmap implementation fails them by the **mid-M4 checkpoint**. The evaluation artifact is the same timeboxed (≤ 1 week) exercise: one 64×64 test map rendered with the heightmap path, judged against these gates.

| # | Criterion | Falls back to B (tiles) if… |
|---|---|---|
| 1 | **Generated texture coverage**: do the `assetgen` splat/cliff texture sets cover flats, 2+ cliff levels, ramps, water, shores at acceptable quality? | Generated sets fail curation and hand-authoring the residual gap is unaffordable |
| 2 | **Look check** against WC3 reference: do the generated cliff transitions pass art review at the fixed camera's angular band? | Cliff geometry looks bad from the one view that matters and cannot be iterated to acceptable by mid-M4 |
| 3 | **Map-authoring cost**: height/cliff-level arrays must be hand-authorable (R-AST-1 conventions, [Validation §3](../06-assets/validation-and-data.md)) without a custom editor for M6's skirmish map | Painting a heightmap proves harder in practice than authoring a tile palette |
| 4 | **Budget check**: heightmap chunks inside chunk/draw-call budgets (§3) on the reference machine | The splat/bake path misses a budget the tile path would meet (performance is expected to be a wash; a surprise here overrides aesthetics) |
| 5 | **Pathing fidelity**: generated ramps/cliffs map cleanly onto the square pathing grid with WC3-correct chokepoint behavior | Generated transition geometry fights the pathing grid |

The outcome — the confirmed heightmap approach (cliff-generation case table, chosen texture sets) or an invoked fallback — is recorded in this document. [PRD §9.3](../../PRD.md#9-open-questions) itself is already closed by D-2026-06-11-7; only the fallback question remains open until mid-M4.

### 6.3 The hybrid worth noting (Option C)

The spike should also note (not prototype) the hybrid both WC3-likes eventually converge on: **heightmap rolling terrain + authored tile/mesh pieces for cliffs and ramps only**. It combines A's organic ground with B's solved cliff art, at the cost of implementing *both* systems' loaders. It is explicitly a v2-shaped refinement: the §5 sim abstraction and the §5.1 map-format layering are designed so Option C is an additive renderer/map-format evolution, not a rewrite. v1 picks one of A or B.

## 7. Milestone placement

| When | Deliverable |
|---|---|
| M3 (sim) | Pathing grid, walkability/cliff data model, deterministic height function — representation-agnostic per §5, needed by pathfinding regardless of the render representation |
| Mid-M4 checkpoint | §6.2 fallback evaluation: heightmap path judged against the five gates; fallback to tiles invoked only on failure *(Revised 2026-06-11 per D-2026-06-11-7)* |
| M4 (render core) | Heightmap terrain rendered (tiles only if the §6.2 fallback fires): chunk baking, splat/bake texturing from `assetgen` sets, biome-atlas material, fog term, picking; terrain in the benchmark scene within the ≤ 40-call sub-budget |
| M6 | Skirmish map for the vertical slice authored in the chosen format, validating the authoring-cost criterion in practice |

## 7.1 Risk register (terrain-specific)

*Revised 2026-06-11 per D-2026-06-11-7: A is now the primary path; B rows apply only if the §6.2 fallback fires.*

| Risk | Option | Mitigation |
|---|---|---|
| Generated cliff geometry looks bad at WC3 camera angles | A (primary) | The fixed camera ([Camera §2.1](./camera-and-culling.md)) means cliffs only need to look right from one angular band — the §6.2 checkpoint evaluates exactly that view, nothing more |
| `assetgen` splat/cliff texture sets fail curation | A (primary) | Criterion 1 of §6.2; hand-author a small residual set in the pipeline's Blender/art stage ([Pipeline §3](../06-assets/pipeline.md)); a large failure falls back to B |
| CC0 square tile sets prove thinner than the hex sets (KayKit's flagship is hexagonal) | B (fallback) | Checked only if the fallback fires; gap-filling with a handful of custom tiles is acceptable if the gap is small |
| Tile-mesh merge produces hidden interior faces inflating triangle counts | B (fallback) | Load-time interior-face stripping during chunk baking (adjacent-tile face culling); chunk triangle budget (§3) is the gate |
| Sim/render height mismatch (unit feet floating or sinking) | both | Structural prevention per §5.3 — render mesh generated from the sim height function; an M4 visual test scatters units across every tile/slope type and screenshots for review |
| Map authoring without an editor is too slow for M6 | both | Criterion 3 of §6.2; fallback is a trivial image-to-grid converter (paint the map in any pixel editor, one color per tile/height class) rather than a real editor |

## 8. Requirements traceability

| Requirement / question | Where satisfied |
|---|---|
| PRD §9.3 (decided: D-2026-06-11-7) | §2 option analysis; §6.1 adopted heightmap design; §6.2 mid-M4 fallback criteria |
| R-AST-5 (generative splat/cliff texture sets) | §4 texturing path; §6.1.2; [Pipeline §2.1](../06-assets/pipeline.md) |
| R-SIM-5 (deterministic pathing grid) | §5 alignment rules; §7 M3 placement |
| R-RND-3 (terrain ≤ ~40 calls) | §3 chunking; [Batching §3](./batching-and-draw-calls.md) |
| R-RND-2 (biome atlas) | §4 texturing decision; [Materials §3](./materials-and-lighting.md) |
| R-RND-6 (culling) | §3 static chunk AABBs; footprint sizing from [Camera §7](./camera-and-culling.md) |
| R-AST-1 (map data as tables) | §5.1 map formats; validation in [Validation §3](../06-assets/validation-and-data.md) |

## 9. Out of scope for v1

- Runtime terrain deformation (WC3's `TerrainDeformation` natives) — tombstoned to v2 in the API manifest; representation choice above keeps it *possible* (chunk rebuild path exists for destructibles).
- Texture-painted custom tilesets per map (WC3 tileset modding) — biome atlas swap is the v1 equivalent.
- LOD/geo-mipmapping — unnecessary under the clamped camera ([Camera §4.2](./camera-and-culling.md)).
