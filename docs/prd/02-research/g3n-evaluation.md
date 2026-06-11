# Research: G3N Engine Capability Audit (vendored fork)

> Expands [PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n).
> Related: [Rendering Dimensionality](rendering-dimensionality.md) · [Model Format Selection](model-format-selection.md) · [Asset Sources](asset-sources.md)

This audit is grounded in the **vendored G3N source at `repoes/engine/`** (the fork LitD maintains), not in upstream documentation. Every capability claim below cites the file that implements it. Structure: what G3N provides (§1–§6), what an RTS needs that it lacks (§7), and the patch-the-fork strategy (§8).

---

## 1. Scene graph and core

- **Hierarchical scene graph:** `core/node.go` — `Node` with parent/children, local/world transform matrices (position, quaternion rotation, scale), visibility flag, and name/loader-ID lookup. Everything renderable, lights and cameras included, is an `INode`.
- **Event dispatcher:** `core/dispatcher.go` — subscribe/dispatch on nodes and globally; the GUI and window input are built on it. *LitD usage note:* this dispatcher is render-side only. Gameplay events go through the deterministic sim event system (R-EXEC-2); the G3N dispatcher never carries game state (§4.1 layering rule).
- **Application shell:** `app/` provides the window (GLFW), GL context, and main loop; `window/` abstracts keyboard/mouse input. Build requires cgo + OpenGL headers, as the PRD notes (§3.4).
- **Math:** `math32/` — float32 vectors, matrices, quaternions, `Frustum`, `Ray`. Render-side only; sim math is LitD's own (fixed-point or ordered float, M1 decision, R-SIM-2). `math32.Ray` is the basis for mouse picking (unit selection).

## 2. Materials, lights, shaders

- **Materials** (`material/`): `Basic` (unlit — the R-RND-5 low preset target), `Standard` (+ `NewBlinnPhong`), `Physical` (PBR metallic-roughness — what the glTF loader emits for core materials), `Point`. All support transparency, blending, polygon offset, and per-material `ShaderDefines`.
- **Lights** (`light/`): `Ambient`, `Directional`, `Point`, `Spot` — exactly the set R-RND-4 budgets (1 directional + ambient in gameplay; ≤8 point/spot for spell VFX).
- **Shader management:** `renderer/shaman.go` composes GLSL from templates in `renderer/shaders/` with `#define`-style specialization. The renderer counts scene lights each frame and recompiles/selects programs accordingly (`renderer/renderer.go:132–134` sets `AmbientLightsMax`/`DirLightsMax`/`PointLightsMax`/`SpotLightsMax`). *Implication:* keeping gameplay light counts fixed (R-RND-4) also avoids shader-permutation churn.
- **Custom shaders:** programs and chunks are registered by name (`renderer/shaders/shaders.go`); `gls/program.go` + `gls/shaderdefines.go` allow LitD-authored shaders — the mechanism the team-color uniform (R-RND-7), fog of war, and decals will use (§7).

## 3. Asset loading

Loaders for glTF, OBJ, COLLADA under `loader/`. The glTF loader is the one LitD uses and it is **partial** — full gap inventory (unsupported KHR extensions, no Draco decode despite the declared constant at `loader/gltf/gltf.go:22`, unenforced `extensionsRequired` at `loader.go:61`, single-primitive limit for rigged meshes at `loader.go:208`) lives in [Model Format Selection §2](model-format-selection.md), with the core-profile-only mitigation (R-FMT-1..3) and the `qmuntal/gltf` fallback in §3–4 of that document.

One additional load-time finding: `LoadMesh` re-creates mesh data on node reuse instead of instancing (`loader.go:441`, `// TODO CLONE/REINSTANCE INSTEAD`). LitD's `litd/asset` layer therefore loads each GLB **once** into a prototype and clones nodes itself for the hundreds of same-type units a match spawns.

## 4. Cameras and rendering pipeline

