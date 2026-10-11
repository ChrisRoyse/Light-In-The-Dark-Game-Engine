#requires -Version 5.1
<#
.SYNOPSIS
    Fail LOUD when a maintenance scheduled task's last run failed.

.DESCRIPTION
    A scheduled maintenance task can fail every single day, forever, and nothing
    surfaces it: Task Scheduler records a non-zero LastTaskResult and no human or agent
    ever looks. That is how the CalyxTargetGC ghost (issue #1555) failed daily with
    0xFFFD0000 (the script it invoked did not exist) while Calyx\target was tended by
    nothing. Same false-signal disease as #1543/#1536/#1545/#1553: the system reports
    health it has not earned.

    This check is the antidote. It reads Get-ScheduledTaskInfo.LastTaskResult for one or
    more tasks and:
      * exits 0 and prints a one-line OK per healthy task, OR
      * exits 1 and prints a loud, unmissable banner naming every task whose last run
        FAILED (a non-zero result that is not a benign "running"/"never run"/"queued"
        sentinel).

    It is wired into run-build-artifact-gc.ps1 (which already runs daily), so every GC
    run self-reports whether its OWN previous run succeeded, straight into the log a human
    reads. A missing task is itself a failure (someone deleted the GC) unless -AllowMissing.

    KEEP-ALIVE-DAEMON EXCEPTION (issue #1572). A minority of tasks are "run forever, one
    instance only" daemons: a logon trigger starts one long-lived process, a short
    repeating trigger re-fires every minute, and MultipleInstances=IgnoreNew makes every
    re-fire while the daemon is alive get REFUSED by Task Scheduler, which records
    0x800710E0 ("The operator or administrator has refused the request" == Event 322
    "Launch request ignored, instance already running"). For such a task that code is PROOF
    the single instance is alive, NOT a failure. It is still a REAL failure on any ordinary
    one-shot task, so it must not be globally benign. The per-task benign table below (and
    -TaskBenignResult) scope that exception to exactly the tasks that earn it; a genuine
    crash of that same daemon surfaces a DIFFERENT code and still fails loud.

.PARAMETER TaskName
    One or more scheduled task names to check. Default: the build-artifact GC task and the
    main-health backstop task (#1567). Both are first-party, always-expected maintenance tasks on this
    build host, so an audit that finds either MISSING (without -AllowMissing) is a real finding.

.PARAMETER TaskBenignResult
    Optional hashtable mapping a task name to one or more LastTaskResult codes that are
    benign FOR THAT TASK ONLY (e.g. @{ 'MyDaemon' = 0x800710E0 } or an array of codes).
    Merges with the built-in keep-alive-daemon table; other tasks still fail on those codes.

.PARAMETER AllowMissing
    Treat a task that does not exist as OK (skipped) instead of a failure. Use when a task
    is optional on some hosts (e.g. a vendor daemon that may not be installed).

.PARAMETER Quiet
    Suppress the per-task OK lines; still prints the loud banner and still sets the exit code
    on failure.

.OUTPUTS
    Exit code 0 = all checked tasks healthy. Exit code 1 = at least one failed (or missing
    without -AllowMissing).

.EXAMPLE
    pwsh -File scripts\check-scheduled-task-health.ps1

.EXAMPLE
    pwsh -File scripts\check-scheduled-task-health.ps1 -TaskName LITD-GameEditor-BuildArtifactGC, SomeOtherTask

.EXAMPLE
    # Check the external Leapable keep-alive daemon; 0x800710E0 is benign for it (built in),
    # and -AllowMissing keeps it green on hosts where Leapable is not installed.
    pwsh -File scripts\check-scheduled-task-health.ps1 -TaskName LeapableSidecarSupervisor -AllowMissing
#>
[CmdletBinding()]
param(
    [string[]]$TaskName = @("LITD-GameEditor-BuildArtifactGC", "LITD-GameEditor-MainHealth"),
    [hashtable]$TaskBenignResult = @{},
    [switch]$AllowMissing,
    [switch]$Quiet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Benign non-zero results reported by Task Scheduler that are NOT failures:
#   0x00000000 (0)      SUCCESS
#   0x00041300 (267008) SCHED_S_TASK_READY       - ready to run at its next scheduled time
#   0x00041301 (267009) SCHED_S_TASK_RUNNING     - currently running
#   0x00041302 (267010) SCHED_S_TASK_DISABLED    - disabled on purpose
#   0x00041303 (267011) SCHED_S_TASK_HAS_NOT_RUN - has never run yet (freshly installed)
#   0x00041305 (267013) SCHED_S_TASK_NO_MORE_RUNS
#   0x00041306 (267014) SCHED_S_TASK_NOT_SCHEDULED
# Anything else (0xFFFD0000, 0x80070002 file-not-found, 0x800710E0, 1, ...) is a real failure.
# Keyed as [uint32] on purpose: a failing result like 0xFFFD0000 (4294770688) overflows Int32,
# so Int32 keys/lookups throw. uint32 keys make every valid task result representable.
$benign = @{}
$benign[[uint32]0]      = "SUCCESS"
$benign[[uint32]267008] = "READY (queued for next run)"
$benign[[uint32]267009] = "RUNNING"
$benign[[uint32]267010] = "DISABLED"
$benign[[uint32]267011] = "HAS NOT RUN YET"
$benign[[uint32]267013] = "NO MORE RUNS SCHEDULED"
$benign[[uint32]267014] = "NOT SCHEDULED"

# Per-task benign results (issue #1572). Some tasks legitimately report a specific
# non-zero LastTaskResult as their NORMAL, healthy steady state. Keying these by task
# name scopes the exception to the ONE task that earns it: every OTHER task still fails
# loud on the same code, and a genuine crash of THIS task surfaces a different code that
# is not listed here and so still fails loud.
#
# LeapableSidecarSupervisor is the canonical keep-alive daemon: an external, third-party
# task (C:\Users\...\Local\Leapable Installer\leapable-installer.exe --leapable-scheduled
# -sidecar-supervisor). A logon trigger starts one long-lived supervisor, a ~1-minute
# repeating trigger re-fires forever, and MultipleInstances=IgnoreNew makes Task Scheduler
# REFUSE every re-fire while the daemon is alive, recording 0x800710E0 (Event 322 "Launch
# request ignored, instance already running"). Proven benign in #1572 by demand-starting
# the task and observing the daemon PID unchanged (no second instance spawned) with the
# result re-recorded as 0x800710E0. It is external code we do not own, so the fix lives
# here (recognise its healthy signal) rather than in the vendor's task definition.
# NOTE on literals: PowerShell parses an 8-hex-digit literal like 0x800710E0 as a NEGATIVE
# Int32 (-2147020576), which then throws on [uint32] conversion. So codes are written as
# positive decimals (with the hex in a comment), matching the global $benign table above.
$taskBenign = @{}
$taskBenign["LeapableSidecarSupervisor"] = @{
    [uint32]2147946720 = "keep-alive daemon alive; duplicate launch refused (Event 322)"  # 0x800710E0
}
# LITD-GameEditor-MainHealth (#1567) is deliberately ABSENT from this table. Its payload
# (run-main-health-gate.ps1) exits 3 when origin/main is RED (a correctness regression reached main)
# and 4 when the gate could NOT be run (fetch/worktree/setup failure). BOTH are REAL failures that
# MUST fail loud here -- allowlisting either would recreate the exact "reports health it has not
# earned" disease this whole check exists to kill. Only the global benign sentinels (0, HAS-NOT-RUN,
# RUNNING, ...) apply to it, so a fresh/queued/running MainHealth stays green and a red main does not.

# Return a reason string if $Code is benign for the task named $Name, else $null.
# Checks the built-in $taskBenign table first, then caller-supplied -TaskBenignResult.
function Get-TaskBenignReason {
    param([string]$Name, [uint32]$Code)
    if ($taskBenign.ContainsKey($Name) -and $taskBenign[$Name].ContainsKey($Code)) {
        return $taskBenign[$Name][$Code]
    }
    if ($TaskBenignResult.ContainsKey($Name)) {
        # Mask with 0xFFFFFFFFL (positive Int64) so a caller-supplied negative-Int32 hex
        # literal such as 0x800710E0 normalises to its unsigned 32-bit value.
        foreach ($c in @($TaskBenignResult[$Name])) {
            if (([uint32]([int64]$c -band 0xFFFFFFFFL)) -eq $Code) {
                return "declared benign for this task (-TaskBenignResult)"
            }
        }
    }
    return $null
}

$failures = New-Object System.Collections.ArrayList

foreach ($name in $TaskName) {
    $task = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
    if (-not $task) {
        if ($AllowMissing) {
            if (-not $Quiet) { Write-Host ("  [skip] task '{0}' not registered (-AllowMissing)" -f $name) }
            continue
        }
        [void]$failures.Add([PSCustomObject]@{ Name = $name; Result = $null; Hex = "MISSING"; Meaning = "task is not registered"; LastRun = $null })
        continue
    }

    $info = $task | Get-ScheduledTaskInfo
    # LastTaskResult is an unsigned 32-bit code; normalise to uint32 for hex display and lookup
    # (values like 0xFFFD0000 overflow Int32, so we never touch Int32 here).
    $u32 = [uint32]([int64]$info.LastTaskResult -band 0xFFFFFFFF)
    $hex = "0x{0:X8}" -f $u32

    if ($benign.ContainsKey($u32)) {
        if (-not $Quiet) {
            Write-Host ("  [ ok ] {0,-34} {1} {2}  (last run {3})" -f $name, $hex, $benign[$u32], $info.LastRunTime)
        }
        continue
    }

    $taskReason = Get-TaskBenignReason -Name $name -Code $u32
    if ($taskReason) {
        if (-not $Quiet) {
            Write-Host ("  [ ok ] {0,-34} {1} {2}  (last run {3})" -f $name, $hex, $taskReason, $info.LastRunTime)
        }
        continue
    }

    [void]$failures.Add([PSCustomObject]@{
        Name    = $name
        Result  = $u32
        Hex     = $hex
        Meaning = "non-zero LastTaskResult - last run FAILED"
        LastRun = $info.LastRunTime
    })
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "################################################################" -ForegroundColor Red
    Write-Host "## SCHEDULED-TASK HEALTH CHECK FAILED" -ForegroundColor Red
    Write-Host "## A maintenance task is failing and nobody was being told." -ForegroundColor Red
    Write-Host "################################################################" -ForegroundColor Red
    foreach ($f in $failures) {
        Write-Host ("##  FAIL  {0}" -f $f.Name) -ForegroundColor Red
        Write-Host ("##          result : {0} ({1})" -f $f.Hex, $f.Meaning) -ForegroundColor Red
        if ($f.LastRun) { Write-Host ("##          lastRun: {0}" -f $f.LastRun) -ForegroundColor Red }
    }
    Write-Host "################################################################" -ForegroundColor Red
    Write-Host ""
    exit 1
}

if (-not $Quiet) { Write-Host "scheduled-task health: all checked tasks OK" }
exit 0
