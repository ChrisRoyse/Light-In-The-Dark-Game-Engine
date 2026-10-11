[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Package,

    [string]$TestFilter,

    # Select one explicit integration-test target. This keeps the routine gate on the
    # production-only golden path instead of compiling the package's cfg(test) unit-test graph.
    [string]$TestTarget,

    [string[]]$Examples,

    [string[]]$Features,

    [string]$FsvRoot,

    [string]$TargetDir,

    [switch]$NoCapture,

    [switch]$Locked,

    [int]$LockTimeoutSeconds = 600,

    [int]$TestTimeoutSeconds = 900
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $repoRoot "Cargo.toml"
$runner = Join-Path $PSScriptRoot "test-rust-package.ps1"
$runtime = & (Join-Path $PSScriptRoot "Set-GameEditorCalyxRuntimeEnv.ps1") -PassThru
Write-Host "TEST_GATE_RUNTIME_READY calyx_home=$($runtime.calyx_home) ort_sha256=$($runtime.ort_dylib_sha256)"

$effectiveFsvRoot = if ($FsvRoot) {
    $FsvRoot
} else {
    Join-Path $repoRoot "target\fsv\test-gameeditor"
}
$effectiveTargetDir = if ($TargetDir) { $TargetDir } else { Join-Path $effectiveFsvRoot "cargo-target" }

$runnerArgs = @{
    ManifestPath = $manifest
    Package = $Package
    LockTimeoutSeconds = $LockTimeoutSeconds
    TestTimeoutSeconds = $TestTimeoutSeconds
    FsvRoot = $effectiveFsvRoot
    TargetDir = $effectiveTargetDir
}
if ($TestFilter) {
    $runnerArgs.TestFilter = $TestFilter
}
if ($TestTarget) {
    $runnerArgs.TestTarget = $TestTarget
}
if ($Examples -and $Examples.Count -gt 0) {
    $runnerArgs.Examples = $Examples
}
if ($Features -and $Features.Count -gt 0) {
    $runnerArgs.Features = $Features
}
if ($NoCapture) {
    $runnerArgs.NoCapture = $true
}
if ($Locked) {
    $runnerArgs.Locked = $true
}

& $runner @runnerArgs
