Set-StrictMode -Version Latest

function ConvertTo-LitdWindowsPackageVersion {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw "LITD_WINDOWS_PACKAGE_VERSION_INVALID: expected canonical major.minor.patch, got '$Value'"
    }
    $major = 0u
    $minor = 0u
    $patch = 0u
    if (
        -not [uint32]::TryParse($Matches[1], [ref]$major) -or
        -not [uint32]::TryParse($Matches[2], [ref]$minor) -or
        -not [uint32]::TryParse($Matches[3], [ref]$patch)
    ) {
        throw "LITD_WINDOWS_PACKAGE_VERSION_INVALID: version component overflow in '$Value'"
    }
    if ($major -gt 255 -or $minor -gt 255 -or $patch -gt 65535) {
        throw "LITD_WINDOWS_PACKAGE_VERSION_INVALID: Windows Installer limits are major<=255 minor<=255 patch<=65535; got '$Value'"
    }
    [pscustomobject]@{
        text = "$major.$minor.$patch"
        major = $major
        minor = $minor
        patch = $patch
    }
}

function Compare-LitdWindowsPackageVersion {
    param(
        [Parameter(Mandatory = $true)][string]$Left,
        [Parameter(Mandatory = $true)][string]$Right
    )

    $leftParts = ConvertTo-LitdWindowsPackageVersion -Value $Left
    $rightParts = ConvertTo-LitdWindowsPackageVersion -Value $Right
    foreach ($field in @("major", "minor", "patch")) {
        if ($leftParts.$field -lt $rightParts.$field) {
            return -1
        }
        if ($leftParts.$field -gt $rightParts.$field) {
            return 1
        }
    }
    return 0
}

function Get-LitdRequiredJsonProperty {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Context
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $null -eq $property.Value) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: $Context is missing '$Name'"
    }
    return $property.Value
}

function ConvertTo-LitdWindowsReleaseArtifact {
    param(
        [Parameter(Mandatory = $true)]$Artifact,
        [Parameter(Mandatory = $true)][string]$Context
    )

    $kind = [string](Get-LitdRequiredJsonProperty -Object $Artifact -Name "kind" -Context $Context)
    $expectedFile = switch ($kind) {
        "portable_zip" { "litd-windows-x64.zip" }
        "windows_msi" { "litd-windows-x64.msi" }
        default {
            throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: $Context kind must be portable_zip or windows_msi, got '$kind'"
        }
    }
    $file = [string](Get-LitdRequiredJsonProperty -Object $Artifact -Name "file" -Context $Context)
    if ($file -ne $expectedFile -or $file -ne [IO.Path]::GetFileName($file)) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: $Context file for $kind must be '$expectedFile', got '$file'"
    }
    $bytes = Get-LitdRequiredJsonProperty -Object $Artifact -Name "bytes" -Context $Context
    $parsedBytes = 0L
    if (-not [Int64]::TryParse([string]$bytes, [ref]$parsedBytes) -or $parsedBytes -le 0) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: $Context bytes must be a positive integer"
    }
    $sha256 = [string](Get-LitdRequiredJsonProperty -Object $Artifact -Name "sha256" -Context $Context)
    if ($sha256 -notmatch '^[0-9a-f]{64}$') {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: $Context sha256 must be 64 lowercase hexadecimal characters"
    }
    [pscustomobject]@{
        kind = $kind
        file = $file
        bytes = $parsedBytes
        sha256 = $sha256
    }
}

