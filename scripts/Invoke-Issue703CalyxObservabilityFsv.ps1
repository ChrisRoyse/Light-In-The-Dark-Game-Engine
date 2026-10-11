[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "CalyxCanonicalPaths.ps1")

$calyxDir = Get-GameEditorCanonicalCalyxHome
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$fsvRoot = Join-Path $RepoRoot "target\fsv\issue703_calyx_observability\$stamp"
New-Item -ItemType Directory -Force -Path $fsvRoot | Out-Null

function Start-OwnedCargo {
    param(
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $calyxDir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment["ISSUE703_FSV_ROOT"] = $fsvRoot
    foreach ($arg in @("run", "-p", "calyxd", "--example", "issue703_bridge_observability_fsv")) {
        [void]$psi.ArgumentList.Add($arg)
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $psi
    [void]$process.Start()
    [pscustomobject]@{
        Process = $process
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
    }
}

function Complete-OwnedProcess {
    param(
        [Parameter(Mandatory = $true)]$Owned,
        [int]$TimeoutMs = 180000
    )
    $process = [System.Diagnostics.Process]$Owned.Process
    if (-not $process.WaitForExit($TimeoutMs)) {
        $process.Kill()
        [void]$process.WaitForExit(10000)
        throw "ISSUE703_FSV_TIMEOUT: killed only owned cargo process pid=$($process.Id)"
    }
    $stdoutText = $process.StandardOutput.ReadToEnd()
    $stderrText = $process.StandardError.ReadToEnd()
    Set-Content -LiteralPath $Owned.StdoutPath -Value $stdoutText -Encoding UTF8
    Set-Content -LiteralPath $Owned.StderrPath -Value $stderrText -Encoding UTF8
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        Stdout = $stdoutText
        Stderr = $stderrText
        StdoutPath = $Owned.StdoutPath
        StderrPath = $Owned.StderrPath
    }
}

$stdoutPath = Join-Path $fsvRoot "issue703-example.stdout.log"
$stderrPath = Join-Path $fsvRoot "issue703-example.stderr.log"
$owned = Start-OwnedCargo -StdoutPath $stdoutPath -StderrPath $stderrPath
$completed = Complete-OwnedProcess -Owned $owned
if ($completed.ExitCode -ne 0) {
    throw "ISSUE703_FSV_EXAMPLE_FAILED: exit=$($completed.ExitCode) stderr=$($completed.Stderr)"
}

$line = @($completed.Stdout -split "`r?`n" |
    Where-Object { $_ -match "^ISSUE703_BRIDGE_OBSERVABILITY_FSV " } |
    Select-Object -Last 1)
if ($line.Count -ne 1) {
    throw "ISSUE703_FSV_MISSING_REPORT_LINE: stdout=$($completed.Stdout)"
}
if ($line[0] -notmatch "report=(?<report>.+?) sha256=(?<sha>[0-9a-fA-F]{64}) root=(?<root>.+)$") {
    throw "ISSUE703_FSV_BAD_REPORT_LINE: $($line[0])"
}

$childReportPath = $Matches.report
$childSha = $Matches.sha.ToLowerInvariant()
if (-not (Test-Path -LiteralPath $childReportPath -PathType Leaf)) {
    throw "ISSUE703_FSV_REPORT_MISSING: $childReportPath"
}
$actualChildSha = (Get-FileHash -LiteralPath $childReportPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualChildSha -ne $childSha) {
    throw "ISSUE703_FSV_REPORT_HASH_MISMATCH: expected=$childSha actual=$actualChildSha"
}

$child = Get-Content -LiteralPath $childReportPath -Raw | ConvertFrom-Json
foreach ($edge in @(
        "bridge_error_counter_incremented",
        "bridge_latency_count_incremented",
        "chain_break_ok_flipped_zero",
        "metrics_endpoint_down_refused"
    )) {
    if ($child.edges.$edge -ne $true) {
        throw "ISSUE703_FSV_EDGE_FALSE: $edge"
    }
}

if ($completed.Stdout -notmatch "CALYX_DAEMON_MCP_REQUEST_OK") {
    throw "ISSUE703_FSV_MISSING_SUCCESS_LOG_CODE"
}
if ($completed.Stderr -notmatch "CALYX_DAEMON_MCP_REQUEST_ERROR" -or
    $completed.Stderr -notmatch "CALYX_DAEMON_MCP_METHOD_NOT_FOUND") {
    throw "ISSUE703_FSV_MISSING_ERROR_LOG_CODE"
}

$alertsPath = Join-Path $calyxDir "infra\aiwonder\alertmanager\calyx-alerts.yml"
$alertTestPath = Join-Path $calyxDir "infra\aiwonder\alertmanager\calyx-alerts.test.yml"
$alertsText = Get-Content -LiteralPath $alertsPath -Raw
$alertTestText = Get-Content -LiteralPath $alertTestPath -Raw
foreach ($required in @(
        "CalyxChainVerifyFail",
        "calyx_ledger_chain_verify_ok == 0",
        "CalyxMetricsEndpointDown",
        "up{job=`"calyxd`"} == 0",
        "CalyxBridgeErrorRateHigh",
        "calyx_bridge_errors_total",
        "calyx_bridge_requests_total"
    )) {
    if ($alertsText -notmatch [Regex]::Escape($required)) {
        throw "ISSUE703_FSV_ALERT_RULE_MISSING: $required"
    }
}
foreach ($required in @("CalyxMetricsEndpointDown", "CalyxBridgeErrorRateHigh")) {
    if ($alertTestText -notmatch [Regex]::Escape($required)) {
        throw "ISSUE703_FSV_ALERT_TEST_MISSING: $required"
    }
}

$aggregate = [ordered]@{
    issue = 703
    timestamp = (Get-Date).ToUniversalTime().ToString("o")
    source_of_truth = "owned cargo-run FSV child process, child stdout/stderr structured CALYX_* logs, child JSON report, Prometheus /metrics text files, real tampered DirectoryLedgerStore bytes, and repo alert-rule files"
    fsv_root = $fsvRoot
    child_report_path = $childReportPath
    child_report_sha256 = $actualChildSha
    child_report = $child
    stdout_path = $stdoutPath
    stderr_path = $stderrPath
    stdout_sha256 = (Get-FileHash -LiteralPath $stdoutPath -Algorithm SHA256).Hash.ToLowerInvariant()
    stderr_sha256 = (Get-FileHash -LiteralPath $stderrPath -Algorithm SHA256).Hash.ToLowerInvariant()
    structured_logs = [ordered]@{
        success_code_present = ($completed.Stdout -match "CALYX_DAEMON_MCP_REQUEST_OK")
        error_code_present = ($completed.Stderr -match "CALYX_DAEMON_MCP_REQUEST_ERROR")
        detail_code_present = ($completed.Stderr -match "CALYX_DAEMON_MCP_METHOD_NOT_FOUND")
    }
    alerts = [ordered]@{
        rules_path = $alertsPath
        tests_path = $alertTestPath
        chain_verify_alert_present = ($alertsText -match "CalyxChainVerifyFail")
        metrics_endpoint_down_alert_present = ($alertsText -match "CalyxMetricsEndpointDown")
        bridge_error_rate_alert_present = ($alertsText -match "CalyxBridgeErrorRateHigh")
    }
    owned_process_cleanup = "The script waited for the owned cargo/example process to exit. On timeout it would kill only that owned process; no unrelated processes are controlled."
}

$aggregatePath = Join-Path $fsvRoot "issue703-calyx-observability-fsv.json"
$aggregate | ConvertTo-Json -Depth 80 | Set-Content -LiteralPath $aggregatePath -Encoding UTF8
$aggregateSha = (Get-FileHash -LiteralPath $aggregatePath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "ISSUE703_FSV_REPORT=$aggregatePath"
Write-Host "ISSUE703_FSV_SHA256=$aggregateSha"
Write-Host "ISSUE703_FSV_ROOT=$fsvRoot"
