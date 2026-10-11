---
name: agent-ops
description: Orchestrate subagents safely and fast in this no-CI repo. Use before fanning out: the hardware split (ONLY the main session on this RTX 5090 box does GPU/HWND/pixel FSV; headless subagents return code verdicts + a DEFERRED-FSV handoff), worktree junction safety (never robocopy /MIR - it emptied the asset vault #1569), and disjoint file ownership. Triggers - subagent, dispatch, fan-out, worktree, parallel agents, delegate, orchestrate, junction.
---

# agent-ops

How to run subagents in THIS repo without the recurring failures: false-green visual claims
from headless agents, data loss from unsafe worktree cleanup, agents that stall mid-task,
and same-file edit conflicts. Read before dispatching a fan-out.

## 1. Hardware split - who can verify what

This box IS the RTX 5090 + 2-display machine (#1485/#1146/#1150). Consequence:

- The **main session** (orchestrator) CAN launch the real `litd-editor.exe`/`litd-game.exe`
  and drive them via synapse for HWND / GPU / rendered-pixel / exit-reason FSV.
- **Subagents launched via the Agent tool run HEADLESS** - no GPU, no display. They MUST
  NOT claim any GPU-backed render verdict. They return code-level + headless verdicts
  (`cargo check`/`test`/FSV-example output) and DEFER all visual FSV to the orchestrator.

So split work accordingly: give subagents the compile/test/logic-verdict work; keep the
"launch the window and look" work on the main session (recipe in `[[reality-loop]]` sec.2).

## 2. The DEFERRED-FSV handoff block (what a headless agent must return)

A headless agent that touched a render/HWND/pixel path is NOT done - it is *deferred*. Its
final message must hand the orchestrator everything needed to run the visual FSV without
re-deriving it:

```
DEFERRED-FSV (headless; needs the RTX 5090 main session):
  launch cmd    : cargo build -p litd-editor --bin litd-editor; <synapse process launch ...>
  expected title: Light in the Dark Editor - world view
  expected log  : exit_reason="close_requested" clean_shutdown=true  (in %LOCALAPPDATA%\LitD\logs\litd-editor.log)
  screenshot    : world viewport renders the Ashen world, non-black, HUD present
  headless proof: <verbatim `test result:` / cargo check output the agent DID run>
```

## 3. Worktree safety - the #1569 data-loss rule

**NEVER `robocopy /MIR` a git worktree or any tree that may hold junctions.** `/MIR`
follows junctions and purges THROUGH them (no `/XJ` default). Subagents junction the large
gitignored `local-asset-vault` into their worktrees; FSV runs create junction fixtures under
`target\fsv\...`. On 2026-07-11 a `/MIR` cleanup followed a worktree's vault junction and
emptied the REAL primary vault (`C:\code\GameEditor\local-asset-vault`, NOT beside the
checkout) - not recoverable from git (#1569).

Safe cleanup (unlink every reparse point WITHOUT recursing, then delete):

```powershell
# 1. Unlink each junction LINK first (the $false = no-recurse is what makes it safe):
Get-ChildItem <worktree> -Recurse -Force | Where-Object { $_.Attributes -match 'ReparsePoint' } |
  ForEach-Object { [System.IO.Directory]::Delete($_.FullName, $false) }
# 2. Then remove the tree:
git worktree remove --force <worktree>          # if long-path fails: [System.IO.Directory]::Delete(<path>, $true)
git worktree prune
# 3. TRIPWIRE - vault child count must be UNCHANGED (run against the REAL path):
(Get-ChildItem C:\code\GameEditor\local-asset-vault | Measure-Object).Count
```

**One worktree per content category, many instances per PR.** Every rpg category funnels
through shared files (`*_test_fixture.rs`, `*_tests.rs`, the `rpg.rs` hub) - intra-category
parallel guarantees conflicts; cross-category parallel is safe. Details in
`[[content-backlog-framework-map]]`.

## 4. Parallelize by DISJOINT file ownership

Assign directories, not topics, so two agents never edit the same file. Give each agent an
explicit owned path set (e.g. agent A owns `crates/litd-engine/src/rpg/race_*`, agent B owns
`.../class_*`). Do any shared-hub edit (mod/re-export in `rpg.rs`) as one serialized step,
not inside a parallel agent. Do not stack more than ~3-4 heavy `cargo` builders at once -
the shared incremental cache is contended and net throughput drops (#1569 recovery lost its
cache to a concurrent build).

## 5. Make agents FINISH - the stall trap

Dispatched content/build agents reliably STALL after kicking off a BACKGROUND `cargo test`:
they say "I'll await the notification" and stop with all files UNCOMMITTED - the background
child never wakes the parent (#1569/#986). In the dispatch prompt require: run the test in
the FOREGROUND and block on it, capture the verbatim `test result:` line, then commit + push
+ `gh pr create` + reply - all in the SAME turn. When an agent reports "done" but the result
is really a *wait*, check ground truth (`git -C <worktree> status --short` + `log --oneline`);
if uncommitted, RESUME it via SendMessage using the raw task/agent id (the display name is
often not reachable) with a foreground-commit-PR instruction.

## 6. Tool limits & fallbacks

- **Bash tool commands over ~8KB truncate and die** with a cryptic quote error - batch
  `gh`/issue operations SMALL (a few at a time), never one giant command. For large issue
  bodies use `scripts\new-issue.ps1 -Title ... -BodyPath ...` (body-file based, hard errors,
  post-create readback verification, 504 create-then-error detection).
- **Grep from repo root is SAFE since 2026-07-12** - the root `.ignore` file excludes the
  ~550k artifact files (`target\`, `.claude\worktrees\`, vault, tmp). Do not delete `.ignore`;
  ripgrep honors it above `.gitignore`. Full evidence: issue #1707.
- **Glob does NOT honor ignore files** and stats the whole tree: even `Glob scripts/*.ps1`
  from root times out at 20s. ALWAYS pass the `path:` parameter to scope the walk
  (`path: C:\code\GameEditor\scripts`, pattern `*.ps1` = instant). Pattern prefixes do NOT scope it.
- **codegraph / codebase-memory MCP is a per-session stdio child** - if it disconnects
  mid-session it cannot be revived from inside; fall back to scoped Grep, note it on #1589,
  do not stall waiting on it.

## 7. Every "done" needs proof, and the no-CI backstop

Every agent claim of done needs EITHER a passing harness command's verbatim output OR an
explicit DEFERRED-FSV block (sec.2). "It should work" is not a verdict.

This repo structurally cannot have an un-bypassable gate (no CI by decision; github.com
cloud = no server-side pre-receive) - `--no-verify` is always possible (#1567). Do NOT fight
that. The backstop is the scheduled `LITD-GameEditor-MainHealth` task
(`scripts\run-main-health-gate.ps1`) that runs the full gate against `origin/main` every 6h
and alarms LOUD (non-zero LastTaskResult + `target\main-health\ALARM-main-red.txt` + Event
Log) when main is red. Audit it: `pwsh -File scripts\check-scheduled-task-health.ps1`. See
`references/main-health-backstop.md` for how it classifies RED vs INFRA.
