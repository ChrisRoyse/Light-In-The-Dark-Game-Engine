#requires -Version 5.1
<#
.SYNOPSIS
    Thin wrapper the scheduled task invokes: runs prune-build-artifacts.ps1 and appends
    its output to a timestamped log. Kept separate so the task action needs no inline
    quoting. Safe to run by hand too.
.PARAMETER LogFile
    Where to append output. Default <repo>\tmp\build-artifact-gc.log.
.PARAMETER IncrementalOnly
    Pass -IncrementalOnly to the pruner: run ONLY the incremental cache cap, not the full sweep.
    The scheduled task's frequent (sub-daily) trigger uses this so the tight-cadence backstop that
    bounds incremental growth intraday (#1554) stays cheap; the daily trigger runs the full prune.
#>
[CmdletBinding()]
param(
    [string]$LogFile,
    [string]$TaskName = "LITD-GameEditor-BuildArtifactGC",
    [switch]$IncrementalOnly
)

$ErrorActionPreference = "Continue"
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$pruner = Join-Path $PSScriptRoot "prune-build-artifacts.ps1"
$healthCheck = Join-Path $PSScriptRoot "check-scheduled-task-health.ps1"

if (-not $LogFile) {
    $logDir = Join-Path $repoRoot "tmp"
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $LogFile = Join-Path $logDir "build-artifact-gc.log"
}

$prunerArgs = @()
if ($IncrementalOnly) { $prunerArgs += "-IncrementalOnly" }

$ts = Get-Date -Format o
$mode = if ($IncrementalOnly) { "incremental-only" } else { "full" }
Add-Content -LiteralPath $LogFile -Value "`n===== $ts  mode=$mode ====="

# Self-report: did THIS GC task's PREVIOUS run succeed? While this wrapper runs, the task's
# LastTaskResult still reflects the last completed run, so we surface it into the log a human
# actually reads. This is the fix for the deeper #1555 bug -- a maintenance task must never be
# able to fail silently forever. A non-zero result prints a loud banner here (and to stderr).
if (Test-Path -LiteralPath $healthCheck) {
    Add-Content -LiteralPath $LogFile -Value "-- scheduled-task health (previous run of this GC task) --"
    & $healthCheck -TaskName $TaskName *>> $LogFile
}

& $pruner @prunerArgs *>> $LogFile