- **Camera** (`camera/camera.go`): one `Camera` type with `Perspective`/`Orthographic` projection modes (`NewPerspective(aspect, near, far, fov, axis)`, `NewOrthographic(...)`) — both R-RND-1 modes covered by one class, switchable behind the planned flag.
- **Camera controls:** only `OrbitControl` (free orbit/zoom/pan, `camera/orbit_control.go`) and `FlyControl` exist. **Neither is an RTS camera.** The locked WC3-style controller (fixed yaw, pitch/zoom clamps, edge-pan, middle-drag — R-RND-1, R-INP-1) is a new, small LitD component driving `Camera` directly; `OrbitControl`'s clamping fields are the reference implementation.
- **Renderer** (`renderer/renderer.go`): per frame it classifies the scene, separating lights, opaque and transparent graphics, **culls against the camera frustum by default** (`renderer.go:123–128`, `classifyAndCull` at `:212`, confirming [g3n#269](https://github.com/g3n/engine/issues/269)), depth-sorts transparents, and issues one draw call per `Graphic`/material. A `Prerender` hook and `postprocessor.go` (fullscreen-quad pass) exist — the natural attachment points for the fog-of-war pass (§7).
- **No batching, no LOD, no occlusion culling.** Draw-call count is strictly (visible graphics × materials), which is why R-RND-3 budgets ≤300 draw calls and mandates shared-material batching and terrain chunk merging in LitD code.

Frame anatomy as the renderer executes it (the structure LitD's frame budget instruments in M4):

1. Traverse scene from the root; reset per-frame light lists (`renderer.go:110–113`).
2. `classifyAndCull` (`renderer.go:212`): partition nodes into lights / opaque / transparent, dropping graphics fully outside the frustum built from the camera's view-projection (`renderer.go:126`).
3. Update shader specs from light counts (`renderer.go:132–134`); `shaman` selects/compiles programs.
4. Render opaque graphics, then depth-sorted transparents; each `Graphic` binds material state and uploads uniforms in `RenderSetup` (for skinned units this includes the bone-matrix palette, `graphic/rigged_mesh.go:52–69`).
5. Optional postprocessor fullscreen pass; then GUI panels render on top via their own `ZLayer` ordering.

## 5. Animation and skinning (verified)

The full skeletal path exists end to end:

- **Skeleton:** `graphic/skeleton.go` — bones are scene-graph `*core.Node`s plus inverse bind matrices (`AddBone`, `:29`); `BoneMatrices(:50)` computes the per-frame palette.
- **Skinned mesh:** `graphic/rigged_mesh.go` — `RiggedMesh` wraps `Mesh`, sets the `TOTAL_BONES` shader define from the skeleton (`:42`), and uploads the bone-matrix array uniform each draw (`:67–69`) into `mBones[TOTAL_BONES]` (`renderer/shaders/include/bones_vertex_declaration.glsl:3`), with **max 4 bone influences per vertex** (`MaxBoneInfluencers = 4`, `rigged_mesh.go:16`) — the asset validator enforces ≤4 influences at export.
- **Loader integration:** `loader/gltf/loader.go` builds the skeleton from glTF skins (`LoadSkin`, `:274`) and attaches it to a `RiggedMesh` per node (`:202–215`).
- **Animation player:** `animation/animation.go` — clip container with play/pause/loop/speed and `Update(delta)`; `animation/channel.go` provides `Position/Rotation/Scale/MorphChannel`s with **STEP and LINEAR interpolation implemented; CUBICSPLINE is an empty TODO** (`channel.go:140`). glTF clips load via `LoadAnimation` (`loader.go:328`).
- **Morph targets:** `geometry/morph.go` + `MorphChannel` + loader support (`loader.go:492–510`) — available, though LitD v1 units only need skeletal clips (R-AST-3).

**Verified limitations to design around:** no animation *blending/crossfade* (one clip drives a channel set; switching `Walk`→`Attack` snaps — acceptable for WC3-style readability, can be patched later, §8); LINEAR-only export rule for assets; one rigged primitive per mesh (loader limit).

## 6. GUI, audio, text

- **GUI** (`gui/`): a complete widget set — `Panel`, `Button`/`ImageButton`, `Label`, `Image`, `Edit`, `Slider`, `Scrollbar`, `List`, `Table`, `Tree`, `Menu`, `TabBar`, `Window`, `DropDown`, `Chart` — with box/grid/dock layouts and three stock styles (`style_dark.go` etc.), rendered in-scene (no external UI dependency). Sufficient for the WC3 HUD: command card = grid of `ImageButton`s, resource bar = `Panel`+`Label`s, minimap = `Image` over a generated texture (§7). Backs R-UI-1.
- **Audio** (`audio/`): OpenAL-based spatial `Player` supporting **WAV and Ogg Vorbis** (`audio/player.go:44`), plus a listener attached to the scene graph — covers R-AUD-1 (`.ogg`-only is an asset policy, not an engine limit).
- **Text** (`text/`): TTF font rasterization to textures, used by the GUI; usable for floating combat text via `Sprite`/texture boards.

## 7. RTS gap list — what LitD must build

None of these are G3N defects; they are genre features no general scene-graph engine ships. Each item below records the gap as verified in the vendored source and the implementation route it will take.

### 7.1 GPU instancing — fork patch candidate (M3, conditional)

The OpenGL bindings already expose everything instancing needs: `glDrawArraysInstanced`, `glDrawElementsInstanced`, `glDrawElementsInstancedBaseVertex`, and `glVertexAttribDivisor` are all loaded in `gls/glapi.c:480–530`. But **no scene-graph object uses them** — `graphic/mesh.go` issues exactly one non-instanced draw per material, and nothing in `renderer/` aggregates same-mesh graphics. This confirms the PRD §3.4 risk note ("no built-in GPU instancing path").

Patch path: an `InstancedMesh` graphic holding one geometry/material plus a per-instance VBO (model matrix + team-color as vertex attributes with divisor 1), submitting a single instanced draw for N units of one type. It slots into the existing renderer untouched because it is just another `IGraphic`. Per R-RND-3, this patch is built in M3 **only if** shared-material batching and terrain chunk merging miss the ≤300 draw-call budget — batching is attempted first because it needs no fork divergence.

### 7.2 Fog of war — LitD `litd/render` + custom GLSL

No visibility/exploration system exists in G3N (nothing to find; this is pure game logic plus a shading term). Design: the sim owns a per-player visibility grid updated deterministically each tick (R-SIM-2 — the grid is gameplay state, replay-affecting). The render layer uploads the local player's grid as a small single-channel texture (128×128 map ⇒ 16 KB, updated at tick rate, not frame rate) and either (a) samples it in the terrain/unit fragment shaders — added via the `shaman` shader-composition mechanism (§2) — or (b) applies it as a fullscreen darkening pass through `renderer/postprocessor.go`. Option (a) is preferred: it lets hidden units be skipped entirely (set invisible from sim visibility, saving their draw calls) rather than merely darkened.

### 7.3 Minimap — LitD `litd/render`

No minimap or render-to-texture helper exists at the scene level, but the pieces do: terrain can be rendered once to an offscreen framebuffer at map-load (top-down ortho camera), and the result displayed in a `gui.Image` (§6). Per-frame overlays — unit dots colored by player, the camera-view trapezoid, ping markers — are drawn as a few dozen tiny quads or a dynamically updated texture. Minimap clicks map linearly to world XY and feed the same order pipeline as world clicks (R-INP-1).

### 7.4 Selection circles and ground decals — LitD `litd/render`

G3N has no decal system. v1 approach: a textured quad (circle texture, alpha-blended, slight polygon offset) placed under each selected unit, conformed to terrain — trivial on flat ground; on cliffs/slopes, a small mesh patch sampling the terrain height field. The renderer's transparent-pass depth sorting (§4) handles ordering. The same mechanism serves AoE targeting reticles, rally-point markers, and blight/buildable-area overlays. Full projected (screen-space or volume) decals are deferred indefinitely — WC3 itself uses the quad approach.

### 7.5 Health bars and floating text — LitD `litd/render`

Two viable routes, both engine-supported: billboarded world-space quads (the `graphic/sprite.go` pattern — a `Sprite` always faces the camera) or screen-space `gui.Panel`s positioned by world→screen projection each frame. Screen-space wins for crisp 1-px bar borders at any zoom; the projection math is `camera.Camera`'s view-projection matrix applied to the unit's interpolated render position. Floating combat text uses `text/` rasterization onto small textures.

### 7.6 RTS camera and input model — LitD `litd/render`

As established in §4, `OrbitControl`/`FlyControl` are the wrong shape. The LitD camera controller implements: fixed yaw, pitch ~34° from vertical with a narrow clamp, zoom clamp, screen-edge pan, middle-drag pan, and map-bounds clamping (R-RND-1, R-INP-1). Drag-select renders a rubber-band rectangle as a GUI overlay; selection itself is resolved sim-side (§7.7). Keyboard handling (control groups 0–9, hotkeys) sits on `window/` key events through the same input layer.

### 7.7 Picking — LitD `litd/sim`

`math32.Ray` exists but G3N ships no scene raycasting utility — and LitD does not want one: picking against render meshes would couple selection to presentation. Instead the mouse ray (unprojected via the camera) is intersected with sim-side unit collision volumes (cylinders/circles on the grid), which is authoritative, deterministic, and cheaper than triangle tests. Render meshes are never picked.

### 7.8 Animation blending — optional fork patch

`animation/Animation` plays one clip; there is no crossfade or layered blending. Switching `Walk`→`Attack` snaps on a keyframe boundary. This matches WC3-era readability and is accepted for v1; if polish demands it, a crossfade is a contained patch in `animation/` (blend two channel evaluations over ~150 ms) with no renderer changes, since channels already write node transforms independently.

### 7.9 Terrain system — LitD `litd/render`

G3N supplies primitive geometries (`geometry/plane.go` etc.) and nothing terrain-specific. Whether Open Question 3 resolves to heightmap cliffs or KayKit-style tiles ([Asset Sources](asset-sources.md)), terrain becomes LitD-built chunked meshes — merged per chunk into one geometry/material to stay inside the draw-call budget (R-RND-3) and culled per chunk by the default frustum pass (§4).

### 7.10 Physics — explicitly unused

G3N's physics lives under `experimental/physics` and is flagged experimental upstream. LitD never touches it: RTS movement, collision, and pathfinding are the deterministic sim's job (PRD §3.4, §5.1, R-SIM-5), and a float32 render-side physics engine inside the state path would violate R-SIM-2 outright.

The PRD risk "API surface underestimation… fog of war, minimap, selection circles" (§8) is therefore bounded: every item above has a known route using stable G3N extension points (custom shaders, postprocessor, render-to-texture, GUI, sprites), all scheduled inside M4 except the conditional instancing patch (M3).

## 7.11 Build and platform requirements (verified)

Practical constraints the audit confirmed, relevant to M0's CI setup:

- **cgo is mandatory.** The GL function loader is C (`gls/glapi.c`, generated by `gls/glapi2go`), and the desktop windowing path uses GLFW via cgo; audio binds OpenAL (`audio/al`) with Ogg Vorbis decoding via `audio/ov`/`audio/vorbis`. Cross-compilation therefore needs a per-target C toolchain — CI builds natively per OS rather than cross-compiling, matching the PRD's "OpenGL driver + GCC-compatible C compiler" note (§3.4).
- **OpenGL core profile**, with entry points resolved at runtime from `glcorearb.h`-generated bindings (`gls/glapi.h`) — consistent with the Intel UHD 620 reference target, whose drivers comfortably cover the required GL level on both Windows and Linux/Mesa.
- **Browser/WASM scaffolding exists** (`gls/gls-browser.go`, `audio/listener-browser.go`, `renderer/version-browser.go`) but is out of scope — desktop-only per Non-Goals.
- **Headless CI:** none of this touches the sim. R-SIM-4's headless tests import only `litd/sim`, so lint/test jobs need no GPU; only the M3+ scripted render benchmark requires a GL-capable runner.

## 8. The patch-the-vendored-fork strategy

G3N's upstream activity is moderate (PRD §3.4 risk table), so LitD treats `repoes/engine` as **owned code**, not a dependency:

1. **Vendored in-repo** — builds never depend on upstream availability; the Go module replaces `github.com/g3n/engine` with the local path.
2. **Patch policy:** fixes land in the fork when they are (a) below LitD's abstraction layers and (b) small relative to working around them — candidates, in expected order: glTF loader fixes (e.g. the `// TODO: BUG HERE` at `loader/gltf/loader.go:577`, `extensionsRequired` validation), `InstancedMesh` (M3, conditional), animation crossfade (post-M6 polish). Each patch gets a `// LITD-PATCH:` marker comment and an entry in `repoes/engine/PATCHES.md` so the diff against upstream stays auditable and upstreamable.
3. **Containment:** R-API-6 (zero G3N types in the public API) plus the §4.1 rule that only `litd/render`/`litd/asset` import the engine keep the fork's blast radius to two packages — the same boundary that makes the [`qmuntal/gltf` fallback](model-format-selection.md#4-fallback-plan-qmuntalgltf-feeding-g3n-directly) a drop-in.
4. **Determinism firewall:** nothing in the fork is on the sim path; render-side patches can never affect state hashes (R-SIM-2, R-SIM-4 headless CI runs without the engine entirely).

## 9. Verdict and capability scorecard

| Capability | Status in vendored fork | LitD requirement served | Action |
|---|---|---|---|
| Scene graph | full (`core/node.go`) | §4.1 render layer | use as-is |
| Materials (PBR/unlit) | full (`material/`) | R-RND-5 presets | use as-is |
| Lights | ambient/dir/point/spot (`light/`) | R-RND-4 budget | use as-is |
| Cameras | perspective + ortho (`camera/camera.go`) | R-RND-1 | use as-is; new controller |
| Frustum culling | default (`renderer.go:212`) | R-RND-6 | tune near/far planes |
| glTF loading | **partial** (`loader/gltf/`) | R-FMT-1..3 | constrain assets; fallback ready |
| Skeletal animation | full path, 4 influences, STEP/LINEAR | R-AST-3 clips | validator enforces export rules |
| Animation blending | absent | polish only | optional fork patch |
| GPU instancing | GL bindings only, no graphic | R-RND-3 | conditional M3 patch |
| GUI | full widget set (`gui/`) | R-UI-1 | use as-is |
| Audio | OpenAL spatial, WAV/OGG (`audio/`) | R-AUD-1 | use as-is |
| Fog of war / minimap / decals / picking | absent (genre features) | PRD §8 risk row | LitD-built in M4 (§7) |
| Physics | experimental | none | never used |

G3N covers the presentation layer LitD actually needs — scene graph, PBR/unlit materials, the exact light set budgeted, perspective/ortho cameras, a verified end-to-end skinned-animation path, default frustum culling, in-scene GUI, and spatial audio — with two real weak points: the partial glTF loader (mitigated by R-FMT-1..3 + fallback) and the absence of every RTS-genre feature (expected; routed per §7). With the engine vendored and the patch policy above, no finding here threatens the M4/M6 milestones.

## 10. Sources

- Vendored G3N source: `repoes/engine/` — files cited inline throughout
- [G3N README](https://github.com/g3n/engine) · [g3n#296 (glTF gaps)](https://github.com/g3n/engine/issues/296) · [g3n#269 (frustum culling)](https://github.com/g3n/engine/issues/269)
