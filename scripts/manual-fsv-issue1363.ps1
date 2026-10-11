param(
    [string]$FsvRoot = "target\fsv\issue1363_manual_fsv"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$gateScript = Join-Path $PSScriptRoot "check-calyx-side-store-eradication.ps1"
$root = if ([System.IO.Path]::IsPathRooted($FsvRoot)) {
    $FsvRoot
} else {
    Join-Path $repoRoot $FsvRoot
}
$runId = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$runRoot = Join-Path $root $runId
$fixturesRoot = Join-Path $runRoot "fixtures"
$reportsRoot = Join-Path $runRoot "gate-reports"
New-Item -ItemType Directory -Force -Path $fixturesRoot, $reportsRoot | Out-Null

function Get-PathState {
    param([Parameter(Mandatory = $true)][string]$Path)
    $exists = Test-Path -LiteralPath $Path
    $isFile = Test-Path -LiteralPath $Path -PathType Leaf
    $isDir = Test-Path -LiteralPath $Path -PathType Container
    $fileCount = 0
    if ($isDir) {
        $fileCount = @(Get-ChildItem -LiteralPath $Path -File).Count
    }
    [pscustomobject][ordered]@{
        path = $Path
        exists = $exists
        is_file = $isFile
        is_dir = $isDir
        file_count = $fileCount
    }
}

function Invoke-GateReadback {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [string[]]$SourceRoot = @()
    )
    $reportDir = Join-Path $reportsRoot $Label
    New-Item -ItemType Directory -Force -Path $reportDir | Out-Null
    $args = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $gateScript,
        "-FsvRoot",
        $reportDir
    )
    if ($SourceRoot.Count -gt 0) {
        $args += "-SourceRoot"
        $args += $SourceRoot
    }
    $gateOutput = & powershell @args 2>&1
    $exitCode = $LASTEXITCODE
    foreach ($line in $gateOutput) {
        Write-Host $line
    }
    $reportPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.json" -File |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $reportPath) {
        throw "ISSUE1363_FSV_FAILED: gate did not write a report for $Label"
    }
    $json = Get-Content -Raw -LiteralPath $reportPath.FullName | ConvertFrom-Json
    $sha = (Get-FileHash -LiteralPath $reportPath.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    [pscustomobject][ordered]@{
        label = $Label
        exit_code = $exitCode
        report_path = $reportPath.FullName
        report_sha256 = $sha
        status = $json.status
        scanned_rust_file_count = $json.scanned_rust_file_count
        violation_count = $json.violation_count
        categories = @($json.violations | ForEach-Object { $_.category })
        first_violation = if ($json.violation_count -gt 0) { $json.violations[0] } else { $null }
    }
}

function Write-Fixture {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$FileName,
        [Parameter(Mandatory = $true)][string]$Content
    )
    New-Item -ItemType Directory -Force -Path $Directory | Out-Null
    $path = Join-Path $Directory $FileName
    Set-Content -LiteralPath $path -Value $Content -Encoding UTF8
    return $path
}

$productionBefore = Get-PathState -Path $repoRoot
$production = Invoke-GateReadback -Label "production"
$productionAfter = Get-PathState -Path $repoRoot

$emptyRoot = Join-Path $fixturesRoot "empty-root"
New-Item -ItemType Directory -Force -Path $emptyRoot | Out-Null
$emptyBefore = Get-PathState -Path $emptyRoot
$empty = Invoke-GateReadback -Label "edge-empty-scan" -SourceRoot @($emptyRoot)
$emptyAfter = Get-PathState -Path $emptyRoot

$sourceTruthRoot = Join-Path $fixturesRoot "source-truth-json"
$sourceTruthContent = @'
pub fn report() -> &'static str {
    "source_of_truth: legacy JSON side store"
}
'@
$sourceTruthFile = Write-Fixture `
    -Directory $sourceTruthRoot `
    -FileName "legacy_source_store.rs" `
    -Content $sourceTruthContent
$sourceTruthBefore = Get-PathState -Path $sourceTruthRoot
$sourceTruth = Invoke-GateReadback -Label "edge-source-truth-json" -SourceRoot @($sourceTruthRoot)
$sourceTruthAfter = Get-PathState -Path $sourceTruthRoot

$writeRoot = Join-Path $fixturesRoot "direct-write"
$writeContent = @'
pub fn write(path: &std::path::Path, bytes: &[u8]) {
    let _ = std::fs::write(path, bytes);
}
'@
$writeFile = Write-Fixture `
    -Directory $writeRoot `
    -FileName "bad_store.rs" `
    -Content $writeContent
$writeBefore = Get-PathState -Path $writeRoot
$write = Invoke-GateReadback -Label "edge-direct-write" -SourceRoot @($writeRoot)
$writeAfter = Get-PathState -Path $writeRoot

$summary = [ordered]@{
    schema_version = 1
    issue = 1363
    source_of_truth = "manual FSV report JSON plus gate report JSON readbacks on disk"
    run_root = $runRoot
    production = [ordered]@{
        before = $productionBefore
        after = $productionAfter
        readback = $production
        passed = ($production.exit_code -eq 0 -and $production.status -eq "passed" -and $production.violation_count -eq 0)
    }
    edges = [ordered]@{
        empty_scan = [ordered]@{
            before = $emptyBefore
            after = $emptyAfter
            expected_category = "empty_scan"
            readback = $empty
            passed = ($empty.exit_code -ne 0 -and $empty.categories -contains "empty_scan")
        }
        source_truth_json = [ordered]@{
            fixture = $sourceTruthFile
            before = $sourceTruthBefore
            after = $sourceTruthAfter
            expected_category = "source_of_truth_names_side_store"
            readback = $sourceTruth
            passed = ($sourceTruth.exit_code -ne 0 -and $sourceTruth.categories -contains "source_of_truth_names_side_store")
        }
        direct_write = [ordered]@{
            fixture = $writeFile
            before = $writeBefore
            after = $writeAfter
            expected_category = "direct_structured_file_write"
            readback = $write
            passed = ($write.exit_code -ne 0 -and $write.categories -contains "direct_structured_file_write")
        }
    }
}
$summaryPath = Join-Path $runRoot "manual-side-store-fsv-summary.json"
$summaryJson = $summary | ConvertTo-Json -Depth 20
$summaryJson | Set-Content -LiteralPath $summaryPath -Encoding UTF8
$summaryReadback = Get-Content -Raw -LiteralPath $summaryPath | ConvertFrom-Json
$summarySha = (Get-FileHash -LiteralPath $summaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
$allPassed = $summaryReadback.production.passed `
    -and $summaryReadback.edges.empty_scan.passed `
    -and $summaryReadback.edges.source_truth_json.passed `
    -and $summaryReadback.edges.direct_write.passed
Write-Host "ISSUE1363_MANUAL_FSV_READBACK path=$summaryPath sha256=$summarySha production=$($summaryReadback.production.passed) empty=$($summaryReadback.edges.empty_scan.passed) source_truth=$($summaryReadback.edges.source_truth_json.passed) direct_write=$($summaryReadback.edges.direct_write.passed)"
if (-not $allPassed) {
    exit 1
}
exit 0
