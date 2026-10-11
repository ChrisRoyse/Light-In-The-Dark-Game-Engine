#requires -Version 5.1
<#
.SYNOPSIS
    Validates already-captured two-machine #85 evidence and writes an IP-free report.

.DESCRIPTION
    This script launches and controls no process. It consumes the fixed artifact
    contract documented in scripts/netplay-fsv.md, checks every machine-readable
    predicate, hashes the artifacts, persists a sanitized JSON report, reads it back,
    and fails loud on missing, contradictory, malformed, or insufficient evidence.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EvidenceRoot,
    [string]$ReportPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$reportSchema = "litd.netplay.fsv_report.v1"

function Fail-Fsv {
    param([string]$Code, [string]$Message)
    throw "${Code}: ${Message}"
}

function Resolve-EvidenceFile {
    param([string]$Name)
    $path = Join-Path $root $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Fail-Fsv "NETPLAY_FSV_ARTIFACT_MISSING" $Name
    }
    return (Resolve-Path -LiteralPath $path).Path
}

function Read-BoundedText {
    param([string]$Name)
    $path = Resolve-EvidenceFile $Name
    $length = (Get-Item -LiteralPath $path).Length
    if ($length -eq 0 -or $length -gt 33554432) {
        Fail-Fsv "NETPLAY_FSV_ARTIFACT_SIZE" "$Name must be between 1 byte and 32 MiB"
    }
    return Get-Content -Raw -LiteralPath $path
}

function Require-Property {
    param($Object, [string]$Name, [string]$Artifact)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        Fail-Fsv "NETPLAY_FSV_FIELD_MISSING" "$Artifact.$Name"
    }
    return $property.Value
}

function Require-True {
    param($Object, [string]$Name, [string]$Artifact)
    if ((Require-Property $Object $Name $Artifact) -ne $true) {
        Fail-Fsv "NETPLAY_FSV_OBSERVATION_FALSE" "$Artifact.$Name must be true"
    }
}

function Get-SingleMatch {
    param([string]$Text, [string]$Pattern, [string]$Label)
    $matches = [regex]::Matches($Text, $Pattern)
    if ($matches.Count -ne 1) {
        Fail-Fsv "NETPLAY_FSV_LOG_CARDINALITY" "$Label expected exactly one match, got $($matches.Count)"
    }
    return $matches[0]
}

function Get-Timeline {
    param([string]$Text, [string]$Role)
    $matches = [regex]::Matches(
        $Text,
        "LAN_HASH role=$Role tick=(\d+) hash=([0-9a-fA-F]{16})"
    )
    $rows = @()
    foreach ($match in $matches) {
        $rows += [pscustomobject][ordered]@{
            tick = [uint64]$match.Groups[1].Value
            hash = $match.Groups[2].Value.ToLowerInvariant()
        }
    }
    return $rows
}

function Read-JsonArtifact {
    param([string]$Name)
    try {
        return (Read-BoundedText $Name) | ConvertFrom-Json
    } catch {
        Fail-Fsv "NETPLAY_FSV_JSON_INVALID" "${Name}: $($_.Exception.Message)"
    }
}

function Assert-Sha {
    param([string]$Value, [string]$Label)
    if ($Value -notmatch '^[0-9a-fA-F]{64}$') {
        Fail-Fsv "NETPLAY_FSV_SHA_INVALID" $Label
    }
}

function Assert-Commit {
    param([string]$Value, [string]$Label)
    if ($Value -notmatch '^[0-9a-fA-F]{40}$') {
        Fail-Fsv "NETPLAY_FSV_COMMIT_INVALID" $Label
    }
}

function Get-ArtifactHash {
    param([string]$Name)
    $path = Resolve-EvidenceFile $Name
    return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-Png {
    param([string]$Name)
    $path = Resolve-EvidenceFile $Name
    $bytes = [System.IO.File]::ReadAllBytes($path)
    $magic = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -le $magic.Length) {
        Fail-Fsv "NETPLAY_FSV_SCREENSHOT_EMPTY" $Name
    }
    for ($i = 0; $i -lt $magic.Length; $i++) {
        if ($bytes[$i] -ne $magic[$i]) {
            Fail-Fsv "NETPLAY_FSV_SCREENSHOT_NOT_PNG" $Name
        }
    }
}

