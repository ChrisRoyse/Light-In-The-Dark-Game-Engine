#requires -Version 5.1
<#
.SYNOPSIS
    Register (or remove) a Windows Scheduled Task that runs prune-build-artifacts.ps1
    on a schedule, so this repo's Cargo target dirs can never silently fill the disk again.

.DESCRIPTION
    The Linux build host bounds its target dir from cron (Calyx/scripts/target-gc.sh).
    Windows dev/agent hosts had no equivalent, which is how C:\code grew past 1 TB. This
    installs the missing scheduled job. It runs in the current user's context (no admin
    required) whenever the machine is on, catching up if a run was missed.

    #1554: it installs TWO tasks, because a once-daily sweep let target\debug\incremental reach
    50 GB / 476 dirs intraday (it grows ~0.7 GB/min under concurrent agents):
      * <TaskName>              -- daily FULL prune at -Time (all stages, incremental cap included).
      * <TaskName>-Incremental  -- FREQUENT incremental-cap-only backstop every -RepeatMinutes, so
                                   the cap actually holds between the daily runs. It passes the
                                   pruner -IncrementalOnly (cheap: no full-tree recursion) and the
                                   pruner skips any target tree a live build holds, so a frequent
                                   run never wipes a cache mid-build. The gate (test-rust-package.ps1)
                                   enforces the same cap under its mutex on the hot path; this task is
                                   the backstop for growth from raw cargo runs that skip the gate.

    Output is appended to <repo>\tmp\build-artifact-gc.log.

.PARAMETER Time
    Daily FULL-prune run time, HH:mm (24h). Default 03:30.

