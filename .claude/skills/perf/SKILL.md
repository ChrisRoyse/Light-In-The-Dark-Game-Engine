---
name: perf
description: "Find and fix performance problems systematically in this Rust/wgpu engine + editor. Use when a frame stutters, FPS drops, the UI stalls, reviewing code for per-frame waste, or making/checking a perf claim. Encodes the waste-pattern catalog (#1461/#1485/#1524), the DUAL budget (editor 5090 vs game $500 PC, #928), frame-time measurement, and perf FSV. Keywords: perf, performance, frame time, stutter, FPS, hot path, per-frame, budget, min-spec."
---

# Find and fix performance problems systematically

Perf work here is evidence-first: a claim needs BEFORE and AFTER numbers from a
repeatable harness, never vibes (see sec 4). And a fix must respect the DUAL
target: the editor may max the RTX 5090, but shipped GAME code must fit a
$500-$1,000 player PC (sec 3). Start by grepping new/changed code against the
waste catalog (sec 1), then measure (sec 2), then apply a quick win (sec 5).

## 1. Waste-pattern catalog -- grep every new hot-path change against this

These are the confirmed per-frame / per-call waste patterns already found and
catalogued (issues #1461, #1485, #1524). Treat this as a CHECKLIST: any code that
runs per frame or per tick must not reintroduce one. File:line anchors are the
original findings -- read them for the exact shape.

| # | Pattern | Smell to grep for | Original finding |
|---|---------|-------------------|------------------|
| 1 | Blocking TCP/network probe on the UI thread | `connect_timeout`, `probe_`, blocking `initialize` in a draw/`ui_` fn | `main_editor_ai_coauthor_ui.rs`, `offline_degradation.rs:119` (#1461-1) |
| 2 | O(project) index rebuild per frame | `build_*_index(` called from a draw loop; fresh `BTreeSet`/`Vec` of all ids each frame | `main_editor_object_manager.rs:11`, `object_manager.rs:119` (#1461-2) |
| 3 | Per-frame RON/serde serialization | `to_ron`, `stable_ron`, `to_string()` of a graph/scene solely for a label | `main_editor_triggers.rs:96` (#1461-3) |
| 4 | Per-frame / per-tile asset clones | `.clone()` of `AssetEntry`/object in a draw loop; eager `format!` hover strings | `main_editor_content_browser.rs:41`, `main_editor_panels.rs:378` (#1461-4, #1485-6) |
| 5 | MCP server re-registered per call | `McpServer::new` + `register_all` inside a per-call `source_server()` | `mcp_tools_calyx_surface.rs:241` (#1461-5) |
| 6 | Full GPU drain + block every frame | `poll(PollType::wait_indefinitely())`, `pop_error_scope` unconditional | `main_egui_renderer.rs:342` (#1485-1) |
| 7 | UI thread paced by `thread::sleep` | `thread::sleep(` in `about_to_wait` instead of `ControlFlow::WaitUntil` | `main.rs:383` (#1485-2) |
| 8 | Per-frame render-evidence / info rebuild | `evidence(...)`, `info()`, `RendererInfo`, `visible_ranges()` rebuilt each frame | `editor_world_frame.rs:72,350`, `runtime_renderer.rs:98` (#1485-3/4/5) |
| 9 | Per-call thread spawn in parallel iterators | `thread::scope` + `scope.spawn` + `.entities.clone()` per query call | `ecs/world.rs:122,177` (#1524-1) |
| 10 | Eager `format!()` in a hot path | `format!("instances[{i}]...")` built even when validation passes | `render/batching.rs:79` (#1524-2) |
| 11 | O(n^2) occlusion / String-keyed batch map | `is_occluded` scanning all occluders per instance; `BTreeMap<String,_>` key clones | `render/batching.rs:96,374` (#1524-2) |
| 12 | Write -> re-read -> hash -> deep-compare per call | ledger write-back on a path about to go per-tick | `computer_opponents.rs:71,175` (#1524-3) |

Note on #9-#12 (#1524): these are NOT on a per-tick path today (example/tool
cadence). The rule is: fix the item BEFORE promoting its path into a per-tick
loop, not speculatively now. For #12 the write-back IS the correct FSV pattern at
tool cadence -- only move it in-memory if it becomes per-decision.

## 2. Measure frame time (the real mechanism)

The engine has a real per-frame profiler, not ad-hoc timers.

- `crates/litd-engine/src/render/ui_perf_overlay.rs` -- `PerformanceOverlayPlan`
  carries `frame_time_ns`, `input_latency_ns`, `over_100ms`, `latency_over_50ms`,
  and per-system `bars` with `budget_ns`/`over_budget`.
  `build_performance_overlay_plan(...)` builds it; `write_performance_overlay_artifacts(...)`
  emits a BMP overlay + a profile JSON readback (`PerformanceOverlayArtifactReadback`
  with `bmp_sha256`). `InputLatencyRecord::latency_ns()` gives input->reaction.
- `crates/litd-editor/src/editor_performance.rs` -- `EditorPerformanceBudget`
  (`target_frame_ms()`), `measure_editor_frame(...)` -> `EditorFrameMetrics`,
  `enforce_editor_performance_budget(...)` (fails on violation). Harness:
  `crates/litd-editor/examples/editor_performance_fsv.rs`.
- Logs (frame stats, exit reasons): GUI binaries log via `tracing` to a FILE, not
  stdout -- `init_file_logging(app)` writes `%LOCALAPPDATA%\LitD\logs\<app>.log`.
  Raise verbosity with `RUST_LOG` (e.g. `RUST_LOG=info,litd_engine=debug`); it is
  honored by `EnvFilter`. Override the path with `LITD_LOG_FILE`. Do NOT read
  stdout expecting frame logs -- it is empty by design. Tail the log file.

For a live HWND/GPU frame-time capture on the real window, this machine IS the
RTX 5090 box -- drive the running editor/game via synapse (process launch ->
screenshot the overlay -> read the profile JSON). See `references/measure.md`.

## 3. The DUAL budget -- do not accept 5090-only evidence for game code (#928, #1470)

Two targets, not in conflict:
- CREATOR (editor/engine): full 9950X3D / RTX 5090 / 128 GB. Max it out.
- PLAYER (produced GAMES): MUST run ~60 fps @ 1080p on a $500-$1,000 PC
  (Ryzen 5 / RX 6600 or GTX 1660 class / 16 GB), scaling up to ultra.

The min-spec budget is code, not a guess:
`crates/litd-engine/src/performance_min_spec.rs` -- `player_min_spec_target()`
= 60 fps, `target_frame_ns = 16_666_666`; `low_player_min_spec_profile()`,
`validate_player_min_spec_profile()`. Quality scale:
`performance_quality.rs` -- `GraphicsQualityTier` (Low..Ultra),
`evaluate_draw_budget`, `max_draw_calls_per_frame`.

RULE (#1470): a perf claim about GAME-side code measured only on the 5090 is a
hypothesis, not evidence -- a 5090 and an RX 6600 can differ 10x. Waste-pattern
#8 (`EditorWorldRenderEvidence`) also lands on the shipping game every frame, so
game-side findings must show min-spec-tier numbers (quality-scale settings on the
5090 are the current stand-in until real min-spec hardware exists -- state which
you used). Editor-only perf may cite 5090 numbers freely.

## 4. Perf FSV -- a claim needs a repeatable harness, before AND after

Per #800: no green test or `Ok` proves a speedup. Requirements:
1. A repeatable harness (an FSV example or the perf overlay) on a fixed workload
   -- e.g. #1461's synthetic large project (1k objects, 500 assets) with Calyx
   unreachable; #1460's frame loop with/without the feature.
2. Capture BEFORE and AFTER on the SAME workload and machine. Paste both numbers
   (frame_time_ns / p-tile / allocation count) into the issue.
3. Assert the budget IN the harness so a regression is a non-zero exit -- do not
   write a verdict and `return Ok(())` (the false-green trap; see the `fsv`
   skill). `enforce_editor_performance_budget` / `evaluate_draw_budget` fail loud.
4. For #1485-style findings, prove the specific thing: the blocking drain gone
   from the steady-state profile, allocation count dropped, evidence Strings no
   longer built in the shipping game's steady frame.

## 5. Quick-wins table (pattern -> typical fix)

| Symptom | Fix |
|---------|-----|
| Rebuilt-every-frame index/serialize | Cache + invalidate on a dirty flag / generation counter; rebuild on mutation only |
| Blocking I/O or TCP on UI thread | Move off the UI thread; cache last result + async refresh; never block draw on the network |
| GPU drain / full poll each frame | Gate behind a `--gpu-validation` debug flag (as litd-game did in #1460) |
| `thread::sleep` pacing | `ControlFlow::WaitUntil` (see `runtime_host.rs:433`) |
| Per-frame clones in draw loops | Borrow instead of clone; build strings lazily (closure / `map_err`), not eagerly |
| Per-call thread spawn | Persistent thread pool (rayon / worker pool); `split_at_mut` column access |
| O(n^2) occlusion, String batch keys | Spatial/hierarchical-Z structure; integer/interned keys |
| Per-frame evidence/info rebuild | Compute lazily; cache; rebuild only on camera/scene/resize change; skip in shipping steady frame |

## References
- `references/measure.md` -- exact commands: run the perf harnesses, enable
  logging, drive the live window via synapse for a real frame-time capture.
- `references/catalog.md` -- the full per-item detail + fix from #1461/#1485/#1524.