$root = (Resolve-Path -LiteralPath $EvidenceRoot -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    Fail-Fsv "NETPLAY_FSV_ROOT_INVALID" "EvidenceRoot must be a directory"
}
if ([string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Join-Path $root "netplay-fsv-report.json"
}

$observation = Read-JsonArtifact "operator-observation.json"
if ((Require-Property $observation "schema" "operator-observation.json") -ne "litd.netplay.fsv_observation.v1") {
    Fail-Fsv "NETPLAY_FSV_OBSERVATION_SCHEMA" "unsupported operator observation schema"
}
$runId = [string](Require-Property $observation "run_id" "operator-observation.json")
if ($runId -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_-]{2,63}$') {
    Fail-Fsv "NETPLAY_FSV_RUN_ID_INVALID" "run_id must be opaque and path/address-free"
}
if ([int](Require-Property $observation "physical_machine_count" "operator-observation.json") -lt 2) {
    Fail-Fsv "NETPLAY_FSV_MACHINE_COUNT" "two physical machines are required"
}
$tickRate = [uint64](Require-Property $observation "tick_rate_hz" "operator-observation.json")
$minimumSeconds = [uint64](Require-Property $observation "minimum_sim_seconds" "operator-observation.json")
if ($tickRate -eq 0 -or $minimumSeconds -lt 600) {
    Fail-Fsv "NETPLAY_FSV_DURATION_CONTRACT" "tick_rate_hz must be positive and minimum_sim_seconds >= 600"
}
$hostHwnd = [string](Require-Property $observation "host_terminal_hwnd" "operator-observation.json")
$joinHwnd = [string](Require-Property $observation "join_terminal_hwnd" "operator-observation.json")
if ($hostHwnd -notmatch '^0x[0-9a-fA-F]+$' -or $joinHwnd -notmatch '^0x[0-9a-fA-F]+$') {
    Fail-Fsv "NETPLAY_FSV_HWND_INVALID" "both exact terminal HWNDs must be hexadecimal"
}
$outcome = [string](Require-Property $observation "win_loss_outcome" "operator-observation.json")
if ($outcome -notin @("host_win", "join_win")) {
    Fail-Fsv "NETPLAY_FSV_OUTCOME_INVALID" "win_loss_outcome must be host_win or join_win"
}
foreach ($flag in @(
    "outcome_manually_inspected",
    "physical_link_unplugged",
    "physical_link_restored",
    "match_resumed_after_restore",
    "host_screenshot_manually_inspected",
    "join_screenshot_manually_inspected",
    "final_hash_lines_manually_read"
)) {
    Require-True $observation $flag "operator-observation.json"
}
$unplugStart = [DateTimeOffset]::Parse(
    [string](Require-Property $observation "unplug_started_utc" "operator-observation.json")
)
$unplugEnd = [DateTimeOffset]::Parse(
    [string](Require-Property $observation "unplug_ended_utc" "operator-observation.json")
)
$unplugSeconds = ($unplugEnd - $unplugStart).TotalSeconds
if ($unplugSeconds -lt 4 -or $unplugSeconds -gt 8) {
    Fail-Fsv "NETPLAY_FSV_UNPLUG_DURATION" "physical disconnect must be approximately 5 seconds (accepted 4..8)"
}

$machineA = Read-JsonArtifact "machine-a-manifest.json"
$machineB = Read-JsonArtifact "machine-b-manifest.json"
foreach ($entry in @(
    @{ Value = $machineA; Name = "machine-a-manifest.json" },
    @{ Value = $machineB; Name = "machine-b-manifest.json" }
)) {
    if ((Require-Property $entry.Value "schema" $entry.Name) -ne "litd.netplay.machine_manifest.v1") {
        Fail-Fsv "NETPLAY_FSV_MACHINE_SCHEMA" $entry.Name
    }
    $label = [string](Require-Property $entry.Value "machine_label" $entry.Name)
    if ($label -notmatch '^[a-z][a-z0-9-]{0,31}$') {
        Fail-Fsv "NETPLAY_FSV_MACHINE_LABEL" "$($entry.Name) label is not sanitized"
    }
    Assert-Commit ([string](Require-Property $entry.Value "git_commit" $entry.Name)) "$($entry.Name).git_commit"
    Assert-Sha ([string](Require-Property $entry.Value "litd_game_sha256" $entry.Name)) "$($entry.Name).litd_game_sha256"
    Assert-Sha ([string](Require-Property $entry.Value "package_sha256" $entry.Name)) "$($entry.Name).package_sha256"
}
if ($machineA.machine_label -eq $machineB.machine_label) {
    Fail-Fsv "NETPLAY_FSV_MACHINE_LABEL_DUPLICATE" "machine labels must differ"
}
foreach ($field in @("git_commit", "litd_game_sha256", "package_sha256")) {
    if ([string]$machineA.$field -ne [string]$machineB.$field) {
        Fail-Fsv "NETPLAY_FSV_MACHINE_BUILD_MISMATCH" "$field differs across physical machines"
    }
}