.PARAMETER RepeatMinutes
    Cadence of the frequent incremental-cap-only backstop task, in minutes. 0 disables that second
    task (daily full prune only, the pre-#1554 behavior). Default 30.

.PARAMETER TaskName
    Base scheduled task name. Default "LITD-GameEditor-BuildArtifactGC". The frequent task is
    "<TaskName>-Incremental".

.PARAMETER Uninstall
    Remove BOTH scheduled tasks instead of creating them.

.EXAMPLE
    pwsh -File scripts\install-build-artifact-gc-task.ps1
.EXAMPLE
    pwsh -File scripts\install-build-artifact-gc-task.ps1 -Time 02:00 -RepeatMinutes 15
.EXAMPLE
    pwsh -File scripts\install-build-artifact-gc-task.ps1 -Uninstall
#>
[CmdletBinding()]
param(
    [string]$Time = "03:30",
    [int]$RepeatMinutes = 30,
    [string]$TaskName = "LITD-GameEditor-BuildArtifactGC",
    [switch]$Uninstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$incrementalTaskName = "$TaskName-Incremental"

# Legacy ghost tasks this one supersedes. CalyxTargetGC (issue #1555) pointed at an untracked
# script (~\.calyx-maint\target-gc.ps1) that did not exist on this machine, so it failed every
# day with 0xFFFD0000 while its job -- bounding Calyx\target -- was already done by
# prune-build-artifacts.ps1 (which treats Calyx\target as a first-class workspace root). We
# unregister it here so re-installing on any host eliminates the ghost and leaves ONE working GC.
$legacyGhostTasks = @("CalyxTargetGC")
function Remove-LegacyGhostTasks {
    foreach ($ghost in $legacyGhostTasks) {
        if (Get-ScheduledTask -TaskName $ghost -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $ghost -Confirm:$false
            Write-Host "Removed redundant legacy ghost task '$ghost' (#1555; its job is done by prune-build-artifacts.ps1)."
        }
    }
}

if ($Uninstall) {
    foreach ($name in @($TaskName, $incrementalTaskName)) {
        if (Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue) {
            Unregister-ScheduledTask -TaskName $name -Confirm:$false
            Write-Host "Removed scheduled task '$name'."
        } else {
            Write-Host "No scheduled task named '$name'."
        }
    }
    return
}

Remove-LegacyGhostTasks

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$wrapper = Join-Path $PSScriptRoot "run-build-artifact-gc.ps1"
if (-not (Test-Path -LiteralPath $wrapper)) { throw "wrapper not found: $wrapper" }

$logDir = Join-Path $repoRoot "tmp"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logFile = Join-Path $logDir "build-artifact-gc.log"

# Prefer PowerShell 7 (pwsh); fall back to Windows PowerShell.
$engine = (Get-Command pwsh -ErrorAction SilentlyContinue)
$engineExe = if ($engine) { $engine.Source } else { (Get-Command powershell).Source }

if ($Time -notmatch '^\d{2}:\d{2}$') { throw "Time must be HH:mm, got '$Time'" }
if ($RepeatMinutes -lt 0) { throw "RepeatMinutes must be >= 0, got '$RepeatMinutes'" }

$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited

$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -DontStopOnIdleEnd `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 2) `
    -MultipleInstances IgnoreNew

# -- Daily FULL prune (all stages) --
# -TaskName threads this task's OWN name to the wrapper so #1555's scheduled-task health self-report
# reads the right task's LastTaskResult (each task must report on itself, not the default name).
$fullAction = New-ScheduledTaskAction -Execute $engineExe `
    -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$wrapper`" -TaskName `"$TaskName`""
$fullTrigger = New-ScheduledTaskTrigger -Daily -At $Time

Register-ScheduledTask -TaskName $TaskName `
    -Action $fullAction -Trigger $fullTrigger -Principal $principal -Settings $settings `
    -Description "Bounds this repo's Cargo build artifacts (prune-build-artifacts.ps1, full sweep). Prevents target\ dirs from filling the disk on this high-churn build host." `
    -Force | Out-Null

Write-Host "Registered scheduled task '$TaskName' (daily FULL prune):"
Write-Host "  runs daily at $Time as $env:USERDOMAIN\$env:USERNAME (catches up if missed)"
Write-Host "  engine : $engineExe"
Write-Host "  wrapper: $wrapper"
Write-Host "  log    : $logFile"

# -- Frequent incremental-cap-only backstop (#1554) --
# A once-daily cap is not a cap on a resource that grows tens of GB/day. This second task enforces
# ONLY the incremental cap (cheap: no full-tree recursion) every -RepeatMinutes, so the cap holds
# intraday. The wrapper passes the pruner -IncrementalOnly, and the pruner skips any target tree a
# live cargo/rustc is building into, so a frequent run never wipes a cache mid-build.
if ($RepeatMinutes -gt 0) {
    $incAction = New-ScheduledTaskAction -Execute $engineExe `
        -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$wrapper`" -IncrementalOnly -TaskName `"$incrementalTaskName`""
    # A -Once trigger starting now, repeating every N minutes indefinitely (very long duration
    # instead of TimeSpan::MaxValue, which the scheduler rejects). Re-created on each install.
    $incTrigger = New-ScheduledTaskTrigger -Once -At (Get-Date) `
        -RepetitionInterval (New-TimeSpan -Minutes $RepeatMinutes) `
        -RepetitionDuration (New-TimeSpan -Days 3650)

    Register-ScheduledTask -TaskName $incrementalTaskName `
        -Action $incAction -Trigger $incTrigger -Principal $principal -Settings $settings `
        -Description "Frequent incremental-cache-cap backstop (prune-build-artifacts.ps1 -IncrementalOnly, #1554). Bounds target\<profile>\incremental every $RepeatMinutes min so the 10 GB cap holds intraday, not just at the daily sweep." `
        -Force | Out-Null

    Write-Host ""
    Write-Host "Registered scheduled task '$incrementalTaskName' (frequent incremental-cap backstop):"
    Write-Host "  runs every $RepeatMinutes min as $env:USERDOMAIN\$env:USERNAME (starts immediately)"
    Write-Host "  action : $wrapper -IncrementalOnly"
} else {
    Write-Host ""
    Write-Host "RepeatMinutes=0: frequent incremental-cap backstop NOT installed (daily full prune only)."
}

Write-Host ""
Write-Host "Run full now with       :  Start-ScheduledTask -TaskName '$TaskName'"
if ($RepeatMinutes -gt 0) {
    Write-Host "Run incremental now with:  Start-ScheduledTask -TaskName '$incrementalTaskName'"
}
Write-Host "Remove both with        :  pwsh -File scripts\install-build-artifact-gc-task.ps1 -Uninstall"
