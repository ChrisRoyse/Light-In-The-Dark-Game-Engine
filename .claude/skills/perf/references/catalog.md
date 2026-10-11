# Waste-pattern catalog -- full detail (issues #1461, #1485, #1524)

Each item: the mechanism, why it costs, and the fix. File:line anchors are the
original audit findings; read them before touching that path.

## From #1461 (editor UI-thread per-frame waste, OPEN)

1. **Blocking Calyx probe every frame.**
   `crates/litd-editor/src/main_editor_ai_coauthor_ui.rs:9,54` call
   `refresh_calyx_feature()` each frame the panel is visible ->
   `probe_calyx_availability` (`offline_degradation.rs:119-230`): `fs::read_dir`
   on CALYX_HOME, full surface-manifest rebuild, and a blocking
   `TcpStream::connect_timeout` + initialize when an endpoint is configured. Worst
   case: UI stalls up to the probe timeout EVERY frame while Calyx is down.
   Fix: cache probe results, async refresh, never block the UI thread on TCP.

2. **O(project) index rebuild per frame.**
   `main_editor_object_manager.rs:11` calls `build_object_manager_index(...)` every
   frame; `object_manager.rs:119-160` clones every object/region/asset id into
   fresh BTreeSets and rebuilds all entry/reference vectors -> thousands of allocs
   at 60fps to show 8 rows. Fix: rebuild on mutation only (dirty flag / generation
   counter).

3. **Full trigger-graph RON serialization per frame.**
   `main_editor_triggers.rs:96` runs `trigger_graph_stable_ron(graph)` every frame
   only to render a byte-count label. Fix: serialize on change.

4. **Content-browser clones per tile per frame.**
   `main_editor_content_browser.rs:41,59-68`: full `AssetEntry` clone per tile plus
   an eagerly-built 7-line `format!` hover string (evaluated even when not
   hovered). Fix: borrow; build hover string lazily.

5. **McpServer rebuilt per call.**
   `mcp_tools_calyx_surface.rs:241-257`: `source_server()` constructs a new
   `McpServer` + full `register_all` on every live call (twice per readback).
   Fix: construct once, reuse.

Accept gate: in-editor frame-time readout before/after on a synthetic large
project (1k objects, 500 assets) with Calyx unreachable; no frame exceeds budget
due to probe/index/serialize; offline->online still emits the #701 status codes.

## From #1485 (editor per-frame waste #1461 missed, OPEN)

1. **[High] Full GPU drain every frame.** `main_egui_renderer.rs:342-355`:
   `poll(PollType::wait_indefinitely())` + `pop_error_scope` unconditionally --
   serializes CPU/GPU, caps FPS at one pipeline latency. litd-game gated this
   behind `--gpu-validation` in #1460; the editor never did. Fix: same debug gate.

2. **[Med-High] Event loop paced by `thread::sleep`.** `main.rs:383-396`:
   `about_to_wait` sleeps then `request_redraw()`, queuing input behind ~16ms.
   litd-game moved to `ControlFlow::WaitUntil` (`runtime_host.rs:433-456`).
   Fix: WaitUntil pacing.

3. **[Med-High] `EditorWorldRenderEvidence` (~25 allocs + format!) every frame --
   in BOTH editor and game.** `crates/litd-engine/src/render/editor_world_frame.rs:72,222-344`:
   `prepare_frame` clones scene-metadata Strings, ~8 `format!` strings, rebuilds
   reference/contract lists, re-derives `build_day_night_lighting_plan` per frame.
   Consumers: litd-game `runtime_renderer.rs:123` (PLAYER target every frame) and
   editor `main_egui_renderer.rs:273`. Only changes on camera/layer/scene change.
   Fix: cache / rebuild on change; do NOT build evidence in the shipping game's
   steady-state frame. (This is the item that also hits the $500 target.)

4. **[Med] `visible_ranges()` recomputed ~6x/frame.**
   `editor_world_frame.rs:350-365` allocs a fresh `Vec<Range<u32>>` per call with
   identical inputs. Fix: compute once/frame, reuse.

5. **[Low-Med] Per-frame `RendererInfo` rebuild + FSV snapshot clone.**
   `runtime_renderer.rs:98-110,179` and `main_egui_renderer.rs:155-167,382`:
   `info()` clones `adapter_report()` + two `format!("{:?}")` that only change on
   resize; editor also clones the whole `OwnedEditorFsvSnapshot` every frame
   (`main.rs:369`) even when FSV writing is off. Fix: cache; clone snapshot only
   when about to write.

6. **[Low-Med] Object-editor inspector re-clones + re-validates every frame.**
   `main_editor_panels.rs:378-393,409-430`: rebuilds a Vec of (id,name,kind)
   triples over all objects, clones the selected object, runs
   `validate_unit_object_for_lowering` per frame while the Objects inspector is
   visible. Fix: validate/rebuild on edit, cache.

## From #1524 (pre-promotion hygiene, CLOSED -- gate BEFORE these go per-tick)

None run per-tick on the player target today. Fix each before wiring its path
into a per-tick loop; profile allocation count + wall time before/after with
determinism preserved.

1. **ecs `for_each_*` / `_parallel` alloc + spawn per call.**
   `crates/litd-engine/src/ecs/world.rs:122,159,181` clone the full
   `Vec<Entity>` per query; `_parallel` (`:177-192`) does `thread::scope` +
   `scope.spawn` with `archetype.entities.clone()` per archetype per call -- a
   fresh OS-thread launch every query. Fix before hot: `split_at_mut` columns; a
   persistent thread pool (rayon / worker pool), not per-call `thread::scope`.

2. **Render batching eager `format!` + O(n^2) occlusion.**
   `render/batching.rs:79-80,96-101,374-384`: `build_batch_plan` builds a per-
   instance `format!("instances[{i}].screen_rect")` error label eagerly (even on
   the pass path); `is_occluded` scans all occluders per instance (O(inst x occl)
   despite the "HZB" name); `key()` clones mesh+material Strings into a String-keyed
   BTreeMap. Fix: lazy error labels (closure / `map_err`), real hierarchical-Z /
   spatial occlusion, integer/interned batch keys.

3. **Computer-opponent ledger write -> re-read -> SHA-256 -> deep-compare per call.**
   `crates/litd-editor/src/computer_opponents.rs:71,117,143,175-198`. Today only
   called from an FSV example + tests (tool cadence), where write-then-read-back is
   the CORRECT full-state-verification pattern and must stay. ONLY if promoted to a
   per-AI-decision tick should the ledger move in-memory with a cadence flush.
