# Netplay FSV: two physical machines (#85)

This is the operator runbook for #85. It is not a loopback test. A passing run
requires two physical Windows machines on the same LAN, the same pushed commit,
the same `litd-game.exe`, the same `.litdgame`, a real approximately five-second
network-link unplug, and manual Synapse inspection of both saved end-state
bitmaps.

The scripts in this runbook do not launch, stop, restart, or kill the game. Open
and control the two terminals yourself. Keep #85 open unless every final
predicate passes and the evidence is manually inspected.

## Evidence contract

Create an evidence directory on each machine. Aggregate these files into one
directory after the run:

```text
machine-a-manifest.json
machine-b-manifest.json
operator-observation.json
host-match.log
join-match.log
host.replay
join.replay
host-replay-on-host.log
host-replay-on-join.log
join-replay-on-host.log
join-replay-on-join.log
mismatch-host.log
mismatch-probe.log
mismatch-probe.json
host-end.png
join-end.png
```

Use opaque machine labels such as `machine-a` and `machine-b`. Do not put an IP
address, host name, user name, or UNC path in filenames, JSON, issue comments, or
screenshots. The join command needs the host address locally, but the verifier
does not persist it.

## 1. Preflight both machines

Use the same absolute layout on each machine only for operator convenience; the
paths are not evidence. Replace the three paths below.

```powershell
$Repo = "D:\git\light-in-the-dark-game-engine"
$Game = "D:\netplay\litd-game.exe"
$Package = "D:\netplay\match.litdgame"
$Evidence = "D:\netplay\evidence"
$MachineLabel = "machine-a" # use machine-b on the other physical machine
New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
```

The package must have a deterministic tick rate known to the operator and must
be capable of reaching and visibly reporting a win/loss outcome. The canonical
#1787 authoring fixture uses 4 Hz, so 2,400 ticks encode exactly 600 seconds of
simulation. The existing headless `SlotBot` transport harness does not by itself
prove a visible win/loss outcome. If the chosen package/presentation cannot show
that outcome on both machines, stop: the issue is still code-blocked as well as
hardware-blocked, and `outcome_manually_inspected` must not be set true.

Record build/package identity independently on each physical machine:

```powershell
$Commit = (git -C $Repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $Commit -notmatch '^[0-9a-f]{40}$') {
    throw "NETPLAY_FSV_COMMIT_READBACK_FAILED"
}
$manifest = [ordered]@{
    schema = "litd.netplay.machine_manifest.v1"
    machine_label = $MachineLabel
    git_commit = $Commit
    litd_game_sha256 = (Get-FileHash -LiteralPath $Game -Algorithm SHA256).Hash.ToLowerInvariant()
    package_sha256 = (Get-FileHash -LiteralPath $Package -Algorithm SHA256).Hash.ToLowerInvariant()
}
$manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Evidence "$MachineLabel-manifest.json") -Encoding UTF8
```

Read both JSON files back and compare the commit, binary SHA-256, and package
SHA-256 before opening the host. Any mismatch is a hard stop.

## 2. Exact lockstep-hash mismatch refusal

This is a separate short host attempt before the real match. It uses the same
package fingerprint but a deliberately wrong lockstep hash, so it satisfies the
issue's build/lockstep-hash refusal edge rather than merely tampering the package.

On machine A:

```powershell
$Port = 47820
& $Game --lan-host --package $Package --lan-port $Port --lan-capacity 2 `
    --lan-name host --lan-faction 0 --lan-ticks 2400 --lan-delay 1 `
    --lan-grace-ms 40000 --lan-interval 16 --lan-move-period 2 `
    --lan-replay (Join-Path $Evidence "unused-mismatch.replay") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "mismatch-host.log")
$MismatchHostExit = $LASTEXITCODE
if ($MismatchHostExit -eq 0) { throw "NETPLAY_FSV_MISMATCH_HOST_FALSE_GREEN" }
```

On machine B, enter the LAN address only into a process variable. Do not echo it
or include it in a transcript:

```powershell
$HostLanAddress = Read-Host "Machine A LAN address (not persisted)"
& (Join-Path $Repo "scripts\invoke-netplay-lockstep-mismatch.ps1") `
    -HostAddress $HostLanAddress -Port 47820 -PackagePath $Package `
    -OutputPath (Join-Path $Evidence "mismatch-probe.json") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "mismatch-probe.log")
if ($LASTEXITCODE -ne 0) { throw "NETPLAY_FSV_MISMATCH_PROBE_FAILED" }
```

Manually read both logs. Machine A must show
`LITD_LAN_PACKAGE_MISMATCH` and `lockstep_hash mismatch`; machine B must
show `NETPLAY_LOCKSTEP_MISMATCH_PROBE verdict=REFUSED`.

## 3. Real 10-minute-sim match and physical unplug

Restart neither program automatically. Open a fresh host command manually on
machine A:

```powershell
& $Game --lan-host --package $Package --lan-port 47820 --lan-capacity 2 `
    --lan-name host --lan-faction 0 --lan-ticks 2400 --lan-delay 1 `
    --lan-grace-ms 40000 --lan-interval 16 --lan-move-period 2 `
    --lan-replay (Join-Path $Evidence "host.replay") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "host-match.log")
$HostExit = $LASTEXITCODE
if ($HostExit -ne 0) { throw "NETPLAY_FSV_HOST_FAILED exit=$HostExit" }
```

