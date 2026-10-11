# Replay viewer FSV (#83)

This runbook records and verifies a real versioned `SessionReplay`, runs the
packaged replay viewer at 4x, captures paired deterministic checkpoints, creates
a physical bad-version-byte copy, and specifies the exact-HWND visual proof for
pause, free camera, and per-player perspective.

Do not close #83 from scripts or exit codes. Full State Verification requires
Synapse binding to the exact live viewer HWND, saved bitmap inspection, and
separate source-of-truth log/report readback.

## Fast deterministic preflight

Before the manual run, exercise the replay logic without building the full
renderer-binary test harness:

```powershell
. .\scripts\agent-build-env.ps1
cargo test -p litd-game --lib replay_viewer --locked `
    --target-dir target\fsv\test-replay-viewer\cargo-target
```

This is supporting evidence only. The warm path runs the five deterministic
replay tests in under five seconds on the canonical cache; retain that cache
between runs. A cold test-profile build is intentionally not part of the
routine edit gate. Do not substitute this preflight for the exact-HWND manual
verification below.

## Evidence files

Collect these files in one directory:

```text
session-manifest.json
record-host.log
record-join.log
recorded.replay
record-replay-verify.log
viewer-final.log
checkpoint-1-baseline.log
checkpoint-1-perspective.log
checkpoint-2-baseline.log
checkpoint-2-perspective.log
checkpoint-3-baseline.log
checkpoint-3-perspective.log
bad-version.replay
bad-version-mutation.json
bad-version-refusal.log
operator-observation.json
pause.png
free-camera.png
perspective.png
```

Use an opaque run ID. Do not put IP addresses, user names, or absolute paths in
JSON or issue comments. Paths remain local operator inputs and are represented
in the final report only by artifact SHA-256.

## 1. Preflight

Set local paths and record the source identity:

```powershell
$Repo = "D:\git\light-in-the-dark-game-engine"
$Game = "D:\replay-fsv\litd-game.exe"
$Package = "D:\replay-fsv\match.litdgame"
$Evidence = "D:\replay-fsv\evidence"
$RunId = "issue83-opaque-run-id"
$TickRateHz = 4
New-Item -ItemType Directory -Force -Path $Evidence | Out-Null

$Commit = (git -C $Repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $Commit -notmatch '^[0-9a-f]{40}$') {
    throw "REPLAY_FSV_COMMIT_READBACK_FAILED"
}
$session = [ordered]@{
    schema = "litd.replay.viewer_session.v1"
    run_id = $RunId
    git_commit = $Commit
    litd_game_sha256 = (Get-FileHash -LiteralPath $Game -Algorithm SHA256).Hash.ToLowerInvariant()
    package_sha256 = (Get-FileHash -LiteralPath $Package -Algorithm SHA256).Hash.ToLowerInvariant()
    tick_rate_hz = $TickRateHz
}
$session | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Evidence "session-manifest.json") -Encoding UTF8
```

The package must contain at least two player perspectives with visibly different
fog and an actual recorded human/AI command stream. At 4 Hz, 1,200 ticks encode
five minutes of simulation. If the package tick rate differs, change
`$TickRateHz` and `$RecordTicks = 300 * $TickRateHz`.

## 2. Record a real SessionReplay

The current production writer is the LAN match completion path. Open two
terminals on the same machine. This is recording provenance for #83, not the
two-physical-machine #85 proof.

Host terminal:

```powershell
$RecordTicks = 300 * $TickRateHz
& $Game --lan-host --package $Package --lan-port 47830 --lan-capacity 2 `
    --lan-name recorder --lan-faction 0 --lan-ticks $RecordTicks `
    --lan-delay 1 --lan-grace-ms 8000 --lan-interval 20 `
    --lan-move-period 2 --lan-replay (Join-Path $Evidence "recorded.replay") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "record-host.log")
$RecordHostExit = $LASTEXITCODE
if ($RecordHostExit -ne 0) { throw "REPLAY_FSV_RECORD_HOST_FAILED exit=$RecordHostExit" }
```

Join terminal:

```powershell
& $Game --lan-join "127.0.0.1:47830" --package $Package `
    --lan-name opponent --lan-slot 1 --lan-faction 1 --lan-move-period 2 `
    --lan-replay (Join-Path $Evidence "discard-join.replay") 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "record-join.log")
$RecordJoinExit = $LASTEXITCODE
if ($RecordJoinExit -ne 0) { throw "REPLAY_FSV_RECORD_JOIN_FAILED exit=$RecordJoinExit" }
```

The host must print one `LAN_DONE role=host` line. Verify the newly written
artifact through the packaged binary:

```powershell
& $Game --lan-verify-replay (Join-Path $Evidence "recorded.replay") `
    --package $Package 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "record-replay-verify.log")
