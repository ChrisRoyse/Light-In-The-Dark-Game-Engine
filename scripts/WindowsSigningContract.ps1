Set-StrictMode -Version Latest

function Get-LitdWindowsSigningConfiguration {
    param(
        [string]$SignToolPath,
        [string]$CertificateThumbprint,
        [string]$TimestampUrl
    )

    $hasTool = -not [string]::IsNullOrWhiteSpace($SignToolPath)
    $hasCertificate = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
    if ($hasTool -ne $hasCertificate) {
        throw "LITD_WINDOWS_SIGNING_CONFIG_PARTIAL: SignToolPath and CertificateThumbprint must be provided together"
    }
    if (-not $hasTool) {
        return [ordered]@{
            enabled = $false
            mode = "unsigned"
            sign_tool_path = ""
            certificate_thumbprint = ""
            timestamp_url = ""
        }
    }

    if (-not (Test-Path -LiteralPath $SignToolPath -PathType Leaf)) {
        throw "LITD_WINDOWS_SIGNING_TOOL_MISSING: SignTool not found at $SignToolPath"
    }
    $resolvedTool = (Resolve-Path -LiteralPath $SignToolPath).Path
    $thumbprint = ($CertificateThumbprint -replace "\s", "").ToUpperInvariant()
    if ($thumbprint -cnotmatch "^[0-9A-F]{40}$") {
        throw "LITD_WINDOWS_SIGNING_THUMBPRINT_INVALID: expected a 40-hex SHA-1 certificate thumbprint"
    }
    $timestamp = $null
    if (
        [string]::IsNullOrWhiteSpace($TimestampUrl) -or
        -not [Uri]::TryCreate($TimestampUrl, [UriKind]::Absolute, [ref]$timestamp) -or
        ($timestamp.Scheme -cne "http" -and $timestamp.Scheme -cne "https")
    ) {
        throw "LITD_WINDOWS_SIGNING_TIMESTAMP_INVALID: expected an absolute HTTP(S) timestamp URL"
    }

    [ordered]@{
        enabled = $true
        mode = "authenticode_required"
        sign_tool_path = $resolvedTool
        certificate_thumbprint = $thumbprint
        timestamp_url = $timestamp.AbsoluteUri
    }
}

function Get-LitdWindowsSignArguments {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$Configuration,
        [Parameter(Mandatory = $true)][string]$Artifact
    )
    @(
        "sign", "/fd", "SHA256",
        "/sha1", [string]$Configuration.certificate_thumbprint,
        "/tr", [string]$Configuration.timestamp_url,
        "/td", "SHA256",
        $Artifact
    )
}

function Get-LitdWindowsVerifyArguments {
    param([Parameter(Mandatory = $true)][string]$Artifact)
    @("verify", "/pa", "/all", "/v", $Artifact)
}

