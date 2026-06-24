# Engine Patch Series

`repoes/engine` is gitignored vendored source. These patches are the tracked source of truth for LitD's G3N fork changes and should apply in lexical order to a clean upstream checkout.

| Patch | Purpose |
|---|---|
| `0001-egl-context-env-fallback.patch` | `G3N_EGL=1` context fallback for headless/WSLg verification. |
| `0002-gltf-interleaved-vbo-and-box3-size.patch` | glTF interleaved VBO attribute offsets and bounding-box size fix. |
| `0003-gltf-implement-khr-materials-unlit.patch` | `KHR_materials_unlit` loader support. |
| `0004-render-frame-stats.patch` | Per-frame draw/state/pass counters for render FSV and gates. |
| `0005-gls-instancing-wrappers.patch` | Go/WebGL wrapper surface for instanced draw calls and attribute divisors. |
| `0006-instanced-mesh.patch` | `graphic.InstancedMesh` with persistent per-instance transform buffers. |
| `0007-team-color-uniform-shader.patch` | Per-graphic team color and presentation scalar shader channel. |
| `0008-vertex-color-base-color.patch` | Vertex color contribution to base color. |
| `0009-instance-team-color-buffer.patch` | Per-instance team-color buffer and shader plumbing for instanced draws. |
| `0010-fog-of-war-shader-term.patch` | Per-fragment fog-of-war texture term (`LITD_FOG`) in both the standard and physical (PBR) shaders: samples the visibility-grid fog texture by world XZ (`ModelMatrix * localPos`, so world-baked terrain and translated unit meshes both fog by world position) and dims in three zones (hidden/explored/visible). Drives `litd/render.FogTerrainMesh` (#161, #536). |
| `0011-gls-framebuffer-delete-wrappers.patch` | `GLS.DeleteFramebuffers` / `GLS.DeleteRenderbuffers` Go wrappers (upstream `gls` ships `GenFramebuffer`/`GenRenderbuffer` but no matching deletes) so offscreen render targets release their GL objects instead of leaking. Drives `litd/render.PortraitTarget` (#193). |
| `0012-renderer-panel-material-no-escape.patch` | Per-panel `GraphicMaterial` pointer no longer escapes (mirror the graphics path: `&mats[0]` into the panel's own slice). Minor — measured ~1 alloc/frame in max-battle (only 1 panel), not the dominant source (#537). |
| `0013-renderer-reuse-zlayers-map.patch` | Reuse the `zLayers` GUI map in place (`delete`-clear, no per-frame `make`). Minor — 1 map/frame (#537). |
| `0014-renderer-reuse-cull-frustum.patch` | Cull frustum allocated once in `NewRenderer`, planes re-set in place each frame via `SetFromMatrix` (was `make([]math32.Plane,6)` per frame). Minor — 1 slice/frame (#537). |
| `0015-renderer-reuse-graphicmaterial-defines.patch` | Reuse `r.specs.Defines` map in place in `renderGraphicMaterial` instead of `make()` per graphic-material per frame. **Dominant** steady-state source: max-battle 433→286 allocs/frame, scene hash unchanged (#537). |

Issue #107 does not add a new engine patch. Its rigid-only instancing floor uses patches `0005`, `0006`, and `0009`, with policy/FSV evidence in `litd/render` and `cmd/renderbench`; skinned GLB sink work remains tracked by #308.
