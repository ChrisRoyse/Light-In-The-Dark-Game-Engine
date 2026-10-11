#requires -Version 5.1
<#
.SYNOPSIS
    Register (or remove) a Windows Scheduled Task that runs run-main-health-gate.ps1 on a schedule --
    the CI-substitute detection backstop for a bypassed / broken origin/main (#1567).

.DESCRIPTION
    Local git hooks are the ONLY quality enforcement here (no CI: #284/#447/#689/#317; github.com CLOUD
    remote so no server-side pre-receive hook), and they are ALWAYS bypassable with --no-verify. There
    is no un-bypassable gate possible in this repo. So instead of trying (and failing) to block the
    bypass, this task DETECTS a red main shortly after it happens: it periodically runs the full
    correctness gate against origin/main and alarms LOUD (non-zero LastTaskResult + red file + Event
    Log) when main is broken. See run-main-health-gate.ps1 for the payload and its exit codes.

    Follows install-build-artifact-gc-task.ps1 exactly: pwsh path resolution, -NoProfile -File action,
    current-user Interactive/Limited principal (no admin needed to install or run), StartWhenAvailable
    (catches up if a run was missed), idempotent Force register, and an -IntervalHours repeating
    -Once trigger (mirrors the frequent GC backstop).

.PARAMETER IntervalHours
    How often to run the main-health gate, in hours. Default 6.
    Justification: a --no-verify red push should be caught within HOURS, not up to a day, so the next
    session inherits the diagnostic instead of discovering it the slow way. Every 6 hours = 4 checks a
    day, a balance between promptness and build-host load: each run is INCREMENTAL against the shared
    warm cache and is near-free when origin/main has not advanced (the gate only recompiles when main
    actually moves). Set larger (e.g. 24) on a light host, or smaller on a busy shared remote.

.PARAMETER TaskName
    Scheduled task name. Default "LITD-GameEditor-MainHealth". check-scheduled-task-health.ps1 knows
    this exact name (it is in that script's default task list), so a red result surfaces on any audit.

.PARAMETER RegisterEventSource
    Also (best-effort) register the "LITD-GameEditor-MainHealth" Windows Event Log source so the
    payload's Event Log alarm channel works. Creating an Event Log source needs an ELEVATED shell once;
    run this installer as admin with this switch to enable it. It is OPTIONAL: the payload still alarms
    via its non-zero exit code (LastTaskResult) and the red alarm file without the Event Log source.

.PARAMETER Remove
    Remove the scheduled task instead of creating it.

.EXAMPLE
    pwsh -File scripts\install-main-health-task.ps1
.EXAMPLE
    pwsh -File scripts\install-main-health-task.ps1 -IntervalHours 12
.EXAMPLE
    # As admin, also enable the Event Log alarm channel:
    pwsh -File scripts\install-main-health-task.ps1 -RegisterEventSource
.EXAMPLE
    pwsh -File scripts\install-main-health-task.ps1 -Remove
#>
[CmdletBinding()]
param(
    [int]$IntervalHours = 6,
    [string]$TaskName = "LITD-GameEditor-MainHealth",
    [switch]$RegisterEventSource,
    [switch]$Remove
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$EventSource  = "LITD-GameEditor-MainHealth"
$EventLogName = "Application"

function Register-MainHealthEventSource {
    try {
        if ([System.Diagnostics.EventLog]::SourceExists($EventSource)) {
            Write-Host "Event Log source '$EventSource' already exists."
            return
        }
        New-EventLog -LogName $EventLogName -Source $EventSource -ErrorAction Stop
        Write-Host "Registered Event Log source '$EventSource' in log '$EventLogName'."
    } catch {
        Write-Warning "Could not register Event Log source '$EventSource' (needs an elevated/admin shell once): $($_.Exception.Message)"
        Write-Warning "Non-fatal: run-main-health-gate.ps1 still alarms via its non-zero exit code (LastTaskResult) and the red alarm file. Re-run this installer AS ADMIN with -RegisterEventSource to enable the Event Log channel too."
    }
}

if ($Remove) {
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "Removed scheduled task '$TaskName'."
    } else {
        Write-Host "No scheduled task named '$TaskName'."
    }
    return
}

if ($RegisterEventSource) { Register-MainHealthEventSource }

if ($IntervalHours -lt 1) { throw "IntervalHours must be >= 1, got '$IntervalHours'" }

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$wrapper = Join-Path $PSScriptRoot "run-main-health-gate.ps1"
if (-not (Test-Path -LiteralPath $wrapper)) { throw "payload not found: $wrapper" }

$logDir = Join-Path $repoRoot "tmp"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logFile = Join-Path $logDir "main-health.log"

# Prefer PowerShell 7 (pwsh); fall back to Windows PowerShell.
$engine = (Get-Command pwsh -ErrorAction SilentlyContinue)
$engineExe = if ($engine) { $engine.Source } else { (Get-Command powershell).Source }

# -TaskName threads this task's OWN name to the payload so its scheduled-task health self-report reads
# the right task's LastTaskResult (each task reports on itself; mirrors run-build-artifact-gc.ps1).
$action = New-ScheduledTaskAction -Execute $engineExe `
    -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$wrapper`" -TaskName `"$TaskName`""

# A -Once trigger starting now, repeating every -IntervalHours indefinitely (very long duration
# instead of TimeSpan::MaxValue, which the scheduler rejects). Mirrors the frequent GC backstop.
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) `
    -RepetitionInterval (New-TimeSpan -Hours $IntervalHours) `
    -RepetitionDuration (New-TimeSpan -Days 3650)

$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited

# ExecutionTimeLimit 4h: the full gate (cargo check + test gate + ~230 FSV examples + strict clippy +
# cargo-deny) is heavier than the GC job; a warm run is minutes but a cold/advanced-main run is longer.
$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -DontStopOnIdleEnd `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 4) `
    -MultipleInstances IgnoreNew

Register-ScheduledTask -TaskName $TaskName `
    -Action $action -Trigger $trigger -Principal $principal -Settings $settings `
    -Description "CI-substitute main-health backstop (#1567). Runs scripts\run-main-health-gate.ps1 every $IntervalHours h: the full correctness gate against origin/main, alarming LOUD (non-zero LastTaskResult + red file + Event Log) when a bypassed/broken push has made main red." `
    -Force | Out-Null

Write-Host "Registered scheduled task '$TaskName' (main-health backstop):"
Write-Host "  runs every $IntervalHours h as $env:USERDOMAIN\$env:USERNAME (starts immediately; catches up if missed)"
Write-Host "  engine : $engineExe"
Write-Host "  payload: $wrapper"
Write-Host "  log    : $logFile"
if (-not $RegisterEventSource) {
    Write-Host ""
    Write-Host "Event Log alarm channel is OPTIONAL and not enabled here. To add it, once, as admin:"
    Write-Host "  pwsh -File scripts\install-main-health-task.ps1 -RegisterEventSource"
    Write-Host "  (the exit-code + red-file alarms work without it.)"
}
Write-Host ""
Write-Host "Run now with:  Start-ScheduledTask -TaskName '$TaskName'"
Write-Host "Audit health:  pwsh -File scripts\check-scheduled-task-health.ps1"
Write-Host "Remove with :  pwsh -File scripts\install-main-health-task.ps1 -Remove"
