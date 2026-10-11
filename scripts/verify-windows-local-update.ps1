param(
    [Parameter(Mandatory = $true)][string]$ReleaseManifestPath,
    [Parameter(Mandatory = $true)][string]$CurrentVersion
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
. (Join-Path $PSScriptRoot "WindowsPackageContract.ps1")
Assert-LitdWindowsPlatform

$manifestPath = if ([IO.Path]::IsPathRooted($ReleaseManifestPath)) {
    $ReleaseManifestPath
} else {
    $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ReleaseManifestPath)
}
$manifest = Read-LitdWindowsReleaseManifest -Path $manifestPath
$current = (ConvertTo-LitdWindowsPackageVersion -Value $CurrentVersion).text
$artifactReadbacks = @()
foreach ($expected in $manifest.artifacts) {
    $artifactPath = Join-Path (Split-Path -Parent $manifestPath) $expected.file
    if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        throw "LITD_WINDOWS_UPDATE_ARTIFACT_MISSING: $($expected.kind) release artifact not found at $artifactPath"
    }
    $artifact = Get-Item -LiteralPath $artifactPath
    $actualSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifactPath).Hash.ToLowerInvariant()
    if ($artifact.Length -ne $expected.bytes -or $actualSha256 -ne $expected.sha256) {
        throw "LITD_WINDOWS_UPDATE_ARTIFACT_HASH_MISMATCH: $($expected.kind) expected bytes=$($expected.bytes) sha256=$($expected.sha256); actual bytes=$($artifact.Length) sha256=$actualSha256"
    }
    $artifactReadbacks += [ordered]@{
        kind = $expected.kind
        file = $expected.file
        bytes = $artifact.Length
        sha256 = $actualSha256
    }
}
$primary = @($artifactReadbacks | Where-Object kind -EQ $manifest.primary_artifact_kind)[0]

$comparison = Compare-LitdWindowsPackageVersion -Left $manifest.version -Right $current
if ($comparison -lt 0) {
    throw "LITD_WINDOWS_UPDATE_DOWNGRADE_REFUSED: installed version $current is newer than candidate $($manifest.version)"
}
$status = if ($comparison -eq 0) { "current" } else { "update_available" }
$result = [ordered]@{
    schema = "litd.windows.update.verification.v2"
    status = $status
    current_version = $current
    candidate_version = $manifest.version
    git_commit = $manifest.git_commit
    primary_artifact = $manifest.primary_artifact_kind
    artifact_file = $primary.file
    artifact_bytes = $primary.bytes
    artifact_sha256 = $primary.sha256
    artifacts = $artifactReadbacks
    manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
}
$result | ConvertTo-Json -Depth 5 -Compress
