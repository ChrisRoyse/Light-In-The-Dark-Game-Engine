# Measure frame time -- exact commands

## Run the perf harnesses (before/after numbers)

Editor performance budget harness (builds an editor world, measures frame /
load / heavy-edit metrics, enforces the budget, writes an Aster-backed report):

```
cargo run -p litd-editor --example editor_performance_fsv
```
Source: `crates/litd-editor/examples/editor_performance_fsv.rs`. Uses
`editor_performance::{EditorPerformanceBudget, measure_editor_frame,
enforce_editor_performance_budget}`. The budget is enforced in-harness so a
regression exits non-zero.

Game frame-loop harness (drives the real litd-game runtime host as a subprocess,
proves off-thread state persistence + `--gpu-validation`-gated validation +
`WaitUntil` pacing, reads state back from the Aster vault):

```
cargo run -p litd-game --example issue1460_frame_loop_fsv
```
Source: `crates/litd-game/examples/issue1460_frame_loop_fsv.rs`. This is the
reference pattern for asserting every sub-verdict before writing the report.

Render draw-budget governance:
```
cargo run -p litd-editor --example render_budget_governance_fsv
```

Min-spec quality-scale checks live in `performance_min_spec.rs` /
`fsv_quality_harness.rs` (reference-scene GPU render + draw-call budget per tier).

FSV artifacts land under `target/fsv/<issue_slug>/runs/<run_id>/` (see
`fsv_artifacts::issue_root` / `begin_issue_run`) -- read the JSON/BMP there as the
source of truth, not the console line.

## Enable logging and read frame stats

GUI binaries (litd-editor, litd-game) log via `tracing` to a FILE, never stdout.

- Default file: `%LOCALAPPDATA%\LitD\logs\<app>.log` (e.g. `litd-editor.log`).
  Resolved by `init_file_logging(app_name)` in `crates/litd-engine/src/lib.rs:327`.
- Override the path: set `LITD_LOG_FILE`.
- Raise verbosity: set `RUST_LOG`, e.g.
  `RUST_LOG=info,litd_engine=debug,litd_editor=debug`. Honored via `EnvFilter`
  (default for GUI is `warn,litd_engine=info,...`).
- CLI/FSV tools use `init_logging()` (console) with default
  `info,litd_engine=debug,litd_editor=debug,litd_game=debug`.

Tail the log for exit_reason / frame stats; an empty console is NOT "no logging".

## Live frame-time capture on the real window (this machine = the RTX 5090 box)

Subagents run headless and correctly defer GPU/HWND FSV. The MAIN session on this
box can drive the real window and capture true frame time via synapse:

1. Build the binary (it is a binary, not lib):
   `cargo build -p litd-editor --bin litd-editor`
   (or `cargo build -p litd-game --bin litd-game`).
2. Launch and observe via synapse: `process` launch -> `target` list to find the
   HWND -> `screenshot` the in-window perf overlay
   (`ui_perf_overlay` bars show frame_time_ns / budget / over_budget) ->
   `read_text` the overlay values.
3. Also read the profile JSON written by `write_performance_overlay_artifacts`
   (`PerformanceOverlayArtifactReadback`: `bmp_sha256` + profile export) and the
   log file for the frame-time series.
4. Capture the same overlay BEFORE and AFTER the fix on the same scene; paste both.

`LITD_EDITOR_PROJECT_ROOT` is MCP path-confinement, not repo root. For the
editor's ONNX-backed Calyx lens tests, source
`scripts/Set-GameEditorCalyxRuntimeEnv.ps1` first (else `CALYX_LENS_UNREACHABLE`).
