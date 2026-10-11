# The no-CI main-health backstop (#1567)

There is NO un-bypassable correctness gate possible here: no CI (permanent decision
#284/#447/#689/#317) and a github.com CLOUD remote (so no server-side pre-receive/update
hook). Client hooks (pre-commit format ratchet, pre-push correctness gate) are ALWAYS
bypassable with `--no-verify` by design. Do not try to block the bypass (impossible; a false
sense of security). Make it DETECTED.

## The scheduled detector

`scripts\install-main-health-task.ps1` registers `LITD-GameEditor-MainHealth`, which runs
`scripts\run-main-health-gate.ps1` every `-IntervalHours` (default 6). Each run:

1. `git fetch origin`, resolves `origin/main`.
2. Hard-syncs a STABLE, REUSED detached worktree at `target\main-health\worktree` to that
   sha (reset --hard + targeted `git clean -fd -e target` that PRESERVES the warm in-worktree
   cargo cache - never a re-clone, never wiped).
3. Runs `scripts\pre-push-gate.ps1` (cargo check + test gate + FSV examples + strict clippy +
   cargo-deny) into that warm in-worktree cache. It MUST be in-worktree: LitdOwnedTarget.ps1
   (#1520) rejects any `-TargetDir` outside the checkout under test.
4. Writes `target\main-health\last-run.json` (utc, main sha, PASS/FAIL, failing step + first
   error lines).

The vault is not copied into the worktree (that copy caused #1569); instead `LITD_REPO_ROOT`
points editor asset tests at the real repo root's vault.

## RED vs INFRA - decided by MainHealthClassification.ps1, not exit code

The gate exits 1 both for a real correctness failure AND for a setup/guard throw. Keying RED
on "exit != 0" would false-alarm on infra. So `Get-MainHealthClassification` (a pure,
unit-testable function of exit code + output lines) decides:

- GREEN (exit 0): gate passed.
- INFRA (exit 4): gate ran but output carries a setup marker
  (`TEST_GATE_TARGET_*` #1520, `LITD_CALYX_*` / `CALYX_LENS_UNREACHABLE` #1493) - could NOT
  reach an honest verdict. Alarms as INFRA, never RED.
- RED (exit 3): gate failed with NO such marker - a genuine regression reached main.

Fetch/worktree/checkout failures happen before gate output and are caught by the payload's
try/catch, classified INFRA (exit 4) there.

## Three always-on alarm channels

- exit code recorded as scheduler LastTaskResult (3 and 4 both non-benign).
- red file `target\main-health\ALARM-main-red.txt` (removed only on a GREEN run).
- Windows Event Log Error (source `LITD-GameEditor-MainHealth`, id 1567) - best-effort;
  needs `-RegisterEventSource` once as admin, but the exit code + red file fire regardless.

## Commands

```powershell
pwsh -File scripts\install-main-health-task.ps1                 # install (every 6h)
pwsh -File scripts\install-main-health-task.ps1 -IntervalHours 12
Start-ScheduledTask -TaskName 'LITD-GameEditor-MainHealth'      # run now
pwsh -File scripts\check-scheduled-task-health.ps1              # audit LastTaskResult
pwsh -File scripts\install-main-health-task.ps1 -Remove
```