if ($LASTEXITCODE -ne 0) { throw "REPLAY_FSV_RECORD_VERIFY_FAILED" }
```

Read the `LAN_DONE` and `LAN_REPLAY_VERIFIED` hashes; they must match.

## 3. Produce the three-checkpoint hash trace

Use three bounded hidden 4x runs for player 0 and the same three runs for player
1. These runs are deterministic measurement support for the live perspective
switch: they must stop at identical ticks and print identical state hashes.

```powershell
$Frames = @(100, 200, 250)
for ($i = 0; $i -lt $Frames.Count; $i++) {
    $checkpoint = $i + 1
    $frameLimit = $Frames[$i]
    & $Game --package $Package --replay-viewer (Join-Path $Evidence "recorded.replay") `
        --replay-speed 4 --replay-perspective 0 --hidden-window `
        --max-frames $frameLimit 2>&1 |
        Tee-Object -LiteralPath (Join-Path $Evidence "checkpoint-$checkpoint-baseline.log")
    if ($LASTEXITCODE -ne 0) { throw "REPLAY_FSV_BASELINE_CHECKPOINT_FAILED index=$checkpoint" }

    & $Game --package $Package --replay-viewer (Join-Path $Evidence "recorded.replay") `
        --replay-speed 4 --replay-perspective 1 --hidden-window `
        --max-frames $frameLimit 2>&1 |
        Tee-Object -LiteralPath (Join-Path $Evidence "checkpoint-$checkpoint-perspective.log")
    if ($LASTEXITCODE -ne 0) { throw "REPLAY_FSV_PERSPECTIVE_CHECKPOINT_FAILED index=$checkpoint" }
}
```

Each pair must print the same tick and `state_hash`; the three ticks must
increase. The perspective IDs must differ. This supports but does not replace
the live mid-playback perspective switch below.

## 4. Exact-HWND interactive proof at 4x

Launch the packaged viewer initially paused:

```powershell
& $Game --package $Package --replay-viewer (Join-Path $Evidence "recorded.replay") `
    --replay-speed 4 --replay-perspective 0 --replay-paused 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "viewer-final.log")
$ViewerExit = $LASTEXITCODE
if ($ViewerExit -ne 0) { throw "REPLAY_FSV_VIEWER_FAILED exit=$ViewerExit" }
```

While that command is live:

1. Bind Synapse to the exact replay viewer HWND. Record it as hexadecimal.
2. Press `Space` to run. The title must say `4x`, player `0`, and show the live
   replay tick/hash.
3. At exactly tick 1000 press `Space`. Confirm the title says `PAUSED`; capture
   that exact HWND to `pause.png`, open the saved bitmap, and manually inspect it.
4. While paused, move the free camera using the RTS camera controls. Confirm the
   view visibly moved while tick/hash stayed unchanged; capture and inspect
   `free-camera.png`.
5. While still paused, press `.` to switch to player 1. Confirm the fog counts
   and visible overlay changed while tick/hash stayed unchanged; capture and
   inspect `perspective.png`.
6. Press `Space` to resume. Wait for `COMPLETE`, inspect the final title/hash,
   then close the viewer manually. Only then does `viewer-final.log` receive the
   final `litd-game replay ...` line.

Do not substitute a generated screenshot, script result, or another window.
Inspect each saved bitmap manually and separately read the final transcript.

## 5. Physical bad-version-byte refusal

Create a separate replay whose byte at zero-based offset 9 is changed from
format version 1 to 2. The mutator proves exactly one byte changed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
    -File (Join-Path $Repo "scripts\new-replay-bad-version-copy.ps1") `
    -ReplayPath (Join-Path $Evidence "recorded.replay") `
    -OutputPath (Join-Path $Evidence "bad-version.replay") `
    -ReportPath (Join-Path $Evidence "bad-version-mutation.json")
```

Pass the physical bad copy to the packaged viewer:

```powershell
& $Game --package $Package `
    --replay-viewer (Join-Path $Evidence "bad-version.replay") `
    --replay-speed 4 2>&1 |
    Tee-Object -LiteralPath (Join-Path $Evidence "bad-version-refusal.log")
$BadVersionExit = $LASTEXITCODE
if ($BadVersionExit -eq 0) { throw "REPLAY_FSV_BAD_VERSION_FALSE_GREEN" }
```

The error must say `session_replay.header` and
`unsupported replay version byte 2; expected 1`. A panic, crash dialog, access
violation, or viewer window is a failure.

## 6. Manual observation and fail-loud evidence validation

After inspecting reality, create `operator-observation.json`:

```json
{
  "schema": "litd.replay.viewer_observation.v1",
  "viewer_hwnd": "0x0000000000012345",
  "pause_bitmap_manually_inspected": true,
  "free_camera_bitmap_manually_inspected": true,
  "perspective_bitmap_manually_inspected": true,
  "paused_at_tick_1000": true,
  "resumed_after_pause": true,
  "free_camera_visibly_moved": true,
  "perspective_fog_visibly_changed": true,
  "presentation_hash_unchanged": true,
  "presentation_hash_before": "0123456789abcdef",
  "presentation_hash_after": "0123456789abcdef",
  "final_hash_line_manually_read": true
}
```

Never set a manual field true from logs alone. Validate the aggregate:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
    -File scripts\verify-replay-viewer-fsv-evidence.ps1 `
    -EvidenceRoot $Evidence `
    -ReportPath (Join-Path $Evidence "replay-viewer-fsv-report.json")
```

Success prints `REPLAY_VIEWER_FSV_EVIDENCE verdict=GREEN`. Read the JSON back,
hash it independently, and attach its SHA-256, the recorded/final hash lines,
three checkpoint pairs, three screenshots, and version-refusal line to #83.

A validator GREEN is supporting evidence only. #83 remains open until the actual
live HWND, saved bitmaps, source-of-truth logs, and persisted report have all been
manually inspected through Synapse in reality.
