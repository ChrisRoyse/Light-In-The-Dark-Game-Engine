Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")

Assert-LitdWindowsPlatform

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$forbiddenEntrypoints = @(
    "scripts/preflight.sh",
    "scripts/package-linux.sh",
    "scripts/package-macos.sh",
    "scripts/package-wsl.ps1",
    "packaging/linux",
    "packaging/macos"
)
$activeForbidden = @($forbiddenEntrypoints | Where-Object {
    Test-Path -LiteralPath (Join-Path $repoRoot $_)
})
if ($activeForbidden.Count -ne 0) {
    throw "LITD_WINDOWS_ALTERNATE_WORKFLOW_ACTIVE: $($activeForbidden -join ',')"
}

$firstPartyManifests = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Filter Cargo.toml -Recurse -File)
$nonWindowsCargoTargets = @($firstPartyManifests | Select-String -Pattern 'target_os\s*=\s*[''"](linux|macos)[''"]|cfg\s*\(unix\)')
if ($nonWindowsCargoTargets.Count -ne 0) {
    throw "LITD_WINDOWS_CARGO_TARGET_ACTIVE: $($nonWindowsCargoTargets.Path -join ',')"
}

# This wrapper is a manual secret-injection compatibility path only. It is excluded from
# product/runtime/gate workflow counts by #800's DeepSeek scope boundary.
$manualBashExceptions = @("scripts/infisical-run.sh")
foreach ($relative in $manualBashExceptions) {
    if (-not (Test-Path -LiteralPath (Join-Path $repoRoot $relative) -PathType Leaf)) {
        throw "LITD_WINDOWS_AUDIT_CLASSIFICATION_STALE: missing classified manual wrapper $relative"
    }
}
$edgeCodes = @()
foreach ($probe in @(
    { Assert-LitdWindowsPlatform -Platform "Linux x86_64" },
    { Assert-LitdWindowsPath -Path "/home/operator/calyx" -Label "Calyx root" },
    { Assert-LitdWindowsPath -Path (Join-Path $env:TEMP "litd-definitely-missing-onnxruntime.dll") -Label "ONNX Runtime" -RequireLeaf }
)) {
    try {
        & $probe
        throw "LITD_WINDOWS_CONTRACT_FALSE_GREEN: an invalid platform contract was accepted"
    } catch {
        if ($_.Exception.Message -like "LITD_WINDOWS_CONTRACT_FALSE_GREEN:*") { throw }
        $edgeCodes += ($_.Exception.Message -split ':', 2)[0]
    }
}
$expected = @("LITD_WINDOWS_PLATFORM_REQUIRED", "LITD_WINDOWS_PATH_REQUIRED", "LITD_WINDOWS_RUNTIME_MISSING")
if (Compare-Object $expected $edgeCodes) {
    throw "LITD_WINDOWS_CONTRACT_EDGE_MISMATCH: expected=$($expected -join ',') actual=$($edgeCodes -join ',')"
}
Write-Host "WINDOWS_PLATFORM_GATE_OK platform=native-windows edges=$($edgeCodes -join ',') first_party_manifests=$($firstPartyManifests.Count) active_non_windows_workflows=0 manual_bash_exceptions=$($manualBashExceptions.Count)"
