#requires -Version 5.1
<#
.SYNOPSIS
    Validates already-captured #83 replay-viewer FSV evidence.

.DESCRIPTION
    Launches no process. It validates a real SessionReplay recording, three paired
    4x checkpoint traces, the completed viewer hash proof, a physical bad-version
    copy/refusal, and exact-HWND manual visual observations. The persisted report
    contains hashes and opaque labels only, never source paths or network addresses.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceRoot,
    [string]$ReportPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$schema = "litd.replay.viewer_fsv_report.v1"

function Fail-Fsv {
    param([string]$Code, [string]$Message)
    throw "${Code}: ${Message}"
}

function Resolve-Artifact {
    param([string]$Name)
    $path = Join-Path $root $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Fail-Fsv "REPLAY_FSV_ARTIFACT_MISSING" $Name
    }
    return (Resolve-Path -LiteralPath $path).Path
}

function Read-Text {
    param([string]$Name)
    $path = Resolve-Artifact $Name
    $length = (Get-Item -LiteralPath $path).Length
    if ($length -eq 0 -or $length -gt 33554432) {
        Fail-Fsv "REPLAY_FSV_ARTIFACT_SIZE" "$Name must be between 1 byte and 32 MiB"
    }
    return Get-Content -Raw -LiteralPath $path
}

function Read-Json {
    param([string]$Name)
    try {
        return (Read-Text $Name) | ConvertFrom-Json
    } catch {
        Fail-Fsv "REPLAY_FSV_JSON_INVALID" "${Name}: $($_.Exception.Message)"
    }
}

function Require-Property {
    param($Object, [string]$Name, [string]$Artifact)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        Fail-Fsv "REPLAY_FSV_FIELD_MISSING" "$Artifact.$Name"
    }
    return $property.Value
}

function Require-True {
    param($Object, [string]$Name, [string]$Artifact)
    if ((Require-Property $Object $Name $Artifact) -ne $true) {
        Fail-Fsv "REPLAY_FSV_OBSERVATION_FALSE" "$Artifact.$Name must be true"
    }
}

function Get-SingleMatch {
    param([string]$Text, [string]$Pattern, [string]$Label)
    $matches = [regex]::Matches($Text, $Pattern)
    if ($matches.Count -ne 1) {
        Fail-Fsv "REPLAY_FSV_LOG_CARDINALITY" "$Label expected one match, got $($matches.Count)"
    }
    return $matches[0]
}

function Read-ViewerReport {
    param([string]$Name)
    $text = Read-Text $Name
    $pattern = "litd-game replay status=(\w+) speed=([0-9.]+x) perspective=(\d+) tick=(\d+) orders=(\d+)/(\d+) state_hash=([0-9a-fA-F]{16}) expected_hash=([0-9a-fA-F]{16}) final_hash_matches=(true|false)"
    $match = Get-SingleMatch $text $pattern $Name
    return [pscustomobject][ordered]@{
        status = $match.Groups[1].Value
        speed = $match.Groups[2].Value
        perspective = [uint32]$match.Groups[3].Value
        tick = [uint64]$match.Groups[4].Value
        orders_applied = [uint64]$match.Groups[5].Value
        orders_total = [uint64]$match.Groups[6].Value
        state_hash = $match.Groups[7].Value.ToLowerInvariant()
        expected_hash = $match.Groups[8].Value.ToLowerInvariant()
        final_hash_matches = $match.Groups[9].Value -eq "true"
    }
}

function Assert-Png {
    param([string]$Name)
    $bytes = [IO.File]::ReadAllBytes((Resolve-Artifact $Name))
    $magic = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -le $magic.Length) {
        Fail-Fsv "REPLAY_FSV_SCREENSHOT_EMPTY" $Name
    }
    for ($i = 0; $i -lt $magic.Length; $i++) {
        if ($bytes[$i] -ne $magic[$i]) {
            Fail-Fsv "REPLAY_FSV_SCREENSHOT_NOT_PNG" $Name
        }
    }
}

function Get-Hash {
    param([string]$Name)
    return (Get-FileHash -LiteralPath (Resolve-Artifact $Name) -Algorithm SHA256).Hash.ToLowerInvariant()
}

$root = (Resolve-Path -LiteralPath $EvidenceRoot -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    Fail-Fsv "REPLAY_FSV_ROOT_INVALID" "EvidenceRoot must be a directory"
}
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Join-Path $root "replay-viewer-fsv-report.json"
}

