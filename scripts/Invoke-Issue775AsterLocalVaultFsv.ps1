[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path,
    [string]$CalyxHome = (Join-Path $PSScriptRoot "..\Calyx")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Set-CalyxWindowsEnv.ps1") -CalyxHome $CalyxHome | Out-Host
if ($env:NVCC_CCBIN) {
    $env:FORGE_CUDA_CCBIN = $env:NVCC_CCBIN
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$fsvRoot = Join-Path $RepoRoot "target\fsv\issue775_aster_local\$stamp"
$logs = Join-Path $fsvRoot "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

$runHot = Join-Path $env:CALYX_HOT_PATH "issue775-fsv\$stamp\hot"
$runCold = Join-Path $env:CALYX_COLD_PATH "issue775-fsv\$stamp\cold"
New-Item -ItemType Directory -Force -Path $runHot, $runCold | Out-Null
$env:CALYX_HOT_PATH = $runHot
$env:CALYX_COLD_PATH = $runCold
$env:CALYX_FSV_ROOT = $fsvRoot

$env:CALYX_ISSUE775_MISSING_COLD_PATH = Join-Path $fsvRoot "missing-cold-root"
$calyxDir = $env:CALYX_HOME

function Invoke-CapturedCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Command,
        [string]$WorkingDirectory = $calyxDir
    )

    $logPath = Join-Path $logs "$Name.log"
    Push-Location $WorkingDirectory
    try {
        $output = & $Command[0] @($Command[1..($Command.Count - 1)]) 2>&1 |
            Tee-Object -FilePath $logPath
        $output | Out-Host
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    if ($null -eq $exitCode) {
        $exitCode = 0
    }
    if ($exitCode -ne 0) {
        throw "ISSUE775_FSV_COMMAND_FAILED: name=$Name exit=$exitCode log=$logPath"
    }
    [pscustomobject]@{
        name = $Name
        exit_code = $exitCode
        log_path = $logPath
        log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $logPath).Hash.ToLowerInvariant()
    }
}

$commands = @(
    Invoke-CapturedCommand -Name "cargo-test-calyxd-config-path-vars" -Command @(
        "cargo", "test", "-p", "calyxd",
        "vault_path_interpolates_hot_and_cold_paths_when_set",
        "--lib", "--", "--nocapture"
    )
    Invoke-CapturedCommand -Name "cargo-test-aster-windows-disk-probe" -Command @(
        "cargo", "test", "-p", "calyx-aster",
        "pressure::tests::windows_os_probe_reads_existing_directory",
        "--lib", "--", "--nocapture"
    )
    Invoke-CapturedCommand -Name "cargo-test-aster-file-lock" -Command @(
        "cargo", "test", "-p", "calyx-aster",
        "file_lock::tests::try_acquire_fails_loud_when_process_lock_is_held",
        "--lib", "--", "--nocapture"
    )
    Invoke-CapturedCommand -Name "cargo-test-issue775-fsv" -Command @(
        "cargo", "test", "-p", "calyxd", "--features", "cuda",
        "--test", "issue775_windows_local_aster_fsv",
        "--", "--ignored", "--nocapture"
    )
)

$reportPath = Join-Path $fsvRoot "issue775-windows-local-aster-fsv.json"
if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
    throw "ISSUE775_FSV_REPORT_MISSING: $reportPath"
}
$report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
$expectedIndexHex = "69737375653737352d696e6465782d76616c7565"
if (-not $report.daemon_restart.after_second.cx_present) {
    throw "ISSUE775_FSV_DAEMON_CX_MISSING"
}
if ($report.daemon_restart.after_second.index_value_hex -ne $expectedIndexHex) {
    throw "ISSUE775_FSV_INDEX_READBACK_MISMATCH: $($report.daemon_restart.after_second.index_value_hex)"
}
if (-not $report.daemon_restart.second_calyxd.stdout_contains_restore_metrics) {
    throw "ISSUE775_FSV_CALYXD_RESTORE_METRIC_MISSING"
}
if (($report.tiered_hot_cold.cold_inactive_slot_ssts | Measure-Object).Count -lt 1) {
    throw "ISSUE775_FSV_COLD_SLOT_SST_MISSING"
}
if (($report.tiered_hot_cold.vault_inactive_slot_ssts | Measure-Object).Count -ne 0) {
    throw "ISSUE775_FSV_COLD_SLOT_MISPLACED"
}
if ($report.edges.missing_cold_path.error_code -ne "CALYX_DISK_PRESSURE") {
    throw "ISSUE775_FSV_MISSING_COLD_EDGE_MISMATCH: $($report.edges.missing_cold_path.error_code)"
}
if ($report.edges.disk_pressure.error_code -ne "CALYX_DISK_PRESSURE" -or -not $report.edges.disk_pressure.wal_unchanged -or -not $report.edges.disk_pressure.blocked_absent) {
    throw "ISSUE775_FSV_DISK_PRESSURE_EDGE_MISMATCH"
}
if ($report.edges.lock_contention.error_code -ne "CALYX_BACKPRESSURE" -or -not $report.edges.lock_contention.wal_unchanged) {
    throw "ISSUE775_FSV_LOCK_CONTENTION_EDGE_MISMATCH"
}

$files = Get-ChildItem -LiteralPath $fsvRoot -File -Recurse |
    Sort-Object FullName |
    ForEach-Object {
        [pscustomobject]@{
            path = $_.FullName
            bytes = $_.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        }
    }

$aggregate = [ordered]@{
    issue = 775
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    fsv_root = $fsvRoot
    env = [ordered]@{
        calyx_home = $env:CALYX_HOME
        calyx_hot_path = $env:CALYX_HOT_PATH
        calyx_cold_path = $env:CALYX_COLD_PATH
        missing_cold_path = $env:CALYX_ISSUE775_MISSING_COLD_PATH
        cuda_arch = $env:CUDA_ARCH
        cuda_compute_cap = $env:CUDA_COMPUTE_CAP
        calyx_fsv_root = $env:CALYX_FSV_ROOT
    }
    commands = @($commands)
    report_path = $reportPath
    report_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant()
    files = @($files)
    validated = [ordered]@{
        daemon_restart_survived = [bool]$report.daemon_restart.after_second.cx_present
        index_value_hex = $report.daemon_restart.after_second.index_value_hex
        cold_slot_sst_count = ($report.tiered_hot_cold.cold_inactive_slot_ssts | Measure-Object).Count
        missing_cold_error = $report.edges.missing_cold_path.error_code
        disk_pressure_error = $report.edges.disk_pressure.error_code
        lock_contention_error = $report.edges.lock_contention.error_code
    }
}

$aggregatePath = Join-Path $fsvRoot "issue775-windows-local-aster-fsv-harness.json"
$aggregate | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $aggregatePath -Encoding UTF8
$aggregateHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $aggregatePath).Hash.ToLowerInvariant()
Write-Host "ISSUE775_FSV_OK report=$reportPath aggregate=$aggregatePath sha256=$aggregateHash"
