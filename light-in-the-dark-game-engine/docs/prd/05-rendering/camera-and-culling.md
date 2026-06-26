# Camera and Culling

> Expands [PRD §5.2](../../PRD.md#52-rendering-g3n-presentation-layer) requirements **R-RND-1** (locked RTS camera) and **R-RND-6** (frustum culling and plane tuning), with input bindings from **R-INP-1** ([PRD §5.4](../../PRD.md#54-audio-ui-input)).
>
> Related: [Batching and Draw Calls](./batching-and-draw-calls.md) · [Materials and Lighting](./materials-and-lighting.md) · [Terrain](./terrain.md) · [Fog of War, Minimap, Selection](./fog-of-war-minimap-selection.md)

---

## 1. Design intent

The camera is the single most leveraged performance decision in the renderer. Warcraft III's own profile showed that its cost problem was *not* true 3D but the absence of culling discipline ([PRD §3.1](../../PRD.md#31-rendering-dimensionality-low-poly-3d-not-25d)). A locked RTS camera makes 3D cheap because the view volume becomes **predictable**: the player can never look at the horizon, never approach a model closely, and never rotate into an unoptimized angle. Every budget downstream — draw calls (R-RND-3), triangle counts (R-RND-2), far-plane distance (R-RND-6) — is sized against this guarantee.

Consequently the camera is **locked by contract, not by convention**. The public API ([PRD §4.3](../../PRD.md#43-api-shape-handles--typed-objects)) exposes camera *position* and *zoom* controls (the WC3 `SetCameraField`-family capability, deduplicated per §4.2), but yaw and pitch ranges are clamped inside `litd/render` and cannot be exceeded by gameplay scripts in v1. Cinematic camera freedom is a tombstoned v2 consideration.

## 2. R-RND-1: The locked RTS perspective camera

### 2.1 Geometry

| Parameter | Value | Rationale |
|---|---|---|
| Projection | Perspective (default) | WC3-faithful depth feel; G3N `camera.NewPerspective` |
| Pitch | **34° from vertical** (i.e. looking down 56° from horizontal), fixed | WC3 default Angle-of-Attack ≈ 304°/−56°; flattening it would expose the horizon and blow the far plane |
| Yaw | **Fixed** (default: map "north" up-screen) | Fixed yaw means unit models, terrain chunks, and impostor-style optimizations only ever need to look good from one angular band |
| Roll | 0, immutable | — |
| FOV (vertical) | 45° default, build-time constant | Matches low-poly asset framing; not player-adjustable in v1 |
| Zoom | Dolly along the fixed view ray, clamped to **[Z_min, Z_max]** | See §2.2 |
| Target point | A point on the terrain plane (the "look-at anchor"); all panning moves this anchor in map-space X/Y | Keeps the frustum footprint a constant-shape trapezoid sliding over the map |

The camera is implemented as a thin controller that owns a G3N `*camera.Camera` (`repoes/engine/camera/camera.go`). G3N's shipped `OrbitControl` and `FlyControl` are **not used** — they allow free rotation. The RTS controller computes the eye position each frame as:

```
eye = anchor + zoom * (sin(pitch)·back_yaw_dir + cos(pitch)·up)
```

with `pitch = 34°` from vertical and `back_yaw_dir` constant. Because pitch and yaw never change at runtime, this is a single fused multiply-add per axis — no trigonometry in the frame loop (precomputed unit offset vector × zoom scalar), which also satisfies the zero-alloc frame constraint **R-GC-1** ([PRD §5.3.1](../../PRD.md#531-go-garbage-collection-discipline)).

### 2.2 Zoom clamps

Zoom distance defaults follow WC3 proportions relative to a 128×128 map with 128-unit cells:

- **Z_default** ≈ 1,650 world units (WC3 default camera distance ~1,650).
- **Z_min** = 0.7 × Z_default — close enough to read unit silhouettes; clamped so the ≤1,500-triangle unit budget (R-RND-2) never looks under-detailed.
- **Z_max** = 1.4 × Z_default — the hard performance ceiling. Z_max directly determines worst-case visible-entity count and is the input to the 500-units-on-screen worst case in [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram). **Raising Z_max requires re-running the M4 render benchmark.**

Mouse-wheel zoom moves between clamps with smoothing applied render-side only (simulation never reads camera state; render never writes sim state — [PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation)).

### 2.3 Panning (from R-INP-1)

Two panning inputs, both standard WC3 muscle memory:

1. **Edge-pan.** When the OS cursor is within an edge band (default 8 px, configurable) of the window border, the anchor pans in that direction at a zoom-proportional speed (`pan_speed = k · zoom`, so screen-space scroll rate feels constant). Corners pan diagonally at normalized speed. Edge-pan is suppressed while a drag-selection box (R-INP-1) is active and the cursor is inside the client area, and when the window loses focus.
2. **Middle-drag.** Holding middle mouse grabs the terrain point under the cursor and drags the anchor so that point stays under the cursor (1:1 map-space dragging, not velocity-based). This requires one ray-vs-terrain intersection on button-down, against the heightfield/tile collision described in [Terrain §5](./terrain.md).

Pan is clamped to a margin-inflated map bounding rectangle so the player can center the screen on map-edge content but cannot scroll into the void. Keyboard arrow-key panning uses the edge-pan code path.

### 2.4 Camera state and interpolation

The camera anchor/zoom are **render-domain state** updated at render rate (60 FPS), not sim state — camera motion must feel immediate and has no gameplay meaning, so it is exempt from the 20 Hz tick quantization (R-EXEC-5). Scripted camera moves issued through the public API (pan-to, shake) are commands enqueued to the render layer and interpolated there.

## 3. Orthographic fallback mode

Behind a startup flag (`-camera=ortho`, also exposed in graphics presets), the renderer swaps `camera.SetProjection(camera.Orthographic)` on the same G3N camera object — G3N supports both projections on one `Camera` (`camera.go`: `SetProjection`, `SetSize`/`UpdateSize`).

Properties of the ortho mode:

- **Look:** pre-WC3 / classic-RTS appearance; no perspective foreshortening.
- **Cost:** marginally cheaper — no perspective-correct interpolation pressure, and crucially a **box frustum**, which makes culling tests and the far plane trivially tight (§5).
- **Zoom semantics change:** dolly distance no longer changes framing, so zoom maps to the orthographic `Size` parameter (G3N `UpdateSize` keeps framing consistent when toggling projections at the same target distance). The same Z_min/Z_max clamps are re-expressed as Size_min/Size_max with identical visible-area footprints, so gameplay balance (how much map a player sees) is **identical in both modes**.
- **Constraint:** all picking/unprojection code must go through G3N's `Camera.Project`/`Unproject`, which are projection-aware, rather than hand-rolled perspective math — this keeps middle-drag, drag-select, and minimap-click correct in both modes.

Ortho is a supported fallback, not a second-class hack: the M4 render benchmark runs in both projections.

## 4. R-RND-6: Frustum culling tuning

### 4.1 What G3N gives us

G3N culls per-graphic against the view frustum by default: `renderer/renderer.go` builds a `math32.Frustum` from the camera matrix each frame (`NewFrustumFromMatrix`) and `classifyAndCull` tests each cullable graphic's geometry `BoundingBox` against it. This is the baseline ([g3n#269](https://github.com/g3n/engine/issues/269)); we do not reimplement it.

### 4.2 What we tune

Default culling is per-node and bounding-box based; with a locked camera we can do much better:

1. **Chunk-level terrain culling.** Terrain is merged into chunks ([Batching §3](./batching-and-draw-calls.md)) sized so that the visible trapezoid at Z_max intersects a bounded, nearly constant number of chunks (target: ≤ 30 of a 128×128 map's chunks visible). Chunk AABBs are static; G3N's per-graphic test handles them, but chunk size is *chosen* against the frustum footprint — see [Terrain §3](./terrain.md) for the sizing analysis.
2. **2D pre-cull in the render-sync pass.** Before touching the G3N scene graph, the render layer's per-frame sync (which copies visible-entity transforms out of sim state) tests each entity's map-space XY against the frustum's **precomputed ground-plane footprint** — a fixed-shape trapezoid (rectangle in ortho mode) translated by the anchor. Entities outside it are never synced, never animated, and their scene nodes are kept detached/invisible. This is cheaper than letting G3N test 1,000+ unit bounding boxes in 3D, skips skinning work for off-screen units entirely, and is allocation-free (pooled visibility lists, R-GC-2). A vertical margin accounts for terrain height and flying-unit altitude so tall cliffs don't cause pop-in.
3. **Cullable flags.** Always-visible full-screen elements (sky backdrop if any, fog-of-war overlay, UI) set `SetCullable(false)` to skip pointless tests; everything else stays cullable.
4. **No LOD in v1.** Asset budgets (R-RND-2) are sized for the worst (closest) zoom; with zoom clamped, LOD adds pipeline complexity for little win. Revisit only if M4 benchmarks miss.

### 4.3 Acceptance hooks

The M4 scripted render benchmark ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)) records visible-graphic counts and culled-graphic counts per frame; a regression that increases visible count at the standard benchmark camera path fails CI alongside the draw-call counter from [Batching §6](./batching-and-draw-calls.md).

## 5. Far-plane strategy

R-RND-6 requires near/far planes tuned so depth precision and culling are tight: "far plane hugs the camera bounding box of the visible map area."

### 5.1 The locked camera makes the far plane computable

With pitch, yaw, FOV, and Z_max fixed, the farthest point the camera can ever need to render is the intersection of the top frustum edge with the lowest terrain elevation (or the map boundary, whichever is nearer). This is a **closed-form constant per map**, computed at map load:

```
far = Z_max + (extent of visible ground past the anchor along the view ray) + margin
```

Concretely, for the 34°-from-vertical pitch and 45° FOV, the far plane lands in the vicinity of `1.6 × Z_max` — roughly 3,700 world units at default tuning — rather than a lazy 10,000+ "skybox" far plane. Tall objects (cliff tops, building roofs, flying units at max altitude) are covered by a fixed vertical margin folded into the constant.

### 5.2 Near plane

The near plane is set as far out as possible: the eye is always ≥ Z_min from the anchor and nothing ever approaches the camera, so `near` is set to a large fraction of Z_min (default `0.25 × Z_min`, ~290 units). Depth-buffer precision is dominated by the near plane, so pushing it out buys far more precision than pulling the far plane in; combined, the near:far ratio stays under ~1:13, leaving a 24-bit depth buffer with abundant precision — no z-fighting mitigations (polygon offset, depth partitioning) should be necessary. Selection circles and other ground decals still use a small polygon offset for coplanarity with terrain, covered in [Fog of War, Minimap, Selection §4](./fog-of-war-minimap-selection.md).

### 5.3 Per-map recomputation and invariants

- Near/far are recomputed on map load and whenever Z_max changes (graphics settings), via G3N `Camera.SetNear`/`SetFar`.
- **Invariant (CI-checked in M4):** no rendered fragment is ever clipped by the far plane on the benchmark camera path at Z_max. The check renders with a debug far plane 2× larger and asserts identical visible-graphic sets.
- In orthographic mode the same computation applies with the box frustum; the far plane is naturally even tighter.

## 6. Interactions with other systems

| System | Interaction |
|---|---|
| [Batching](./batching-and-draw-calls.md) | Terrain chunk size is chosen against the frustum ground footprint; draw-call budget assumes the Z_max worst case |
| [Terrain](./terrain.md) | Middle-drag and click-to-order picking ray-cast against the terrain collision representation; chunk AABBs feed culling |
| [Fog of war / minimap](./fog-of-war-minimap-selection.md) | Minimap viewport indicator is the frustum ground footprint; minimap clicks set the camera anchor |
| Input (R-INP-1) | Edge-pan/middle-drag defined here; drag-select unprojection uses `Camera.Unproject` |
| Public API (R-API-6) | Camera capability exposed without G3N types; clamps enforced inside `litd/render` |

## 7. Worked example: default tuning at a glance

For the default constants (pitch 34° from vertical, FOV 45°, Z_default 1,650, 16:9 aspect), the derived numbers the rest of the renderer designs against:

| Derived quantity | Approx. value | Consumer |
|---|---|---|
| Ground footprint depth (near edge → far edge of visible terrain) at Z_default | ~1,900 world units (~15 cells at 128 units/cell) | terrain chunk sizing ([Terrain §3](./terrain.md)) |
| Ground footprint width at the far edge, Z_max | ~2,600 world units (~20 cells) | worst-case visible-chunk count |
| Visible terrain cells at Z_max | ~300 of 16,384 (128×128 map) — < 2% of the map | culling effectiveness baseline |
| Near plane | ~290 (0.25 × Z_min) | depth precision (§5.2) |
| Far plane | ~3,700 (≈1.6 × Z_max) | far-plane invariant (§5.3) |
| Smallest on-screen unit (1.8 m infantry at Z_max, 1080p) | ~28 px tall | readability floor for Z_max; triangle budget sanity ([Materials §2](./materials-and-lighting.md)) |

These are design-time estimates; M4 freezes the real measured values into the benchmark scene definition so regressions are detected against actuals, not estimates.

## 8. Debugging and tooling

- **Debug camera unlock** (`-camera=free`, debug builds only): switches to G3N's `FlyControl` for inspecting culling behavior from outside the frustum; the frustum of the *locked* camera is drawn as a wireframe (G3N `Lines` graphic) so engineers can see exactly what the player-camera would cull. Never compiled into release builds — the locked-camera contract (§1) is absolute in shipping configurations.
- **Culling HUD counters**: visible vs culled graphic counts, current footprint cell count, and near/far values on the debug HUD, shared with the draw-call counters of [Batching §7](./batching-and-draw-calls.md).
- **Screenshot determinism**: because the camera is fully described by (anchor, zoom, projection mode), benchmark and bug-report screenshots embed that triple, making any view exactly reproducible.

## 9. Open items for M4

1. Exact Z_min/Z_max/FOV constants — tune by feel against WC3 reference footage during M4, then freeze in the benchmark scene.
2. Edge-pan acceleration curve (constant vs eased) — playtest decision, no performance impact.
3. Whether map-edge pan clamping uses a hard stop or soft spring — cosmetic; default hard stop.
4. Whether the ortho fallback is exposed as a user-facing graphics option or remains a developer flag in v1 — leaning user-facing (it is the cheapest "potato mode" lever together with the unlit preset, [Materials §5](./materials-and-lighting.md)).
