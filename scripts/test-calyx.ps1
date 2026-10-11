[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Package,

    [string]$TestFilter,

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
$manifest = Join-Path $repoRoot "Calyx\Cargo.toml"
$runner = Join-Path $PSScriptRoot "test-rust-package.ps1"

$effectiveFsvRoot = if ($FsvRoot) {
    $FsvRoot
} else {
    Join-Path $repoRoot "target\fsv\test-calyx"
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
if ($NoCapture) {
    $runnerArgs.NoCapture = $true
}
if ($Locked) {
    $runnerArgs.Locked = $true
}

& $runner @runnerArgs