$hostLog = Read-BoundedText "host-match.log"
$joinLog = Read-BoundedText "join-match.log"
$start = Get-SingleMatch $hostLog "LAN_START role=host ticks=(\d+) delay=(\d+) grace_ms=(\d+) interval=(\d+)" "host start"
$matchTicks = [uint64]$start.Groups[1].Value
$graceMs = [uint64]$start.Groups[3].Value
$interval = [uint64]$start.Groups[4].Value
if ($matchTicks -lt ($tickRate * $minimumSeconds)) {
    Fail-Fsv "NETPLAY_FSV_SIM_TOO_SHORT" "match ticks encode less than $minimumSeconds seconds"
}
if ($graceMs -le [uint64]($unplugSeconds * 1000)) {
    Fail-Fsv "NETPLAY_FSV_GRACE_TOO_SHORT" "grace window did not exceed physical unplug duration"
}
$hostDone = Get-SingleMatch $hostLog "LAN_DONE role=host frames=(\d+) final_hash=([0-9a-fA-F]{16})" "host done"
$joinDone = Get-SingleMatch $joinLog "LAN_DONE role=join frames=(\d+) final_hash=([0-9a-fA-F]{16})" "join done"
$hostFinal = $hostDone.Groups[2].Value.ToLowerInvariant()
$joinFinal = $joinDone.Groups[2].Value.ToLowerInvariant()
if ([uint64]$hostDone.Groups[1].Value -ne $matchTicks -or [uint64]$joinDone.Groups[1].Value -ne $matchTicks) {
    Fail-Fsv "NETPLAY_FSV_FRAME_COUNT" "both peers must finish every configured tick"
}
if ($hostFinal -ne $joinFinal) {
    Fail-Fsv "NETPLAY_FSV_FINAL_HASH_MISMATCH" "host and join final hashes differ"
}
$stall = Get-SingleMatch $joinLog "LAN_STALL role=join tick=(\d+) ms=(\d+)" "join stall window"
if ([uint64]$stall.Groups[2].Value -ge $graceMs) {
    Fail-Fsv "NETPLAY_FSV_STALL_EXCEEDS_GRACE" "operator action window must remain inside host grace"
}
$hostTimeline = @(Get-Timeline $hostLog "host")
$joinTimeline = @(Get-Timeline $joinLog "join")
if ($hostTimeline.Count -lt 2 -or $hostTimeline.Count -ne $joinTimeline.Count) {
    Fail-Fsv "NETPLAY_FSV_TIMELINE_COUNT" "host/join periodic hash timelines are missing or unequal"
}
for ($i = 0; $i -lt $hostTimeline.Count; $i++) {
    if ($hostTimeline[$i].tick -ne $joinTimeline[$i].tick -or $hostTimeline[$i].hash -ne $joinTimeline[$i].hash) {
        Fail-Fsv "NETPLAY_FSV_TIMELINE_MISMATCH" "periodic hash mismatch at row $i"
    }
}
$expectedTimelineCount = [uint64][Math]::Floor(($matchTicks - 1) / $interval) + 1
if ([uint64]$hostTimeline.Count -ne $expectedTimelineCount) {
    Fail-Fsv "NETPLAY_FSV_TIMELINE_INCOMPLETE" "expected $expectedTimelineCount hash rows, got $($hostTimeline.Count)"
}
$hashOkCount = [regex]::Matches($hostLog, "LAN_HASH_OK role=host tick=\d+").Count
if ($hashOkCount -ne $hostTimeline.Count) {
    Fail-Fsv "NETPLAY_FSV_HASH_ACK_INCOMPLETE" "host did not accept every periodic client hash"
}

