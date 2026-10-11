#requires -Version 5.1
<#
.SYNOPSIS
    Sends the #85 lockstep-hash mismatch probe to an already-running LAN host.

.DESCRIPTION
    This script does not start or stop any process. It reads the package fingerprint,
    opens one TCP connection, sends the production length-prefixed JoinHello JSON with
    a deliberately invalid lockstep hash, and accepts only the typed host refusal.
    The persisted report contains no host address or package fingerprint.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$HostAddress,
    [Parameter(Mandatory = $true)][ValidateRange(1, 65535)][int]$Port,
    [Parameter(Mandatory = $true)][string]$PackagePath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [ValidateRange(1, 60)][int]$TimeoutSeconds = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$schema = "litd.netplay.lockstep_mismatch_probe.v1"

function Fail-Probe {
    param([string]$Code, [string]$Message)
    throw "${Code}: ${Message}"
}

function Read-Exactly {
    param(
        [Parameter(Mandatory = $true)]$Stream,
        [Parameter(Mandatory = $true)][byte[]]$Buffer
    )
    $offset = 0
    while ($offset -lt $Buffer.Length) {
        $read = $Stream.Read($Buffer, $offset, $Buffer.Length - $offset)
        if ($read -eq 0) {
            Fail-Probe "NETPLAY_MISMATCH_EOF" "host closed the connection before a full verdict arrived"
        }
        $offset += $read
    }
}

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

$package = Resolve-Path -LiteralPath $PackagePath -ErrorAction Stop
if (-not (Test-Path -LiteralPath $package.Path -PathType Leaf)) {
    Fail-Probe "NETPLAY_MISMATCH_PACKAGE_MISSING" "package is not a file"
}
$packageBytes = [System.IO.File]::ReadAllBytes($package.Path)
if ($packageBytes.Length -eq 0 -or $packageBytes.Length -gt 16777216) {
    Fail-Probe "NETPLAY_MISMATCH_PACKAGE_SIZE" "package must be between 1 byte and 16 MiB"
}
try {
    $packageJson = [Text.Encoding]::UTF8.GetString($packageBytes) | ConvertFrom-Json
} catch {
    Fail-Probe "NETPLAY_MISMATCH_PACKAGE_JSON" $_.Exception.Message
}
$runtime = $packageJson.PSObject.Properties["runtime"]
if ($null -eq $runtime -or $null -eq $runtime.Value) {
    Fail-Probe "NETPLAY_MISMATCH_RUNTIME_MISSING" "package has no runtime object"
}
$fingerprintProperty = $runtime.Value.PSObject.Properties["datasets_fingerprint"]
if ($null -eq $fingerprintProperty -or [string]::IsNullOrWhiteSpace([string]$fingerprintProperty.Value)) {
    Fail-Probe "NETPLAY_MISMATCH_FINGERPRINT_MISSING" "runtime datasets_fingerprint is absent"
}

$hello = [ordered]@{
    protocol_version = 1
    datasets_fingerprint = [string]$fingerprintProperty.Value
    lockstep_hash = [uint64]0
    player_name = "fsv-lockstep-mismatch"
    requested_slot = 1
    requested_faction = 0
}
$helloBytes = [Text.Encoding]::UTF8.GetBytes(($hello | ConvertTo-Json -Compress))
if ($helloBytes.Length -gt 4194304) {
    Fail-Probe "NETPLAY_MISMATCH_FRAME_OVERSIZE" "JoinHello exceeds the production frame cap"
}

$client = [Net.Sockets.TcpClient]::new()
try {
    $connect = $client.BeginConnect($HostAddress, $Port, $null, $null)
    if (-not $connect.AsyncWaitHandle.WaitOne([TimeSpan]::FromSeconds($TimeoutSeconds))) {
        Fail-Probe "NETPLAY_MISMATCH_CONNECT_TIMEOUT" "host did not accept the probe before timeout"
    }
    $client.EndConnect($connect)
    $client.ReceiveTimeout = $TimeoutSeconds * 1000
    $client.SendTimeout = $TimeoutSeconds * 1000
    $stream = $client.GetStream()
    $lengthBytes = [BitConverter]::GetBytes([uint32]$helloBytes.Length)
    $stream.Write($lengthBytes, 0, $lengthBytes.Length)
    $stream.Write($helloBytes, 0, $helloBytes.Length)
    $stream.Flush()

    $responseLengthBytes = [byte[]]::new(4)
    Read-Exactly -Stream $stream -Buffer $responseLengthBytes
    $responseLength = [BitConverter]::ToUInt32($responseLengthBytes, 0)
    if ($responseLength -eq 0 -or $responseLength -gt 65536) {
        Fail-Probe "NETPLAY_MISMATCH_RESPONSE_SIZE" "verdict frame length $responseLength is invalid"
    }
    $responseBytes = [byte[]]::new($responseLength)
    Read-Exactly -Stream $stream -Buffer $responseBytes
} finally {
    $client.Dispose()
}

try {
    $verdict = [Text.Encoding]::UTF8.GetString($responseBytes) | ConvertFrom-Json
} catch {
    Fail-Probe "NETPLAY_MISMATCH_RESPONSE_JSON" $_.Exception.Message
}
$refused = $verdict.PSObject.Properties["Refused"]
if ($null -eq $refused) {
    Fail-Probe "NETPLAY_MISMATCH_FALSE_ACCEPT" "host did not return a Refused verdict"
}
$code = [string]$refused.Value.code
$reason = [string]$refused.Value.reason
if ($code -ne "LITD_LAN_PACKAGE_MISMATCH" -or $reason -notmatch "lockstep_hash mismatch") {
    Fail-Probe "NETPLAY_MISMATCH_WRONG_REFUSAL" "expected typed lockstep_hash mismatch refusal"
}

$report = [ordered]@{
    schema = $schema
    status = "passed"
    refusal_code = $code
    mismatch_field = "lockstep_hash"
    host_address_persisted = $false
    package_fingerprint_persisted = $false
    response_sha256 = Get-Sha256Hex -Bytes $responseBytes
}
$outputParent = Split-Path -Parent ([System.IO.Path]::GetFullPath($OutputPath))
if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
}
$reportJson = $report | ConvertTo-Json -Depth 6
if ($reportJson -match '(?<!\d)(?:\d{1,3}\.){3}\d{1,3}(?!\d)') {
    Fail-Probe "NETPLAY_MISMATCH_REPORT_IP_LEAK" "sanitized report contains an IPv4 address"
}
$reportJson | Set-Content -LiteralPath $OutputPath -Encoding UTF8
$readback = Get-Content -Raw -LiteralPath $OutputPath | ConvertFrom-Json
if ($readback.schema -ne $schema -or $readback.status -ne "passed") {
    Fail-Probe "NETPLAY_MISMATCH_REPORT_READBACK" "persisted report did not match the accepted verdict"
}
$reportSha = (Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "NETPLAY_LOCKSTEP_MISMATCH_PROBE verdict=REFUSED code=$code field=lockstep_hash report_sha256=$reportSha"