function Invoke-LitdWindowsSigningTool {
    param(
        [Parameter(Mandatory = $true)][string]$SignToolPath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Operation,
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )
    Push-Location -LiteralPath $WorkingDirectory
    try {
        $output = @(& $SignToolPath @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
        $output | ForEach-Object { Write-Host $_ }
        if ($exitCode -ne 0) {
            throw "LITD_WINDOWS_SIGNING_TOOL_FAILED: operation=$Operation artifact=$Label exit_code=$exitCode"
        }
    } finally {
        Pop-Location
    }
}

function Invoke-LitdWindowsAuthenticodeSigning {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$Configuration,
        [Parameter(Mandatory = $true)][string]$Artifact,
        [Parameter(Mandatory = $true)][string]$Label
    )

    if (-not (Test-Path -LiteralPath $Artifact -PathType Leaf)) {
        throw "LITD_WINDOWS_SIGNING_ARTIFACT_MISSING: artifact=$Label path=$Artifact"
    }
    $resolved = (Resolve-Path -LiteralPath $Artifact).Path
    if ($Configuration.enabled) {
        Invoke-LitdWindowsSigningTool `
            -SignToolPath $Configuration.sign_tool_path `
            -Arguments (Get-LitdWindowsSignArguments -Configuration $Configuration -Artifact $resolved) `
            -Operation "sign" `
            -Label $Label `
            -WorkingDirectory (Split-Path -Parent $resolved)
        Invoke-LitdWindowsSigningTool `
            -SignToolPath $Configuration.sign_tool_path `
            -Arguments (Get-LitdWindowsVerifyArguments -Artifact $resolved) `
            -Operation "verify" `
            -Label $Label `
            -WorkingDirectory (Split-Path -Parent $resolved)
    }

    $item = Get-Item -LiteralPath $resolved
    [ordered]@{
        label = $Label
        file = $item.Name
        status = if ($Configuration.enabled) { "signed_verified" } else { "unsigned" }
        bytes = $item.Length
        sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Get-LitdStreamSha256 {
    param([Parameter(Mandatory = $true)][IO.Stream]$Stream)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        ([BitConverter]::ToString($sha.ComputeHash($Stream))).Replace("-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Test-LitdWindowsPortableZipSigningCoverage {
    param(
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [Parameter(Mandatory = $true)][string]$StageRoot,
        [Parameter(Mandatory = $true)][object[]]$BinarySigningRows,
        [Parameter(Mandatory = $true)][bool]$SigningEnabled
    )

    if (-not (Test-Path -LiteralPath $ZipPath -PathType Leaf)) {
        throw "LITD_WINDOWS_SIGNING_ZIP_MISSING: ZIP not found at $ZipPath"
    }
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $expected = @(
        [pscustomobject]@{ label = "editor"; path = "app/litd-editor.exe" }
        [pscustomobject]@{ label = "game"; path = "app/litd-game.exe" }
    )
    $archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $ZipPath).Path)
    try {
        $readback = @()
        foreach ($binary in $expected) {
            $rows = @($BinarySigningRows | Where-Object { $_.label -ceq $binary.label })
            if ($rows.Count -ne 1) {
                throw "LITD_WINDOWS_SIGNING_BINARY_ROW_INVALID: label=$($binary.label) expected_rows=1 actual_rows=$($rows.Count)"
            }
            $expectedStatus = if ($SigningEnabled) { "signed_verified" } else { "unsigned" }
            if ($rows[0].status -cne $expectedStatus) {
                throw "LITD_WINDOWS_SIGNING_BINARY_STATUS_INVALID: label=$($binary.label) expected=$expectedStatus actual=$($rows[0].status)"
            }
            $entries = @($archive.Entries | Where-Object { $_.FullName -ceq $binary.path })
            if ($entries.Count -ne 1) {
                throw "LITD_WINDOWS_SIGNING_ZIP_ENTRY_INVALID: path=$($binary.path) expected_rows=1 actual_rows=$($entries.Count)"
            }
            $source = Join-Path $StageRoot $binary.path
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
                throw "LITD_WINDOWS_SIGNING_ARTIFACT_MISSING: artifact=$($binary.label) path=$source"
            }
            $stream = $entries[0].Open()
            try {
                $embeddedHash = Get-LitdStreamSha256 -Stream $stream
            } finally {
                $stream.Dispose()
            }
            $sourceItem = Get-Item -LiteralPath $source
            $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
            if (
                $entries[0].Length -ne $sourceItem.Length -or
                $embeddedHash -cne $sourceHash -or
                $sourceHash -cne $rows[0].sha256
            ) {
                throw "LITD_WINDOWS_SIGNING_ZIP_BINARY_MISMATCH: path=$($binary.path)"
            }
            $readback += [ordered]@{
                path = $binary.path
                bytes = $entries[0].Length
                sha256 = $embeddedHash
                authenticode_status = $rows[0].status
            }
        }
    } finally {
        $archive.Dispose()
    }

    $zipItem = Get-Item -LiteralPath $ZipPath
    [ordered]@{
        file = $zipItem.Name
        authenticode_signed = $false
        verification = "sha256_and_embedded_binary_identity"
        contains_authenticode_verified_binaries = $SigningEnabled
        bytes = $zipItem.Length
        sha256 = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        binaries = $readback
    }
}