$verificationHashes = @()
foreach ($name in @(
    "host-replay-on-host.log",
    "host-replay-on-join.log",
    "join-replay-on-host.log",
    "join-replay-on-join.log"
)) {
    $text = Read-BoundedText $name
    $match = Get-SingleMatch $text "LAN_REPLAY_VERIFIED path=.* final_hash=([0-9a-fA-F]{16})" $name
    $verificationHashes += $match.Groups[1].Value.ToLowerInvariant()
}
if (@($verificationHashes | Where-Object { $_ -ne $hostFinal }).Count -ne 0) {
    Fail-Fsv "NETPLAY_FSV_REPLAY_HASH_MISMATCH" "a local or cross-machine replay verification differs"
}
$mismatchHost = Read-BoundedText "mismatch-host.log"
$mismatchProbe = Read-BoundedText "mismatch-probe.log"
$mismatchReport = Read-JsonArtifact "mismatch-probe.json"
if ($mismatchHost -notmatch "LITD_LAN_PACKAGE_MISMATCH" -or $mismatchHost -notmatch "lockstep_hash mismatch") {
    Fail-Fsv "NETPLAY_FSV_MISMATCH_HOST_MISSING" "host transcript lacks typed lockstep_hash refusal"
}
if ($mismatchProbe -notmatch "NETPLAY_LOCKSTEP_MISMATCH_PROBE verdict=REFUSED code=LITD_LAN_PACKAGE_MISMATCH field=lockstep_hash") {
    Fail-Fsv "NETPLAY_FSV_MISMATCH_PROBE_MISSING" "probe transcript lacks accepted refusal"
}
if (
    $mismatchReport.schema -ne "litd.netplay.lockstep_mismatch_probe.v1" -or
    $mismatchReport.status -ne "passed" -or
    $mismatchReport.mismatch_field -ne "lockstep_hash" -or
    $mismatchReport.host_address_persisted -ne $false
) {
    Fail-Fsv "NETPLAY_FSV_MISMATCH_REPORT_INVALID" "mismatch probe report did not pass sanitized readback"
}

Assert-Png "host-end.png"
Assert-Png "join-end.png"
$artifactNames = @(
    "host-match.log", "join-match.log", "host.replay", "join.replay",
    "host-replay-on-host.log", "host-replay-on-join.log",
    "join-replay-on-host.log", "join-replay-on-join.log",
    "mismatch-host.log", "mismatch-probe.log", "mismatch-probe.json",
    "host-end.png", "join-end.png",
    "machine-a-manifest.json", "machine-b-manifest.json",
    "operator-observation.json"
)
$artifactHashes = [ordered]@{}
foreach ($name in $artifactNames) {
    $artifactHashes[$name] = Get-ArtifactHash $name
}
if ($artifactHashes["host.replay"] -ne $artifactHashes["join.replay"]) {
    Fail-Fsv "NETPLAY_FSV_REPLAY_BYTES_MISMATCH" "host and join replay files are not byte-identical"
}

$report = [ordered]@{
    schema = $reportSchema
    issue = 85
    status = "passed"
    run_id = $runId
    git_commit = [string]$machineA.git_commit
    machine_labels = @([string]$machineA.machine_label, [string]$machineB.machine_label)
    physical_machine_count = 2
    tick_rate_hz = $tickRate
    match_ticks = $matchTicks
    simulated_duration_seconds = [double]$matchTicks / [double]$tickRate
    final_state_hash = $hostFinal
    periodic_hash_rows = $hostTimeline.Count
    local_and_cross_replay_verifications = 4
    replay_files_byte_identical = $true
    mismatch_refusal_code = "LITD_LAN_PACKAGE_MISMATCH"
    mismatch_field = "lockstep_hash"
    physical_unplug_seconds = [Math]::Round($unplugSeconds, 3)
    match_resumed_after_restore = $true
    win_loss_outcome = $outcome
    host_terminal_hwnd = $hostHwnd
    join_terminal_hwnd = $joinHwnd
    screenshots_manually_inspected = $true
    final_hash_lines_manually_read = $true
    network_addresses_persisted = $false
    artifact_sha256 = $artifactHashes
}
$reportJson = $report | ConvertTo-Json -Depth 12
$ipv4Pattern = '(?<!\d)(?:\d{1,3}\.){3}\d{1,3}(?!\d)'
$ipv6Pattern = '(?i)(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}'
if ($reportJson -match $ipv4Pattern -or $reportJson -match $ipv6Pattern) {
    Fail-Fsv "NETPLAY_FSV_REPORT_ADDRESS_LEAK" "sanitized report contains a network address"
}
$reportParent = Split-Path -Parent ([System.IO.Path]::GetFullPath($ReportPath))
if (-not (Test-Path -LiteralPath $reportParent -PathType Container)) {
    New-Item -ItemType Directory -Force -Path $reportParent | Out-Null
}
$reportJson | Set-Content -LiteralPath $ReportPath -Encoding UTF8
$readback = Get-Content -Raw -LiteralPath $ReportPath | ConvertFrom-Json
if ($readback.schema -ne $reportSchema -or $readback.status -ne "passed" -or $readback.final_state_hash -ne $hostFinal) {
    Fail-Fsv "NETPLAY_FSV_REPORT_READBACK" "persisted report does not match validated state"
}
$reportSha = (Get-FileHash -LiteralPath $ReportPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "NETPLAY_FSV_EVIDENCE verdict=GREEN final_hash=$hostFinal report_sha256=$reportSha"
