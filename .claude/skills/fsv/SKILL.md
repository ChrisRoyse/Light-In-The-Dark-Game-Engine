---
name: fsv
description: "Run Full-State-Verification (FSV) correctly before closing any issue in this Rust/wgpu game engine repo. Use when verifying litd-editor / litd-game / engine work is REALLY done, closing an issue, writing or running an FSV example harness, checking a verdict, reading editor/game logs, or driving the live app via synapse. Keywords: FSV, full state verification, reality, verify, prove, close issue, harness, verdict, false-green, synapse, #800."
---

# Run Full-State-Verification (FSV) correctly

Doctrine (`gh issue view 800` sec 0-1): an issue is NOT done on a green test or a
returned `Ok`. It is done only when the real running system was driven and the
physical source of truth (Aster row / file / log line / rendered pixels) was read
back and equals expected. Only reality proves it works.

Work at the LOWEST tier that gives 100% proof of the specific claim. For a speed
playbook and the tier decision table, see the `fsv-fast` skill.

## 1. Minimal-sufficient dataset (doctrine #800 sec 14)

Before running data through the system, ask: what is the SMALLEST dataset that, if
it passes, 100% proves this works? Use that, not the biggest. Verification's job is
coverage, not volume. State the sufficiency argument in your readback: which cases
the set covers, why passing it equals passing the full set, what was excluded.
Exception: when SCALE itself is the property under test (throughput / memory /
concurrency / overflow), scale is NOT reducible.

## 2. Verdict integrity: a harness MUST fail the process on a failed verdict

The false-green anti-pattern (issues #1458, #1483): compute a verdict, serialize it
into the report JSON, then `return Ok(())` regardless. The acceptance boolean is
written and never read, so the harness reports green over a broken system. A harness
that does this is WORSE than no harness.

Correct: every acceptance boolean must gate the process. Use `assert!(...)` /
`ensure!(...)` / `return Err(...)` so a failed verdict is a non-zero exit. See
`crates/litd-game/examples/issue1460_frame_loop_fsv.rs:97-122` for the pattern
(every sub-verdict asserted before the report is written).

Also avoid tautological evidence: `sha256(x) == sha256(x)` from the same in-memory
bytes can never fail. The independent side must come from an independent read (e.g.
the physical Aster WAL frame in issue1460 at `read_calyx_json_wal_write`), not a
recompute of the same buffer.

Sweep for the pattern before trusting a harness:

```powershell
pwsh -File scripts\check-fsv-idioms.ps1   # bans the PS measurement traps (sec 6)
```

## 3. FSV example blocks MUST set harness = false

FSV harnesses live as `[[example]]` targets run via
`cargo test --example <name> --features fsv`. `cargo test --example` compiles in
`--test` mode, so libtest's harness REPLACES `main()` unless the block sets
`harness = false`. Symptom of the bug: `running 0 tests / test result: ok. 0 passed`
and NO report JSON written -- a hollow false-green gate (#1497 broke this, #1527
caught it). Every FSV `[[example]]` block needs BOTH:

```toml
required-features = ["fsv"]
harness = false
```

When confirming an FSV example ran, confirm the on-disk report artifact exists and
read its values. NEVER trust the gate exit code alone: `cargo test --example` exits
0 on "0 passed". Examples that read repo files must use `fsv_artifacts::workspace_path()`
(CWD under `cargo test --example` is the PACKAGE root, not the workspace root).

Run the whole gated example gate:

```powershell
pwsh -File scripts\test-fsv-examples.ps1 -Package litd-engine
pwsh -File scripts\check-examples.ps1     # cargo-checks every example so none rots
```

## 4. Editor / game log to a FILE, not stdout

`litd-editor.exe` and `litd-game.exe` log via `litd_engine::init_file_logging` to
`%LOCALAPPDATA%\LitD\logs\<app>.log` (override `LITD_LOG_FILE`). stdout/stderr are
EMPTY by design -- do NOT read empty console as "silent / no logging" (that misfiled
#1531). The structured `EditorExitReason` + panic hook write the exact cause there.
Always tail the log first when a binary exits non-zero:

```powershell
Get-Content -Tail 40 "$env:LOCALAPPDATA\LitD\logs\litd-editor.log"
Select-String -Path "$env:LOCALAPPDATA\LitD\logs\litd-editor.log" -Pattern "exit_reason|last_exit_reason|renderer_init"
```

Clean shutdown logs `exit_reason="close_requested" clean_shutdown=true`.

## 5. Live-app FSV via synapse (main session only; GPU + HWND + pixels)

This box IS the RTX 5090 + 2-display box. The MAIN session can launch the real
binaries and drive them via the `synapse` MCP tools. Headless subagents (Agent tool)
run with no GPU/display and MUST defer GPU/HWND/pixel claims to the main session.

Editor recipe (see `references/live-app-recipe.md` for the full flow and gotchas):
1. `cargo build --bin litd-editor`  (binary, not lib; ~17s incremental)
2. `mcp__synapse__process` operation=launch, target=...\litd-editor.exe,
   `wait_for_window_title_regex="(?i)light in the dark|litd|editor"`, timeout_ms=90000
   (GPU+world init takes ~30s) -> returns real hwnd + pid.
3. `mcp__synapse__target` operation=list, process_name_contains=litd -> proves HWND on
   the interactive desktop.
4. `mcp__synapse__screenshot` path=<abs>.png window_hwnd=<hwnd> -> real pixels + sha256.
5. `taskkill //PID <pid>`  (WM_CLOSE, NOT /F) so the log records the exit reason.

Game host has no window recipe -- it runs end-to-end via a SELF-BUILT synthetic
package (no pre-existing .litdgame needed):

```powershell
cargo run -p litd-game --example issue1460_frame_loop_fsv --features fsv
```

GOTCHA: `LITD_EDITOR_PROJECT_ROOT` is the MCP tool path-confinement root (#1518
security), NOT the editor's repo/world root. A bad value does NOT fail boot.

## 6. PowerShell measurement traps -- the FSV command is itself code

Two idioms silently produce WRONG results (#1556). `scripts\check-fsv-idioms.ps1`
bans them. A verification you have never watched fail is not a verification.

- `| Select-Object -First N` short-circuits the pipeline and corrupts <!-- fsv-idioms:allow (teaching the trap) -->
  `$LASTEXITCODE`: a FAILING native command reads as exit 0. `-Last N` is safe.
  Correct: `$out = & cmd 2>&1 | Out-String; $code = $LASTEXITCODE`
- `Measure-Object -Line` does NOT count blank lines (undercounts). Correct: <!-- fsv-idioms:allow (teaching the trap) -->
  `@(Get-Content $f).Count`, or `wc -l` via the Bash tool.

## 7. Evidence to paste when closing an issue

Post ALL of, from the reality run (not a plan):
1. The exact command(s) run (copy-paste).
2. The verdict / report line, e.g. `ISSUE1460_FRAME_LOOP_FSV report=... status=exited
   hash_ok=true edge_failed_closed=true`, and the report artifact path + sha256.
3. The log excerpt proving the source of truth (Aster sha256, `exit_reason=...`).
4. For GPU/HWND/pixel claims: the screenshot path + sha256 and the HWND from
   `target list`.
5. A one-line sufficiency argument (sec 1): why this minimal run proves the claim.

If any tier could not run (headless subagent, missing hardware), say so explicitly
and mark the claim DEFERRED -- never imply reality-FSV when it did not happen.
