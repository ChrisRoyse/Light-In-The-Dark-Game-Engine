# Live-app FSV recipe (synapse) -- full flow and gotchas

Main session only. This box has an RTX 5090 + 2 displays and IS the hardware-FSV
box that gates like #1485 / #1146 / #1150 / #928 require. Subagents launched via the
Agent tool are HEADLESS (no GPU/display) and correctly refuse GPU-backed render
paths -- they must DEFER such claims to the main session, not fake them.

## Editor (litd-editor): HWND / GPU / pixel / exit-reason

The editor boots and renders end-to-end (RTX 5090, Vulkan, 60 FPS, real Ashen
world); the old #1531 "silent exit / no window" symptom is resolved (closed #1531).

1. Rebuild at HEAD. It is a BINARY (main.rs), not a lib -- verify with the bin:

   ```powershell
   cargo build --bin litd-editor        # ~17s incremental
   ```

2. Launch on the interactive desktop:

   `mcp__synapse__process` operation=launch
   - target = C:\code\GameEditor\target\debug\litd-editor.exe
   - working_dir = C:\code\GameEditor
   - wait_for_window_title_regex = "(?i)light in the dark|litd|editor"
   - timeout_ms = 90000   (GPU + world init takes ~30s; use a generous timeout)

   Returns real `hwnd`, `pid`, and `matched_title`
   (e.g. "Light in the Dark Editor - world view").

3. Prove the window is real and on the interactive desktop:

   `mcp__synapse__target` operation=list, process_name_contains=litd
   -> enumerates the HWND via EnumWindows.

4. Capture real rendered viewport pixels + sha256:

   `mcp__synapse__screenshot` path=<abs>.png, window_hwnd=<hwnd>

5. Clean shutdown that LOGS the exit reason -- WM_CLOSE, NOT force kill:

   ```powershell
   taskkill //PID <pid>        # NOT /F -- /F skips the exit-reason log
   ```

   Then read the log (see below); expect
   `exit_reason="close_requested" clean_shutdown=true exit_code="success"`.

### Forcing a loud failure to prove the failure path logs
`LITD_EDITOR_PROJECT_ROOT` is the MCP path-confinement root (#1518), NOT the repo
root, and a bad value does NOT fail boot. The real loud failure is a renderer /
shell-state store init error: it logs `renderer_init_failed` + `exit_code="failure"`.

## Game host (litd-game): synthetic-package end-to-end run

The runtime host requires `--package <path>` and errors without one
(`load_runtime_locked_package`), so it long looked un-runnable. The synthetic-package
technique unblocks it: build a package in-code and run the real host for N frames,
then read back the runtime-state Aster store. Reference implementation:
`crates/litd-game/examples/issue1460_frame_loop_fsv.rs`
(`package_value(share_id, wav_path)` writes a `*.litdgame.json`; the host runs with
`--package`, `--runtime-state`, `--hidden-window`, `--max-frames`, optional
`--gpu-validation`).

```powershell
cargo run -p litd-game --example issue1460_frame_loop_fsv --features fsv
```

Real GPU host, 120 frames, `clean_shutdown=true`, and an independent physical Aster
WAL sha256 readback asserted equal to the row sha256. The same `package_value`
technique regenerates a stale runtime-state row by running the current binary
directly: `litd-game --quality low --runtime-state <path> --package <synthetic>`.

## Reading the log (both binaries)
Default path `%LOCALAPPDATA%\LitD\logs\<app>.log` (override `LITD_LOG_FILE`; no CWD
fallback since #1492). stdout/stderr are empty by design.

```powershell
Get-Content -Tail 40 "$env:LOCALAPPDATA\LitD\logs\litd-editor.log"
Select-String -Path "$env:LOCALAPPDATA\LitD\logs\litd-game.log" `
  -Pattern "exit_reason|clean_shutdown|gpu context initialized|renderer_init"
```

Filter env is `RUST_LOG` / `EnvFilter`; default
`warn,litd_engine=info,litd_editor=info,litd_game=info`. The log records adapter /
device init, surface config, shell-state load, and the final
`... returning exit_code=... last_exit_reason=...`.

## Build note
Never `cargo clean` the warm target/ tree (it is a cache to retain). If a build feels
slow, check `target\debug\incremental` size first -- it is capped at 10GB by the
daily GC. Never disable incremental (20s vs 93s on litd-editor).
