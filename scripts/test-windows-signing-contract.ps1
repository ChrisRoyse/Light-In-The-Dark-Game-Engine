Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsSigningContract.ps1")

function Assert-Equal {
    param($Actual, $Expected, [string]$Label)
    if ($Actual -cne $Expected) {
        throw "TEST_WINDOWS_SIGNING_FAILED: $Label expected='$Expected' actual='$Actual'"
    }
}

function Assert-ThrowsCode {
    param([scriptblock]$Action, [string]$Code, [string]$Label)
    $caught = $null
    try {
        & $Action
    } catch {
        $caught = $_
    }
    if ($null -eq $caught -or $caught.Exception.Message -notmatch [regex]::Escape($Code)) {
        throw "TEST_WINDOWS_SIGNING_FAILED: $Label expected_code=$Code actual='$($caught.Exception.Message)'"
    }
}

$timestampUrl = "http://timestamp.digicert.com"
$validThumbprint = "0123456789abcdef0123456789abcdef01234567"
$existingTool = (Get-Process -Id $PID).Path

$unsigned = Get-LitdWindowsSigningConfiguration `
    -SignToolPath "" `
    -CertificateThumbprint "" `
    -TimestampUrl $timestampUrl
Assert-Equal -Actual $unsigned.enabled -Expected $false -Label "unsigned.enabled"
Assert-Equal -Actual $unsigned.mode -Expected "unsigned" -Label "unsigned.mode"

Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_CONFIG_PARTIAL" -Label "tool only" -Action {
    Get-LitdWindowsSigningConfiguration `
        -SignToolPath $existingTool `
        -CertificateThumbprint "" `
        -TimestampUrl $timestampUrl
}
Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_CONFIG_PARTIAL" -Label "certificate only" -Action {
    Get-LitdWindowsSigningConfiguration `
        -SignToolPath "" `
        -CertificateThumbprint $validThumbprint `
        -TimestampUrl $timestampUrl
}
Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_TOOL_MISSING" -Label "missing tool" -Action {
    Get-LitdWindowsSigningConfiguration `
        -SignToolPath (Join-Path ([IO.Path]::GetTempPath()) "missing-signtool.exe") `
        -CertificateThumbprint $validThumbprint `
        -TimestampUrl $timestampUrl
}
Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_THUMBPRINT_INVALID" -Label "bad thumbprint" -Action {
    Get-LitdWindowsSigningConfiguration `
        -SignToolPath $existingTool `
        -CertificateThumbprint "not-a-thumbprint" `
        -TimestampUrl $timestampUrl
}
Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_TIMESTAMP_INVALID" -Label "bad timestamp" -Action {
    Get-LitdWindowsSigningConfiguration `
        -SignToolPath $existingTool `
        -CertificateThumbprint $validThumbprint `
        -TimestampUrl "file:///not-a-timestamp-service"
}

$enabled = Get-LitdWindowsSigningConfiguration `
    -SignToolPath $existingTool `
    -CertificateThumbprint $validThumbprint `
    -TimestampUrl $timestampUrl
Assert-Equal -Actual $enabled.enabled -Expected $true -Label "enabled.enabled"
Assert-Equal -Actual $enabled.certificate_thumbprint `
    -Expected $validThumbprint.ToUpperInvariant() `
    -Label "enabled.thumbprint"
$artifact = "C:\artifact\litd-editor.exe"
$signArgs = @(Get-LitdWindowsSignArguments -Configuration $enabled -Artifact $artifact)
$verifyArgs = @(Get-LitdWindowsVerifyArguments -Artifact $artifact)
Assert-Equal -Actual ($signArgs -join "|") `
    -Expected "sign|/fd|SHA256|/sha1|$($validThumbprint.ToUpperInvariant())|/tr|http://timestamp.digicert.com/|/td|SHA256|$artifact" `
    -Label "sign arguments"
Assert-Equal -Actual ($verifyArgs -join "|") `
    -Expected "verify|/pa|/all|/v|$artifact" `
    -Label "verify arguments"

$scratch = Join-Path ([IO.Path]::GetTempPath()) ("litd-signing-test-" + [guid]::NewGuid().ToString("N"))
$stage = Join-Path $scratch "stage"
$app = Join-Path $stage "app"
$zip = Join-Path $scratch "payload.zip"
try {
    [void][IO.Directory]::CreateDirectory($app)
    [IO.File]::WriteAllBytes((Join-Path $app "litd-editor.exe"), [byte[]](1, 2, 3, 4))
    [IO.File]::WriteAllBytes((Join-Path $app "litd-game.exe"), [byte[]](5, 6, 7))
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip)
    $rows = @(
        Invoke-LitdWindowsAuthenticodeSigning -Configuration $unsigned `
            -Artifact (Join-Path $app "litd-editor.exe") -Label "editor"
        Invoke-LitdWindowsAuthenticodeSigning -Configuration $unsigned `
            -Artifact (Join-Path $app "litd-game.exe") -Label "game"
    )
    $coverage = Test-LitdWindowsPortableZipSigningCoverage `
        -ZipPath $zip `
        -StageRoot $stage `
        -BinarySigningRows $rows `
        -SigningEnabled $false
    Assert-Equal -Actual $coverage.authenticode_signed -Expected $false `
        -Label "zip.authenticode_signed"
    Assert-Equal -Actual $coverage.verification `
        -Expected "sha256_and_embedded_binary_identity" `
        -Label "zip.verification"
    Assert-Equal -Actual @($coverage.binaries).Count -Expected 2 -Label "zip.binary_count"

    $rows[1].sha256 = ("0" * 64)
    Assert-ThrowsCode -Code "LITD_WINDOWS_SIGNING_ZIP_BINARY_MISMATCH" `
        -Label "changed signing row" `
        -Action {
            Test-LitdWindowsPortableZipSigningCoverage `
                -ZipPath $zip `
                -StageRoot $stage `
                -BinarySigningRows $rows `
                -SigningEnabled $false
        }
} finally {
    if ([IO.Directory]::Exists($scratch)) {
        [IO.Directory]::Delete($scratch, $true)
    }
}

[pscustomobject]@{
    schema = "litd.windows.signing-contract-test.v1"
    status = "passed"
    cases = 10
    certificate_used = $false
    signing_tool_invoked = $false
}
