# Batching and Draw Calls

> Expands [PRD §5.2](../../PRD.md#52-rendering-g3n-presentation-layer) requirements **R-RND-3** (≤ 300 draw calls/frame), **R-RND-7** (team color via uniform), the instancing work — M3 investigation, **patch planned into M4 per [D-2026-06-11-18](../01-vision/decisions.md)** — from [PRD §7](../../PRD.md#7-milestones)/[§8](../../PRD.md#8-risks), and the per-frame allocation discipline of **R-GC-1..5** ([PRD §5.3.1](../../PRD.md#531-go-garbage-collection-discipline)).
>
> Related: [Camera and Culling](./camera-and-culling.md) · [Materials and Lighting](./materials-and-lighting.md) · [Terrain](./terrain.md) · [Fog of War, Minimap, Selection](./fog-of-war-minimap-selection.md)

---

## 1. The budget and why it exists

**R-RND-3: ≤ 300 draw calls per frame at maximum army size** (the 500-units-on-screen worst case of [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)).

*Revised 2026-06-11 per D-2026-06-11-18:* the low-tier 500-visible-unit budget above is unchanged as the **guarantee**; a **1,000-visible-unit stretch tier** on the recommended-spec machine is added ([Budgets §5.1](../08-performance/budgets-and-benchmarks.md)) under the same 300-call ceiling — which is what turns the §4 instancing patch from a contingency into planned M4 work.

On the reference machine (Intel UHD 620, dual-core 2 GHz), the binding constraint is **CPU driver overhead**, not GPU fill or vertex rate: low-poly content at ≤1,500 triangles/unit leaves the GPU underworked, while each draw call costs state validation and submission time on a weak CPU that is also running the 20 Hz sim. G3N's renderer walks the scene graph and issues one draw per visible `Graphic` per material; without intervention, 500 units + terrain + props + UI would exceed 1,000 calls and the frame budget with it.

The budget allocates roughly:

| Category | Allocation | Mechanism |
|---|---|---|
| Terrain | ≤ 40 | static chunk merging (§3) |
| Units (≤ 500 visible) | ≤ 150 | shared-material batching + instancing (§2, §4) |
| Buildings & doodads | ≤ 50 | shared atlas material; static doodad merging into terrain chunks |
| FX / projectiles | ≤ 30 | pooled billboards, shared materials |
| Fog-of-war, decals, minimap | ≤ 15 | see [Fog of War, Minimap, Selection](./fog-of-war-minimap-selection.md) |
| GUI | ≤ 15 | G3N GUI widgets |

These sub-budgets are engineering targets, not separate gates; the CI gate is the 300 total (§6).

## 2. Shared-material batching

The foundation is asset-side, enforced before runtime: the **single-atlas pattern** ([Materials §3](./materials-and-lighting.md), KayKit-style per [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasyrts-packs-zero-cost)). When every unit of a faction samples one shared 1024² atlas through one shared material, G3N's state-change cost between consecutive unit draws collapses: same shader program, same texture bindings, same material uniforms — only per-object transform (and skinning/team uniforms) change between calls.

Render-layer rules:

1. **One `Material` instance per (atlas, shader-preset) pair**, owned by the asset registry — never one material per model and never per entity. Loaded GLBs are rebound to the shared material at import time; the asset validator ([Validation §2](../06-assets/validation-and-data.md)) rejects models whose textures are not the declared atlas, which is what makes this rebinding safe.
2. **Draw-order grouping.** Opaque graphics are sorted so entities sharing a material render consecutively. G3N's default traversal is scene-graph order, so the render layer parents entities under per-material group nodes; this gets material-sorted submission without patching the renderer's sort.
3. **No per-entity material mutation.** Anything per-entity (team color, flash-on-hit, fade-out) goes through per-graphic uniforms or vertex attributes (§5), never `material.Clone()` — cloning forks the batch key and silently multiplies state changes.

Shared-material batching alone does **not** reduce draw-call count (each skinned unit is still its own call); it reduces the *cost per call* and is the prerequisite for the merging and instancing steps that do reduce count.

## 3. Static geometry merging (terrain and doodads)

Static world geometry is merged offline-style at map load:

- **Terrain chunks.** The terrain (heightmap or tile-composed — decision in [Terrain](./terrain.md)) is baked into chunk meshes of N×N cells, one G3N `Graphic` + one shared terrain material each. Chunk size trades culling granularity against draw count; the sizing analysis lives in [Terrain §3](./terrain.md), with a default of 16×16 cells → ≤ ~30 visible chunks at max zoom given the fixed frustum footprint ([Camera §4.2](./camera-and-culling.md)).
- **Static doodads** (rocks, trees that are not gameplay-destructible, fences) are merged **into their containing chunk's mesh** at map load, inheriting the chunk's culling AABB. They cost zero additional draw calls.
- **Destructible doodads** (WC3 trees that can be harvested/destroyed) cannot be merged permanently. Default approach: merge per chunk, and on first destruction event in a chunk, rebuild that chunk's doodad mesh from pooled buffers (a tree death is rare relative to frame rate; rebuild happens at most once per tick and reuses preallocated vertex buffers per R-GC-2). Fallback if rebuild cost spikes: per-doodad draws within a small per-chunk cap.

Map-load merging is allowed to allocate freely — R-GC-1's zero-alloc constraint applies to steady state only.

## 4. GPU instancing: planned M4 G3N patch, scoped by the M3 investigation

*Revised 2026-06-11 per D-2026-06-11-18: the 1,000-unit stretch target assumes the PRD §8 instancing risk trigger has fired — the patch is **planned M4 work**, not a contingency. The M3 investigation no longer decides* whether *to patch; it decides* how *(above all the skinned-mesh question, §4.3.3).*

### 4.1 Status quo

G3N has **no documented GPU instancing path** ([PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n), risk table [§8](../../PRD.md#8-risks)): `renderer/renderer.go` issues per-graphic draws and the `gls` layer wraps classic `glDrawElements`-style submission. Because the engine is vendored in `repoes/engine`, we can patch it.

### 4.2 Why instancing matters here

An RTS is the canonical instancing workload: hundreds of entities sharing a handful of meshes. With instancing, all visible footmen become **one draw call** carrying a per-instance buffer (transform + team color + animation parameters), turning the unit sub-budget from ~150 calls into ~10–20 (one per distinct visible model type per material). At the D-18 stretch case — 1,000 visible units — instancing is not an optimization but the only way the 300-call ceiling is reachable at all.

### 4.3 Investigation plan (investigation in M3; patch lands in M4)

*Revised 2026-06-11 per D-2026-06-11-18 — step 1 no longer gates whether the patch happens.*

1. **Baseline measurement (still first, repurposed).** Run the M3 benchmark scenes (500 and 1,000 units, max zoom) with batching + merging only. The baseline no longer decides *whether* to patch — 1,000 visible units cannot fit 300 calls without instancing, so the patch is committed M4 work — it sizes *how much* the patch must recover and which content classes (rigid vs skinned) sit on the critical path.
2. **Patch surface survey.** Identify the minimal G3N touch points: `gls` (expose `glDrawElementsInstanced` and `glVertexAttribDivisor` in `gls-desktop.go`/`gls-browser.go`), `geometry.Geometry` (attach an instance VBO with per-instance attribute layout), `graphic` (an `InstancedMesh` type that renders once with a count instead of N `Mesh` nodes), and the shader generator (`renderer/shaman.go` + `renderer/shaders`) to consume instance attributes in the standard/unlit vertex shaders.
3. **Skinned-mesh question.** Per-instance *rigid* transforms are straightforward; per-instance *skinning state* is not (each unit is at a different animation time). Candidate answers, evaluated in this order: (a) baked vertex-animation textures (sample bone matrices from a per-clip texture by instance animation-phase — fits the low preset and low bone counts of CC0 packs); (b) shared-pose cohorts (units in the same clip+quantized-phase share a pose; draws per cohort); (c) instancing for rigid content only (buildings, doodads, projectiles) and per-draw skinned units. Option (c) is the guaranteed-shippable floor.
4. **Prototype + benchmark.** Implement the smallest variant that covers the 1,000-unit stretch case per the baseline; re-run the M3 scenes; record draw calls, frame time, and CPU submission time.
5. **Decision record.** Outcome (variant chosen, skinned-mesh answer, content classes covered) is written into this document and the vendored-fork patch list before the M4 render core builds on it. "Defer entirely" is no longer an outcome (D-2026-06-11-18); the floor is option (c) of step 3 — rigid-content instancing — with skinned coverage recorded as adopted or scheduled.

### 4.4 Patch hygiene

Any instancing patch lives in clearly-marked files/commits in `repoes/engine` (we own maintenance per PRD §8), behind a build tag where feasible, with an upstreamable diff kept rebase-clean.

## 5. R-RND-7: Team color via uniform

Team color must not multiply materials or textures (per-team textures would fork every batch and inflate the asset set 12×).

- **Mechanism.** One `vec3 TeamColor` uniform on the shared unit/building shader. Asset-side, team-colorable regions are authored as a **mask**: either a dedicated atlas region with a flag channel or (preferred, KayKit-compatible) a reserved palette strip in the atlas where the shader detects the team-color UV zone and multiplies/replaces with `TeamColor`. The exact mask convention is fixed in the asset pipeline spec ([Pipeline §4](../06-assets/pipeline.md)) and enforced by the validator.
- **G3N fit.** Per-graphic uniforms are idiomatic in G3N (materials and graphics can add shader uniforms without forking the program). The uniform changes per draw, not per material — the shader program and textures stay bound across the whole faction batch, preserving §2's state-coherence.
- **Instancing interaction.** If the M3 instancing patch lands, `TeamColor` moves from a uniform to a per-instance vertex attribute (or an index into a 12-entry palette UBO) so one draw can carry mixed-team units. The shader is written from day one to read team color through a single function (`getTeamColor()`) so this swap touches one shader block.
- **Cap.** 12 player slots + neutral palette, fixed at build time (WC3 parity).

The same per-graphic-uniform channel carries the other per-entity scalars: hit-flash intensity, corpse fade alpha, and fog-of-war dimming factor ([Fog of War §2.4](./fog-of-war-minimap-selection.md)).

## 6. Per-frame zero-allocation constraints (R-GC-1 applied to the render path)

The batching machinery runs every frame and therefore falls under the zero-alloc steady-state rule. Specific obligations:

1. **Pooled visibility and batch lists.** The per-frame visible-set and per-material batch lists are preallocated slices sized at map load (max entities known from map data, R-GC-2), reset by reslicing to zero length — never reallocated, never `append`-grown past capacity in steady state.
2. **No per-frame scene-graph churn.** Entity scene nodes are created at spawn (a unit-creation burst is an allowed allocation event) and toggled visible/detached on culling, not added/removed per frame. G3N node insertion allocates; visibility toggling does not.
3. **No closures or interface boxing** in the frame sync loop (R-GC-3): the sim→render sync iterates struct-of-arrays component data with index loops, writing into preallocated transform buffers.
4. **Uniform updates are value writes.** Team color / flash / fade updates write into pre-existing uniform locations; no string-keyed map lookups per frame (uniform handles resolved once at material creation).
5. **Destructible-chunk rebuilds** (§3) reuse preallocated vertex/index buffers; a rebuild is a copy into existing capacity.
6. **Instance buffers** (if §4 lands) are persistent, orphaned/updated with `BufferSubData`-style updates from a preallocated CPU mirror.

CI enforcement (R-GC-5): `testing.AllocsPerRun` benchmarks wrap the frame-sync + batch-build path headlessly (GL submission mocked through the `gls` interface), asserting **0 allocs/frame** at steady state.

## 7. Transparent and overlay passes

Opaque batching (§2–§4) covers most of the frame; the remaining passes have their own ordering rules:

- **Blended FX/billboards** render after all opaque content, sorted back-to-front *per material group* (coarse sort — at RTS camera distance per-particle sorting is invisible). Blended content is capped by the FX sub-budget (§1) and FX data-table limits ([Validation §3.3](../06-assets/validation-and-data.md)).
- **Ground decals** (selection circles, blob shadows — [Fog of War §4](./fog-of-war-minimap-selection.md)) render between opaque terrain and blended FX with a small depth bias; they batch by shared decal material exactly like units (§2).
- **GUI** renders last through G3N's GUI pipeline and is excluded from world batching but included in the 300-call count — the budget is the *frame's* budget.
- Pass boundaries are fixed and explicit in the render loop (terrain → opaque world → decals → blended FX → GUI); no per-frame dynamic pass construction, which keeps the loop allocation-free and the call counts attributable per pass in the instrumentation below.

## 8. Worked accounting: the 500-unit worst case

A sanity model of the benchmark frame with batching + merging only (no instancing), at Z_max with a max army on screen:

| Source | Calls | Notes |
|---|---|---|
| Terrain chunks | ~25–30 | ~25–30 visible chunks ([Camera §7](./camera-and-culling.md)), 1 call each, doodads merged in |
| Units (500 visible, 2 factions × ~6 model types) | ~150–500 | **the risk item**: without instancing, skinned units are 1 call each; shared materials make calls cheap but not fewer. If the visible count truly reaches 500, batching alone misses the budget — this is exactly the §4.3 step-1 measurement that sizes the planned instancing patch |
| Buildings + destructible doodads | ~30 | mostly merged or shared-material |
| FX/projectiles | ~20 | pooled billboards, few materials |
| Decals (circles, bars, shadows: Alt held) | ~10 | pooled, shared materials ([Fog of War §4.3](./fog-of-war-minimap-selection.md)) |
| Minimap + fog | ~6 | fog costs 0 ([Fog of War §2.4](./fog-of-war-minimap-selection.md)) |
| GUI | ~15 | G3N widgets |

The honest reading *(Revised 2026-06-11 per D-2026-06-11-18)*: **the 300-call budget at a literal 500 visible units on the low-tier machine — and a fortiori at the 1,000-visible stretch case on the recommended-spec machine — requires the instancing patch**, which is exactly why D-18 plans it into M4 instead of holding it as a contingency. The tiering: **1,000 visible units is the recommended-spec target; 500 remains the low-tier guarantee**, still priced at the [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram) 30 FPS floor rather than 60 ([Budgets §5.1](../08-performance/budgets-and-benchmarks.md)). M3's baseline measurement turns this model into data; the §4.3 record fixes the patch's shape, not its existence.

## 9. Instrumentation and CI gate

- The renderer is instrumented (vendored-fork patch, trivial) with a per-frame **draw-call counter, state-change counter, and visible-graphic counter**, exposed on the debug HUD and dumped by the benchmark harness.
- From M3 onward ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)), the scripted render benchmark fails CI if any frame on the benchmark camera path exceeds **300 draw calls** or if allocs/frame exceed zero.
- The benchmark scene is the shared acceptance artifact for this document, [Camera and Culling §4.3](./camera-and-culling.md), and [Materials and Lighting §6](./materials-and-lighting.md).

## 10. Failure-mode playbook

What to reach for, in order, if the M3/M4 benchmarks miss — pre-agreed so the milestone doesn't stall on debate:

1. **Calls over 300, frame time fine** → tighten first: verify material-group parenting (§2.2) didn't fragment; check for accidental `material.Clone()` (§2.3, add a debug assert that counts live material instances); merge more doodad classes into chunks (§3).
2. **Calls over 300 after tightening** → extend the planned instancing patch (§4.3 — it lands in M4 regardless, per D-2026-06-11-18): rigid content first (buildings/doodads/projectiles/decals), which is low-risk and may alone recover 50–100 calls, then the skinned-mesh question (§4.3.3).
3. **Frame time over budget with calls under 300** → the problem is not draw calls: profile skinning CPU cost (mitigation: animation-rate halving for far units, pose-sharing cohorts) and fill rate (mitigation: blended-FX caps, low preset — [Materials §5](./materials-and-lighting.md)).
4. **Allocs/frame nonzero** → R-GC-5 treats this as a correctness failure, not a tuning matter; the offending path is fixed, never waived.
5. **Last resort** → renegotiate the visible-army worst case with design (tighter Z_max in [Camera §2.2](./camera-and-culling.md) shrinks worst-case visible count quadratically) before renegotiating the 300-call budget itself.

## 11. Summary of mechanisms by milestone

| Milestone | Deliverable |
|---|---|
| M3 | Baseline batching + merging benchmark; instancing investigation (§4.3) scoping the planned patch *(Revised 2026-06-11 per D-2026-06-11-18)* |
| M4 | Team-color uniform path; chunked terrain merging; **instancing patch lands (D-2026-06-11-18)** — 1,000-unit stretch scene inside the 300-call ceiling on the recommended spec; draw-call counter gating CI; both camera projections benchmarked |
| M6 | Full-match budget validation (vertical slice) with all overlays from [Fog of War, Minimap, Selection](./fog-of-war-minimap-selection.md) active |

## 12. Requirements traceability

| Requirement | Where satisfied |
|---|---|
| R-RND-3 (≤ 300 draw calls) | §1 budget allocation; §2–§4 mechanisms; §9 CI gate |
| R-RND-7 (team color via uniform) | §5 |
| R-GC-1/2/3/5 (zero-alloc frame path) | §6 obligations; §9 `AllocsPerRun` gate |
| PRD §8 instancing risk / D-2026-06-11-18 (trigger assumed fired — planned M4 patch) | §4 plan; §10 playbook steps 1–2 |
| R-RND-2 atlas prerequisite | §2.1 (consumes [Materials §3](./materials-and-lighting.md)) |
