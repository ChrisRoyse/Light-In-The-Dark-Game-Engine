#requires -Version 5.1
<#
.SYNOPSIS
    Creates the physical bad-version replay file required by #83.

.DESCRIPTION
    Reads a real SessionReplay, validates its 10-byte envelope, changes only the
    format-version byte at offset 9 from 1 to 2, writes a separate file, reads it
    back, and persists a path-free mutation report. It launches no process.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ReplayPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][string]$ReportPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$schema = "litd.replay.bad_version_mutation.v1"
$magic = [Text.Encoding]::ASCII.GetBytes("LITDRPLY")
$encodingIndex = 8
$versionIndex = 9
$expectedEncoding = [byte]1
$expectedVersion = [byte]1
$badVersion = [byte]2

function Fail-Mutation {
    param([string]$Code, [string]$Message)
    throw "${Code}: ${Message}"
}

function Get-Hash {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$source = (Resolve-Path -LiteralPath $ReplayPath -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    Fail-Mutation "REPLAY_BAD_VERSION_SOURCE_MISSING" "ReplayPath is not a file"
}
$sourceBytes = [IO.File]::ReadAllBytes($source)
if ($sourceBytes.Length -le $versionIndex -or $sourceBytes.Length -gt 33554432) {
    Fail-Mutation "REPLAY_BAD_VERSION_SOURCE_SIZE" "replay must contain the full header and be <= 32 MiB"
}
for ($i = 0; $i -lt $magic.Length; $i++) {
    if ($sourceBytes[$i] -ne $magic[$i]) {
        Fail-Mutation "REPLAY_BAD_VERSION_MAGIC" "source is not a LITDRPLY replay"
    }
}
if ($sourceBytes[$encodingIndex] -ne $expectedEncoding) {
    Fail-Mutation "REPLAY_BAD_VERSION_ENCODING" "source encoding byte is not 1"
}
if ($sourceBytes[$versionIndex] -ne $expectedVersion) {
    Fail-Mutation "REPLAY_BAD_VERSION_ALREADY_MUTATED" "source version byte is not 1"
}

$badBytes = [byte[]]::new($sourceBytes.Length)
[Array]::Copy($sourceBytes, $badBytes, $sourceBytes.Length)
$badBytes[$versionIndex] = $badVersion
$outputFull = [IO.Path]::GetFullPath($OutputPath)
$outputParent = Split-Path -Parent $outputFull
if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
}
[IO.File]::WriteAllBytes($outputFull, $badBytes)
$readback = [IO.File]::ReadAllBytes($outputFull)
if ($readback.Length -ne $sourceBytes.Length -or $readback[$versionIndex] -ne $badVersion) {
    Fail-Mutation "REPLAY_BAD_VERSION_READBACK" "persisted mutation did not retain version byte 2"
}
$differences = 0
for ($i = 0; $i -lt $sourceBytes.Length; $i++) {
    if ($sourceBytes[$i] -ne $readback[$i]) {
        $differences += 1
        if ($i -ne $versionIndex) {
            Fail-Mutation "REPLAY_BAD_VERSION_EXTRA_MUTATION" "byte $i changed unexpectedly"
        }
    }
}
if ($differences -ne 1) {
    Fail-Mutation "REPLAY_BAD_VERSION_DIFFERENCE_COUNT" "expected exactly one changed byte, got $differences"
}

$report = [ordered]@{
    schema = $schema
    status = "passed"
    magic_ascii = "LITDRPLY"
    encoding_byte_index = $encodingIndex
    encoding_byte = [int]$expectedEncoding
    version_byte_index = $versionIndex
    original_version_byte = [int]$expectedVersion
    mutated_version_byte = [int]$badVersion
    changed_byte_count = $differences
    source_size_bytes = $sourceBytes.Length
    source_sha256 = Get-Hash $source
    mutated_sha256 = Get-Hash $outputFull
    source_path_persisted = $false
    output_path_persisted = $false
}
$reportJson = $report | ConvertTo-Json -Depth 6
$reportFull = [IO.Path]::GetFullPath($ReportPath)
$reportParent = Split-Path -Parent $reportFull
if (-not (Test-Path -LiteralPath $reportParent -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $reportParent | Out-Null
}
$reportJson | Set-Content -LiteralPath $reportFull -Encoding UTF8
$reportReadback = Get-Content -Raw -LiteralPath $reportFull | ConvertFrom-Json
if (
    $reportReadback.status -ne "passed" -or
    [int]$reportReadback.version_byte_index -ne $versionIndex -or
    [int]$reportReadback.mutated_version_byte -ne $badVersion
) {
    Fail-Mutation "REPLAY_BAD_VERSION_REPORT_READBACK" "persisted report does not match mutation"
}
$reportSha = Get-Hash $reportFull
Write-Host "REPLAY_BAD_VERSION_COPY verdict=CREATED byte_index=9 original=1 mutated=2 changed_bytes=1 report_sha256=$reportSha"