$session = Read-Json "session-manifest.json"
if ((Require-Property $session "schema" "session-manifest.json") -ne "litd.replay.viewer_session.v1") {
    Fail-Fsv "REPLAY_FSV_SESSION_SCHEMA" "unsupported session manifest"
}
$runId = [string](Require-Property $session "run_id" "session-manifest.json")
if ($runId -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_-]{2,63}$') {
    Fail-Fsv "REPLAY_FSV_RUN_ID_INVALID" "run_id must be opaque"
}
$commit = [string](Require-Property $session "git_commit" "session-manifest.json")
if ($commit -notmatch '^[0-9a-fA-F]{40}$') {
    Fail-Fsv "REPLAY_FSV_COMMIT_INVALID" "git_commit must be 40 hex characters"
}
foreach ($field in @("litd_game_sha256", "package_sha256")) {
    if ([string](Require-Property $session $field "session-manifest.json") -notmatch '^[0-9a-fA-F]{64}$') {
        Fail-Fsv "REPLAY_FSV_SHA_INVALID" $field
    }
}
$tickRate = [uint64](Require-Property $session "tick_rate_hz" "session-manifest.json")
if ($tickRate -eq 0) {
    Fail-Fsv "REPLAY_FSV_TICK_RATE" "tick_rate_hz must be positive"
}

$recordLog = Read-Text "record-host.log"
$recordDone = Get-SingleMatch $recordLog "LAN_DONE role=host frames=(\d+) final_hash=([0-9a-fA-F]{16})" "recorded match"
$recordTicks = [uint64]$recordDone.Groups[1].Value
$recordedHash = $recordDone.Groups[2].Value.ToLowerInvariant()
if ($recordTicks -lt ($tickRate * 300)) {
    Fail-Fsv "REPLAY_FSV_RECORDING_TOO_SHORT" "recording must encode at least five sim minutes"
}
$verifyLog = Read-Text "record-replay-verify.log"
$verifyMatch = Get-SingleMatch $verifyLog "LAN_REPLAY_VERIFIED path=.* final_hash=([0-9a-fA-F]{16})" "record verification"
if ($verifyMatch.Groups[1].Value.ToLowerInvariant() -ne $recordedHash) {
    Fail-Fsv "REPLAY_FSV_RECORD_VERIFY_MISMATCH" "recorded replay did not reproduce recorded hash"
}

$final = Read-ViewerReport "viewer-final.log"
if (
    $final.status -ne "complete" -or
    $final.speed -ne "4x" -or
    -not $final.final_hash_matches -or
    $final.state_hash -ne $recordedHash -or
    $final.expected_hash -ne $recordedHash -or
    $final.orders_applied -ne $final.orders_total
) {
    Fail-Fsv "REPLAY_FSV_FINAL_INVALID" "4x viewer completion does not match recorded final hash"
}

$checkpointTrace = @()
for ($index = 1; $index -le 3; $index++) {
    $baseline = Read-ViewerReport "checkpoint-$index-baseline.log"
    $perspective = Read-ViewerReport "checkpoint-$index-perspective.log"
    if (
        $baseline.speed -ne "4x" -or
        $perspective.speed -ne "4x" -or
        $baseline.perspective -ne 0 -or
        $perspective.perspective -ne 1 -or
        $baseline.tick -ne $perspective.tick -or
        $baseline.state_hash -ne $perspective.state_hash -or
        $baseline.expected_hash -ne $recordedHash -or
        $perspective.expected_hash -ne $recordedHash
    ) {
        Fail-Fsv "REPLAY_FSV_CHECKPOINT_MISMATCH" "checkpoint $index baseline/perspective traces differ"
    }
    if ($checkpointTrace.Count -gt 0 -and $baseline.tick -le $checkpointTrace[-1].tick) {
        Fail-Fsv "REPLAY_FSV_CHECKPOINT_ORDER" "checkpoint ticks must increase"
    }
    $checkpointTrace += [pscustomobject][ordered]@{
        index = $index
        tick = $baseline.tick
        state_hash = $baseline.state_hash
    }
}

$mutation = Read-Json "bad-version-mutation.json"
if (
    $mutation.schema -ne "litd.replay.bad_version_mutation.v1" -or
    $mutation.status -ne "passed" -or
    [int]$mutation.version_byte_index -ne 9 -or
    [int]$mutation.original_version_byte -ne 1 -or
    [int]$mutation.mutated_version_byte -ne 2 -or
    [int]$mutation.changed_byte_count -ne 1
) {
    Fail-Fsv "REPLAY_FSV_MUTATION_INVALID" "bad-version mutation report is not the exact byte-9 edge"
}
$refusal = Read-Text "bad-version-refusal.log"
if (
    $refusal -notmatch "unsupported replay version byte 2; expected 1" -or
    $refusal -notmatch "session_replay.header"
) {
    Fail-Fsv "REPLAY_FSV_REFUSAL_MISSING" "viewer did not print the clear bad-version refusal"
}
if ((Get-Hash "recorded.replay") -ne [string]$mutation.source_sha256) {
    Fail-Fsv "REPLAY_FSV_MUTATION_SOURCE_HASH" "mutation source hash differs from recorded replay"
}
if ((Get-Hash "bad-version.replay") -ne [string]$mutation.mutated_sha256) {
    Fail-Fsv "REPLAY_FSV_MUTATION_OUTPUT_HASH" "mutated replay hash differs from report"
}

