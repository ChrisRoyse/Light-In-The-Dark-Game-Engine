---
name: reality-loop
description: Instant is-it-working-in-reality feedback while coding this Rust/wgpu engine + editor. Use after every edit to KNOW a change works, not just compiles: a tiered watch loop (cargo check first, escalate on pass), a persistent litd-editor/litd-game probe launched once and re-probed, and structured log-line verdicts from the file log. Triggers - reality loop, does it work, fast feedback, edit rebuild probe, tail logs, FSV while coding.
---

# reality-loop

Collapse the time between writing code and KNOWING it works in reality (issue #1166
Loop 1: the agent loop is programmatic, headless, machine-verdicted). Compiling is not
working; a green `cargo test` is not a rendered frame. This skill keeps one hot shell and
one running process so each edit costs seconds, not a cold relaunch.

Tier definitions live in `[[fsv-fast]]` - do not duplicate them here. When a build turns
slow or throws phantom errors, jump to `[[build]]`. The final full pass is `[[fsv]]`.

## 0. Open a hot shell ONCE (env pre-sourced)

Any code touching the Calyx A38 ONNX lens needs the runtime env or it throws
`CALYX_LENS_UNREACHABLE` (that is missing env, NOT a regression - #1493). Source it once
per session and reuse the shell:

```powershell
cd C:\code\GameEditor
. .\scripts\Set-GameEditorCalyxRuntimeEnv.ps1        # ORT_DYLIB_PATH, CALYX_ORT_CAPI, CALYX_HOME
$env:LITD_EDITOR_PROJECT_ROOT = "C:\code\GameEditor"  # MCP path-confinement root (#1518)
```

## 1. Tiered watch loop - cheapest tier first, escalate only on PASS

After each edit run the fastest tier; climb a tier only when the current one is green.
Do NOT jump straight to a full build/test - a compile error caught by `cargo check` in
~20s should never cost a 90s test cycle.

```powershell
# Tier 0 (fastest, ~20s incremental): type-check only the crate you touched.
cargo check -p litd-engine
```

Know what your build command actually compiles. The editor UI (`EditorShellRuntime`,
`main_editor_*.rs`, `main_egui_renderer.rs`) is BINARY, not lib - `--lib` does NOT compile
it (burned #1461). After any edit there, check the binary:

```powershell
cargo check -p litd-editor --bin litd-editor   # NOT --lib for UI/shell changes
```

Escalate through the tiers in `[[fsv-fast]]` (check -> targeted test -> example/FSV ->
live process). NEVER set `CARGO_INCREMENTAL=0` to "fix" slowness - it is 4.6x slower here
(20s vs 93s); if a build is slow, that is a `[[build]]` problem, not an incremental one.

## 2. Persistent-probe pattern - launch ONCE, re-probe many times

Do not relaunch the app per edit. Launch the running process once in the background, then
loop: edit -> rebuild just the crate -> probe the ALREADY-RUNNING process -> repeat. Kill
only at session end.

Editor (main session on this RTX 5090 box only - see `[[agent-ops]]` for why subagents
cannot do this):

```powershell
cargo build -p litd-editor --bin litd-editor        # ~17s incremental
# launch once via synapse: mcp__synapse__process operation=launch
#   target=...\target\debug\litd-editor.exe working_dir=C:\code\GameEditor
#   wait_for_window_title_regex="(?i)light in the dark|litd|editor" timeout_ms=90000
# GPU+world init ~30s -> use a generous timeout. Returns hwnd + pid; keep them.
```

Game host (no pre-existing `.litdgame` needed - the example builds a synthetic package
in-code, #1460):

```powershell
cargo run -p litd-game --example issue1460_frame_loop_fsv --features fsv
# real GPU host, 120 frames, asserts clean_shutdown=true + snapshot readback
```

Re-probe the running process: tail its log (below); take a synapse screenshot ONLY when
the change is visual (`mcp__synapse__screenshot path=<abs>.png window_hwnd=<hwnd>`).
Shut down cleanly so the exit reason is LOGGED: `taskkill //PID <pid>` (WM_CLOSE, never
`/F`) -> log shows `exit_reason="close_requested" clean_shutdown=true`.

## 3. Structured log-line verdicts - the cheapest reality probe

stdout/stderr are EMPTY by design; `litd-editor`/`litd-game` log via tracing to a FILE.
Do not read empty console as "no logging" (mis-filed #1531). The file is the truth channel.

```powershell
$log = "$env:LOCALAPPDATA\LitD\logs\litd-editor.log"   # or litd-game.log; override LITD_LOG_FILE
Get-Content $log -Tail 60 -Wait                         # live follow while you iterate
Select-String -Path $log -Pattern 'exit_reason|renderer_init_failed|panicked' | Select-Object -Last 20
```

Cheapest possible feature probe: make the code under test EMIT a distinctive tracing line
(e.g. `info!(target: "fsv", issueNNNN_applied = true, actual = ?x)`), rebuild, then assert
on it - a declarative verdict with the actual value ("the error is the prompt", #1166):

```powershell
Select-String -Path $log -Pattern 'issueNNNN_applied' | Select-Object -Last 1
```

Default env filter is `warn,litd_engine=info,litd_editor=info,litd_game=info`; widen with
`$env:RUST_LOG` if your line is at debug.

## 4. Parallel gate verification - overlap wall-clock

Run lint/format gates in a background job WHILE you probe behavior, so the gate cost hides
under the reality check instead of adding to it:

```powershell
$gate = Start-Job { pwsh -NoProfile -File C:\code\GameEditor\scripts\check-format.ps1 }
# ... tail the log / screenshot the window while it runs ...
Receive-Job $gate -Wait -AutoRemoveJob   # drains the pipeline; safe for $LASTEXITCODE
```

Measurement traps that silently LIE (#1556): `... | Select-Object -First N` corrupts <!-- fsv-idioms:allow (teaching the trap) -->
`$LASTEXITCODE` (use `-Last`, or capture `$out = & cmd 2>&1 | Out-String; $code = $LASTEXITCODE`);
`Measure-Object -Line` skips blank lines (use `@(Get-Content $f).Count` or `wc -l`). An FSV <!-- fsv-idioms:allow (teaching the trap) -->
command is itself code and must be verified.

## 5. When the loop stalls

Build suddenly slow, phantom link errors, or a false test "failure" -> do NOT thrash. Go to
`[[build]]`'s false-failure table: check `target\debug\incremental` size first (10GB cap,
#1530); a bare `cargo test` red on a Calyx-lens file is usually just missing step-0 env.

## 6. End of change - one full-tier pass

Before claiming done, run one full-tier reality pass per `[[fsv]]` (top tier of
`[[fsv-fast]]`), with a minimal-sufficient dataset: the smallest input that 100% proves the
path, not the biggest (#800 sec.14). State the sufficiency argument in the readback. For a
change with any runtime surface, "tests pass" is NOT done - a live-process or FSV-example
verdict is.