Then join from machine B. The 30-second bounded send pause creates a safe manual
action window inside the 40-second grace period; it does not replace the physical
network edge:

```powershell
$HostLanAddress = Read-Host "Machine A LAN address (not persisted)"
& $Game --lan-join "$($HostLanAddress):47820" --package $Package `
    --lan-name join --lan-slot 1 --lan-faction 1 --lan-move-period 2 `
    --lan-stall-at 600 --lan-stall-ms 30000 `
    --lan-replay (Join-Path $Evidence "join.replay") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "join-match.log")
$JoinExit = $LASTEXITCODE
if ($JoinExit -ne 0) { throw "NETPLAY_FSV_JOIN_FAILED exit=$JoinExit" }
```

When machine B prints `LAN_STALL role=join tick=600`, physically unplug its
Ethernet cable (or disable the physical Wi-Fi link by hand), record UTC start,
wait approximately five seconds, restore the same link, and record UTC end.
Do not use a firewall rule, process suspension, second local process, or another
software-only substitute. The match must resume and both processes must reach
`LAN_DONE`. Read the two final hashes manually; they must be identical.

The match must also reach a real win/loss outcome. Bind Synapse to the exact live
end-state HWND on each physical machine, save `host-end.png` and `join-end.png`,
open each saved bitmap, and manually inspect the outcome and final hash. Record
the exact hexadecimal HWNDs. Logs, hashes, and screenshots support the manual
readback; they do not replace it.

Keep the IP-bearing command line out of the final screenshot. The image should
show only the end-state/output surface needed for the proof.

## 4. Verify both replays on both machines

Copy `host.replay` to machine B and `join.replay` to machine A without renaming
the originals. Run all four commands through the real binary:

Machine A:

```powershell
& $Game --lan-verify-replay (Join-Path $Evidence "host.replay") --package $Package 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "host-replay-on-host.log")
if ($LASTEXITCODE -ne 0) { throw "NETPLAY_FSV_HOST_REPLAY_LOCAL_FAILED" }
& $Game --lan-verify-replay (Join-Path $Evidence "join.replay") --package $Package 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "join-replay-on-host.log")
if ($LASTEXITCODE -ne 0) { throw "NETPLAY_FSV_JOIN_REPLAY_CROSS_FAILED" }
```

Machine B:

```powershell
& $Game --lan-verify-replay (Join-Path $Evidence "join.replay") --package $Package 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "join-replay-on-join.log")
if ($LASTEXITCODE -ne 0) { throw "NETPLAY_FSV_JOIN_REPLAY_LOCAL_FAILED" }
& $Game --lan-verify-replay (Join-Path $Evidence "host.replay") --package $Package 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "host-replay-on-join.log")
if ($LASTEXITCODE -ne 0) { throw "NETPLAY_FSV_HOST_REPLAY_CROSS_FAILED" }
```

All four `LAN_REPLAY_VERIFIED` hashes must equal both live final hashes.

## 5. Observation record and fail-loud validation

Aggregate the contract files into one local evidence directory. Create
`operator-observation.json` only after manually inspecting reality:

```json
{
  "schema": "litd.netplay.fsv_observation.v1",
  "run_id": "issue85-opaque-run-id",
  "physical_machine_count": 2,
  "tick_rate_hz": 4,
  "minimum_sim_seconds": 600,
  "host_terminal_hwnd": "0x0000000000012345",
  "join_terminal_hwnd": "0x0000000000067890",
  "win_loss_outcome": "host_win",
  "outcome_manually_inspected": true,
  "physical_link_unplugged": true,
  "physical_link_restored": true,
  "match_resumed_after_restore": true,
  "unplug_started_utc": "2026-07-29T20:00:00.000Z",
  "unplug_ended_utc": "2026-07-29T20:00:05.000Z",
  "host_screenshot_manually_inspected": true,
  "join_screenshot_manually_inspected": true,
  "final_hash_lines_manually_read": true
}
```

Do not set a manual field true based only on script output. Run the validator:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify-netplay-fsv-evidence.ps1 `
    -EvidenceRoot $Evidence `
    -ReportPath (Join-Path $Evidence "netplay-fsv-report.json")
```

Success prints one `NETPLAY_FSV_EVIDENCE verdict=GREEN` line. Read the JSON back
from disk, independently hash it, and attach the report SHA-256, both screenshots,
both final-hash lines, the four replay-verification lines, and unplug timestamps
to #85. The report intentionally contains no IP address. Any missing artifact,
short simulation, timeline disagreement, replay mismatch, wrong mismatch type,
bad screenshot, uninspected manual predicate, or address leak fails loudly.

Even a validator GREEN is supporting evidence only. Close #85 only after the
operator has manually inspected both exact-HWND saved bitmaps and source-of-truth
logs on the two physical machines.
