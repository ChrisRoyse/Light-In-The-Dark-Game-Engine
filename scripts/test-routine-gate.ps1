# Routine edit-cycle gate: lightweight production golden paths that must finish
# in <=30s warm (goal: fast agent feedback). Heavy FSV examples stay on-demand.
# Usage:
#   pwsh -File scripts/test-routine-gate.ps1
# Optional: -Package litd-engine | litd-editor | both (default both)

[CmdletBinding()]
param(
    [ValidateSet("litd-engine", "litd-editor", "both")]
    [string]$Package = "both",

    # Reuse the repository's persistent warm gate cache. This routine gate runs
    # in the same checkout as the pre-push gate; a second cache would cold-build
    # the monolithic engine graph and defeat the <=120s feedback budget.
    [string]$TargetDir = "target\fsv\test-gameeditor\cargo-target",

    # Keep the default tight enough to catch accidental heavy-target regressions;
    # callers may raise it explicitly for a slower host or a cold cache.
    [ValidateRange(1, 3600)]
    [int]$BudgetSeconds = 30,

    # Bound each production golden child independently so a hung cargo/rustc tree
    # cannot wait for the total-budget check to run. Zero follows BudgetSeconds.
    [ValidateRange(0, 3600)]
    [int]$StepTimeoutSeconds = 0
)

Set-StrictMode -Version Latest
# Cargo writes diagnostics to stderr; do not treat that as a terminating error.
$ErrorActionPreference = "Continue"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Set-Location -LiteralPath $repoRoot
$resolvedTargetDir = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $TargetDir))
$repoTargetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
$insideRepo = $resolvedTargetDir.StartsWith($repoRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)
$isCanonicalTargetRoot = $resolvedTargetDir.Equals($repoTargetRoot, [System.StringComparison]::OrdinalIgnoreCase)
if (-not $insideRepo -and -not $isCanonicalTargetRoot) {
    throw "ROUTINE_GATE_TARGET_OUTSIDE_REPO target=$resolvedTargetDir repo=$repoRoot"
}
New-Item -ItemType Directory -Force -Path $resolvedTargetDir | Out-Null

# Fast cycles: force AI addon absent so ORT/panel materialisation is not required.
$env:LITD_CALYX_AI_ADDON = "absent"
$effectiveStepTimeoutSeconds = if ($StepTimeoutSeconds -gt 0) { $StepTimeoutSeconds } else { $BudgetSeconds }
$cargo = (Get-Command cargo -ErrorAction Stop).Source
$outputRoot = Join-Path $repoRoot "target\fsv\test-gameeditor\routine-gate-output"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

function Invoke-BoundedCargo {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds
    )

    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssfffZ")
    $safeLabel = $Label -replace "[^A-Za-z0-9_.-]", "_"
    $stdoutPath = Join-Path $outputRoot "$safeLabel-$stamp.stdout.log"
    $stderrPath = Join-Path $outputRoot "$safeLabel-$stamp.stderr.log"
    $process = $null
    try {
        $process = Start-Process -FilePath $cargo -ArgumentList $Arguments -WorkingDirectory $repoRoot `
            -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath `
            -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit([int]([Math]::Min([long]$TimeoutSeconds * 1000, [int]::MaxValue)))) {
            & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
            $process.WaitForExit()
            $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
            $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
            Write-Host $stdout
            Write-Host $stderr
            throw "ROUTINE_GATE_TIMEOUT label=$Label timeout_seconds=$TimeoutSeconds pid=$($process.Id) stdout=$stdoutPath stderr=$stderrPath"
        }
        $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
        $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
        Write-Host $stdout
        Write-Host $stderr
        return $process.ExitCode
    } finally {
        if ($process -and -not $process.HasExited) {
            & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
        }
        if ($process -and $process.HasExited -and $process.ExitCode -eq 0) {
            Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
        }
    }
}

# The engine gate uses the production golden executable rather than filtering
# the giant lib-test harness. Full-package suites and FSV examples stay
# on-demand for issue close (#800).
$filters = @{
    "litd-engine-routine-golden" = @(
        "production_golden_path"
    )
    # Skip broad `asset_vault` filter: full-corpus fixtures require >=10700 records
    # (#1878 progressive rematerialization). Keep verify_cache + fail-loud edges.
    "litd-editor-routine-golden" = @(
        "routine_golden_path"
    )
}

$packages = if ($Package -eq "both") { @("litd-engine-routine-golden", "litd-editor-routine-golden") } else {
    if ($Package -eq "litd-editor") { @("litd-editor-routine-golden") } else { @($Package) }
}
$packages = @($packages | ForEach-Object {
    if ($_ -eq "litd-engine") { "litd-engine-routine-golden" } else { $_ }
})
$swTotal = [System.Diagnostics.Stopwatch]::StartNew()

foreach ($pkg in $packages) {
    foreach ($filter in $filters[$pkg]) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        Write-Host "ROUTINE_GATE_START package=$pkg filter=$filter"
        $arguments = @()
        if ($pkg -eq "litd-engine-routine-golden") {
            $arguments = @("run", "-p", $pkg, "--locked", "--target-dir", $resolvedTargetDir)
        } else {
            $testTarget = if ($filter -eq "routine_golden_path") { "--test $filter" } else { "--lib -- $filter --test-threads=8" }
            $arguments = if ($filter -eq "routine_golden_path") {
                @("test", "-p", $pkg, "--test", $filter, "--locked", "--target-dir", $resolvedTargetDir)
            } else {
                @("test", "-p", $pkg, "--lib", "--", $filter, "--test-threads=8", "--locked", "--target-dir", $resolvedTargetDir)
            }
        }
        Write-Host "ROUTINE_GATE_COMMAND package=$pkg filter=$filter timeout_seconds=$effectiveStepTimeoutSeconds"
        $code = Invoke-BoundedCargo -Label "$pkg-$filter" -Arguments $arguments -TimeoutSeconds $effectiveStepTimeoutSeconds
        $sw.Stop()
        $sec = [math]::Round($sw.Elapsed.TotalSeconds, 2)
        Write-Host "ROUTINE_GATE_END package=$pkg filter=$filter exit=$code seconds=$sec"
        if ($code -ne 0) {
            Write-Error "ROUTINE_GATE_FAILED package=$pkg filter=$filter exit=$code seconds=$sec"
            exit $code
        }
    }
}

$swTotal.Stop()
$total = [math]::Round($swTotal.Elapsed.TotalSeconds, 2)
Write-Host "ROUTINE_GATE_TOTAL seconds=$total packages=$($packages -join ',')"
if ($total -gt $BudgetSeconds) {
    Write-Host "ROUTINE_GATE_BUDGET_EXCEEDED seconds=$total budget=$BudgetSeconds"
    exit 2
}
Write-Host "ROUTINE_GATE_BUDGET_OK seconds=$total budget=$BudgetSeconds"
exit 0