$observation = Read-Json "operator-observation.json"
if ($observation.schema -ne "litd.replay.viewer_observation.v1") {
    Fail-Fsv "REPLAY_FSV_OBSERVATION_SCHEMA" "unsupported visual observation schema"
}
$hwnd = [string](Require-Property $observation "viewer_hwnd" "operator-observation.json")
if ($hwnd -notmatch '^0x[0-9a-fA-F]+$') {
    Fail-Fsv "REPLAY_FSV_HWND_INVALID" "viewer_hwnd must be exact hexadecimal HWND"
}
foreach ($flag in @(
    "pause_bitmap_manually_inspected",
    "free_camera_bitmap_manually_inspected",
    "perspective_bitmap_manually_inspected",
    "paused_at_tick_1000",
    "resumed_after_pause",
    "free_camera_visibly_moved",
    "perspective_fog_visibly_changed",
    "presentation_hash_unchanged",
    "final_hash_line_manually_read"
)) {
    Require-True $observation $flag "operator-observation.json"
}
$visualHashBefore = [string](Require-Property $observation "presentation_hash_before" "operator-observation.json")
$visualHashAfter = [string](Require-Property $observation "presentation_hash_after" "operator-observation.json")
if ($visualHashBefore -notmatch '^[0-9a-fA-F]{16}$' -or $visualHashBefore -ne $visualHashAfter) {
    Fail-Fsv "REPLAY_FSV_PRESENTATION_HASH_CHANGED" "pause/camera/perspective changed the observed sim hash"
}
Assert-Png "pause.png"
Assert-Png "free-camera.png"
Assert-Png "perspective.png"

$artifactNames = @(
    "session-manifest.json", "record-host.log", "record-replay-verify.log",
    "recorded.replay", "viewer-final.log",
    "checkpoint-1-baseline.log", "checkpoint-1-perspective.log",
    "checkpoint-2-baseline.log", "checkpoint-2-perspective.log",
    "checkpoint-3-baseline.log", "checkpoint-3-perspective.log",
    "bad-version.replay", "bad-version-mutation.json", "bad-version-refusal.log",
    "operator-observation.json", "pause.png", "free-camera.png", "perspective.png"
)
$hashes = [ordered]@{}
foreach ($name in $artifactNames) {
    $hashes[$name] = Get-Hash $name
}
$report = [ordered]@{
    schema = $schema
    issue = 83
    status = "passed"
    run_id = $runId
    git_commit = $commit
    record_ticks = $recordTicks
    record_sim_seconds = [double]$recordTicks / [double]$tickRate
    recorded_final_hash = $recordedHash
    viewer_final_hash = $final.state_hash
    viewer_speed = "4x"
    checkpoint_trace = $checkpointTrace
    physical_bad_version_byte_index = 9
    bad_version_refused = $true
    viewer_hwnd = $hwnd
    pause_tick = 1000
    visual_checks_manually_inspected = $true
    presentation_hash_unchanged = $true
    source_paths_persisted = $false
    network_addresses_persisted = $false
    artifact_sha256 = $hashes
}
$reportJson = $report | ConvertTo-Json -Depth 12
if (
    $reportJson -match '(?<!\d)(?:\d{1,3}\.){3}\d{1,3}(?!\d)' -or
    $reportJson -match '(?i)(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}'
) {
    Fail-Fsv "REPLAY_FSV_REPORT_ADDRESS_LEAK" "sanitized report contains a network address"
}
$reportFull = [IO.Path]::GetFullPath($ReportPath)
$reportParent = Split-Path -Parent $reportFull
if (-not (Test-Path -LiteralPath $reportParent -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $reportParent | Out-Null
}
$reportJson | Set-Content -LiteralPath $reportFull -Encoding UTF8
$readback = Get-Content -Raw -LiteralPath $reportFull | ConvertFrom-Json
if ($readback.schema -ne $schema -or $readback.status -ne "passed" -or $readback.viewer_final_hash -ne $recordedHash) {
    Fail-Fsv "REPLAY_FSV_REPORT_READBACK" "persisted report does not match validated evidence"
}
$reportSha = (Get-FileHash -LiteralPath $reportFull -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "REPLAY_VIEWER_FSV_EVIDENCE verdict=GREEN final_hash=$recordedHash report_sha256=$reportSha"