function Read-LitdWindowsReleaseManifest {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_MISSING: release manifest not found at $Path"
    }
    try {
        $manifest = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    } catch {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: release manifest JSON failed to parse: $($_.Exception.Message)"
    }
    $schema = [string](Get-LitdRequiredJsonProperty -Object $manifest -Name "schema" -Context "release manifest")
    if ($schema -ne "litd.windows.release.v2") {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: expected schema litd.windows.release.v2, got '$schema'"
    }
    $packageName = [string](Get-LitdRequiredJsonProperty -Object $manifest -Name "package_name" -Context "release manifest")
    if ($packageName -ne "Light in the Dark") {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: package_name must be 'Light in the Dark', got '$packageName'"
    }
    $version = [string](Get-LitdRequiredJsonProperty -Object $manifest -Name "version" -Context "release manifest")
    try {
        $versionParts = ConvertTo-LitdWindowsPackageVersion -Value $version
    } catch {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: invalid candidate version '$version': $($_.Exception.Message)"
    }
    $gitCommit = [string](Get-LitdRequiredJsonProperty -Object $manifest -Name "git_commit" -Context "release manifest")
    if ($gitCommit -notmatch '^[0-9a-f]{40}$') {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: git_commit must be 40 lowercase hexadecimal characters"
    }
    $primaryKind = [string](Get-LitdRequiredJsonProperty -Object $manifest -Name "primary_artifact" -Context "release manifest")
    if ($primaryKind -notin @("portable_zip", "windows_msi")) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: primary_artifact must be portable_zip or windows_msi, got '$primaryKind'"
    }
    $artifactValues = @(Get-LitdRequiredJsonProperty -Object $manifest -Name "artifacts" -Context "release manifest")
    if ($artifactValues.Count -lt 1 -or $artifactValues.Count -gt 2) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: artifacts must contain one or two rows, got $($artifactValues.Count)"
    }
    $artifacts = @()
    $seenKinds = @{}
    for ($index = 0; $index -lt $artifactValues.Count; $index++) {
        $artifact = ConvertTo-LitdWindowsReleaseArtifact `
            -Artifact $artifactValues[$index] `
            -Context "release manifest artifacts[$index]"
        if ($seenKinds.ContainsKey($artifact.kind)) {
            throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: duplicate artifact kind '$($artifact.kind)'"
        }
        $seenKinds[$artifact.kind] = $true
        $artifacts += $artifact
    }
    if (-not $seenKinds.ContainsKey("portable_zip")) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: artifacts must contain portable_zip"
    }
    if (-not $seenKinds.ContainsKey($primaryKind)) {
        throw "LITD_WINDOWS_UPDATE_MANIFEST_INVALID: primary_artifact '$primaryKind' has no matching artifact row"
    }
    $primary = @($artifacts | Where-Object kind -EQ $primaryKind)[0]
    [pscustomobject]@{
        schema = $schema
        package_name = $packageName
        version = $versionParts.text
        git_commit = $gitCommit
        primary_artifact_kind = $primaryKind
        primary_artifact = $primary
        artifacts = $artifacts
    }
}

function Get-LitdWindowsReleaseManifestRow {
    param([Parameter(Mandatory = $true)][string]$Path)

    $manifest = Read-LitdWindowsReleaseManifest -Path $Path
    [ordered]@{
        schema = $manifest.schema
        version = $manifest.version
        primary_artifact = $manifest.primary_artifact_kind
        artifacts = @($manifest.artifacts | ForEach-Object {
            [ordered]@{
                kind = $_.kind
                file = $_.file
                bytes = $_.bytes
                sha256 = $_.sha256
            }
        })
        manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }
}

function Write-LitdWindowsReleaseManifest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$PackageVersion,
        [Parameter(Mandatory = $true)][string]$GitCommit,
        [Parameter(Mandatory = $true)][string]$PortableZipPath,
        [string]$WindowsMsiPath,
        [ValidateSet("portable_zip", "windows_msi")][string]$PrimaryArtifact = "portable_zip"
    )

    $version = (ConvertTo-LitdWindowsPackageVersion -Value $PackageVersion).text
    if ($GitCommit -notmatch '^[0-9a-f]{40}$') {
        throw "LITD_WINDOWS_PACKAGE_GIT_COMMIT_INVALID: expected 40 lowercase hexadecimal characters"
    }
    $artifactPaths = [ordered]@{
        portable_zip = $PortableZipPath
    }
    if (-not [string]::IsNullOrWhiteSpace($WindowsMsiPath)) {
        $artifactPaths.windows_msi = $WindowsMsiPath
    }
    if (-not $artifactPaths.Contains($PrimaryArtifact)) {
        throw "LITD_WINDOWS_PACKAGE_RELEASE_PRIMARY_INVALID: primary artifact '$PrimaryArtifact' was not built"
    }
    $artifacts = @()
    foreach ($kind in $artifactPaths.Keys) {
        $artifactPath = $artifactPaths[$kind]
        if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
            throw "LITD_WINDOWS_PACKAGE_ARTIFACT_MISSING: $kind release artifact not found at $artifactPath"
        }
        $artifact = Get-Item -LiteralPath $artifactPath
        $artifacts += [ordered]@{
            kind = $kind
            file = $artifact.Name
            bytes = $artifact.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact.FullName).Hash.ToLowerInvariant()
        }
    }
    $manifest = [ordered]@{
        schema = "litd.windows.release.v2"
        package_name = "Light in the Dark"
        version = $version
        git_commit = $GitCommit
        primary_artifact = $PrimaryArtifact
        artifacts = $artifacts
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Path -Encoding UTF8
    Get-LitdWindowsReleaseManifestRow -Path $Path
}
