---
name: fsv-fast
description: "Make the reality-feedback loop FAST when verifying litd-editor / litd-game / engine work. Use when an FSV or verify step feels slow, picking the cheapest way to prove a claim, batching checks into one app launch, or deciding whether a claim needs a GPU/pixel run. Encodes a verdict-speed pyramid (cargo check -> unit test -> FSV example -> game-host run -> live editor) and a claim->tier decision table. Keywords: fast FSV, speed, tier, warm cache."
---

# Make the reality-feedback loop FAST

Companion to the `fsv` skill (correctness). This is the SPEED playbook. Correctness
doctrine still holds: the verdict must gate the process, evidence is a real readback.

## Core rule

Answer at the LOWEST tier that constitutes 100% proof for the SPECIFIC claim being
verified. Only climb when the claim is actually about rendering / window / input /
GPU reality. Most claims are proven far below a live GPU launch. Climbing tiers "to
be safe" is the single biggest waste in this loop.

## The verdict-speed pyramid

| Tier | What | Typical latency | Proves |
|------|------|-----------------|--------|
| T0 | `cargo check -p <crate>` | ~20s incremental | It compiles / types line up |
| T1 | targeted unit test `cargo test -p <crate> <name> -- --nocapture` | seconds after warm build | A pure function / invariant / branch |
| T2 | FSV example harness, headless, no GPU | tens of seconds | Real logic + independent readback (Aster/file/hash), no display needed |
| T3 | synthetic-package game-host run (litd-game) | ~seconds of run + build | Real host + GPU frame loop + persisted runtime state, no pre-existing .litdgame |
| T4 | live editor GPU / HWND / pixel via synapse | ~30s init + probes; MAIN session only | Actual window, rendered pixels, input, exit reason |

T0-T2 run fine in a headless subagent. T3 needs a GPU (this box). T4 is main-session
only (subagents are headless and must DEFER).

### Tier commands (copy-paste)

```powershell
# T0
cargo check -p litd-editor
# T1
cargo test -p litd-engine <test_name> -- --nocapture
# T2  (needs the fsv feature + harness=false blocks; see the fsv skill sec 3)
cargo run -p litd-engine --example <name> --features fsv
pwsh -File scripts\test-fsv-examples.ps1 -Package litd-engine   # the whole gated set
# T3
cargo run -p litd-game --example issue1460_frame_loop_fsv --features fsv
# T4  -> see the fsv skill sec 5 and references/live-app-recipe.md (synapse)
cargo build --bin litd-editor
```

## Claim -> minimum tier -> command template

| Claim type | Min tier | Command template |
|------------|----------|------------------|
| "It compiles / API still lines up" | T0 | `cargo check -p <crate>` |
| Pure fn / math / invariant / single branch | T1 | `cargo test -p <crate> <name> -- --nocapture` |
| Data persisted correctly (Aster row / file / hash matches independent read) | T2 | `cargo run -p <crate> --example <name> --features fsv`, then read the report JSON |
| Engine/editor logic path, no display needed | T2 | same |
| Game frame loop / runtime-state persistence / off-thread cadence | T3 | `cargo run -p litd-game --example issue1460_frame_loop_fsv --features fsv` |
| Editor renders / window exists / correct pixels / input works | T4 | synapse launch -> target list -> screenshot -> taskkill //PID (main session) |
| Exit reason / clean shutdown of a binary | T2/T4 | run it, then tail `%LOCALAPPDATA%\LitD\logs\<app>.log` for `exit_reason` |

If the claim is NOT about pixels/window/input, do NOT climb to T4 -- a log-line or
Aster-hash verdict at T2/T3 is 100% proof and orders of magnitude cheaper.

## Speed tactics

- Keep the warm `target/` cache. NEVER `cargo clean` per run. If a build is slow,
  check `target\debug\incremental` size first (capped at 10GB by the daily GC);
  never disable incremental (20s vs 93s on litd-editor).
- Reuse a running process across probes. Launch the editor/game ONCE, then
  screenshot / probe / read-log MANY times, and `taskkill //PID` at the end. Do not
  relaunch per check (each launch costs ~30s of GPU+world init).
- Batch multiple FSV claims into ONE launch session. Plan every probe you need, run
  them against the single live process, collect all evidence, then shut down.
- Prefer log-line verdicts over pixel evidence when pixels are NOT the claim. Tailing
  `exit_reason` / a structured tracing line is far cheaper than a screenshot and is
  100% proof for non-visual claims.
- Pre-flight env ONCE per shell, not per command:
  `. scripts\Set-GameEditorCalyxRuntimeEnv.ps1` (sets CALYX_HOME + ONNX ORT paths; a
  bare `cargo test` "failure" on the A38 ONNX lens is usually just missing env, not a
  regression). Also set `LITD_EDITOR_PROJECT_ROOT` once for editor MCP tests.
- Run independent harnesses in parallel PowerShell jobs
  (`Start-Job` / `-run_in_background`) instead of serially.
- Time-compress soaks with config knobs (`--max-frames`, cadence/quality flags)
  rather than real-time waits. issue1460 proves a 1 Hz off-thread heartbeat in ~1.9s
  (120 frames), not a real minute.
- Write NEW FSV harnesses as `[[example]]` with `harness = false` + `required-features
  = ["fsv"]`. They then compile in the normal example build with zero extra per-gate
  compile cost, and never leak into the routine `cargo test` gate.
- Apply minimal-sufficient sizing (fsv skill sec 1): the smallest dataset that gives
  100% proof runs fastest. Do not process a million rows when a chosen small set
  covers every branch and boundary.
