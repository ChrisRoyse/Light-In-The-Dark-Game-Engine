#requires -Version 5.1
<#
.SYNOPSIS
    Main-health backstop payload (#1567): run the FULL correctness gate against origin/main and
    ALARM LOUD when main is red. This is the CI substitute this no-CI repo cannot get any other way.

.DESCRIPTION
    Local git hooks (pre-commit format ratchet #1528, pre-push correctness gate #1553) are the ONLY
    enforcement of quality here: GitHub Actions is a permanent no (#284/#447/#689/#317) and the remote
    is github.com CLOUD, so no server-side pre-receive hook is available either. Client hooks are
    ALWAYS bypassable with `git commit/push --no-verify` -- that is by design and cannot be blocked.
    A single --no-verify push can therefore land a broken build, a failing test, a clippy -D warnings
    violation, an unlicensed dep, or LFS breakage on the shared remote with NOTHING to catch it.

    There is no technically un-bypassable correctness gate possible in this repo (decided doctrine,
    #1567). So the correct answer is NOT to try to block the bypass (impossible; a false sense of
    security) but to make a bypassed / broken main VISIBLE and DETECTED shortly after it happens.
    This script is that detection backstop. On a schedule (install-main-health-task.ps1) it:

      1. `git fetch origin`, resolves origin/main, and hard-syncs a STABLE, REUSED detached
         worktree (target\main-health\worktree) to it: created once, then on every later run
         reset --hard to origin/main plus a targeted `git clean` that PRESERVES the worktree's own
         untracked target\ (the warm cargo cache -- see below), never a throwaway re-clone.
      2. Runs scripts\pre-push-gate.ps1 (cargo check + test gate + FSV examples + strict clippy +
         cargo-deny) against that origin/main checkout, building into a warm cache that lives
         INSIDE the worktree (target\main-health\worktree\target\fsv\test-gameeditor\cargo-target).
         It MUST be in-worktree: LitdOwnedTarget.ps1 (#1520) rightly rejects any -TargetDir that is
         not a child of the checkout-under-test's own target\, so the OUTER repo's shared cache
         cannot be passed in (that was the #1567 bug). TRADEOFF: this main-health cache is separate
         from the outer repo's dev cache, so it does not share fingerprints with local dev builds;
         but because the detection worktree is STABLE and reset (not deleted) each run, the cache
         stays warm across scheduled runs -- a run is still INCREMENTAL, not a cold rebuild, and
         the cache is NEVER wiped. Only deleting the whole detection worktree cools it.
      3. Writes a Source-of-Truth result file (target\main-health\last-run.json): UTC timestamp,
         main commit sha, PASS/FAIL, and on fail the failing step + first error lines.

    RED vs INFRA is decided by MainHealthClassification.ps1, NOT by the inner exit code alone: the
    test gate exits 1 both for a real correctness failure AND for a setup/guard throw (LitdOwnedTarget
    #1520, Calyx/ORT runtime env #1493). Keying RED on "gate exit != 0" would announce an infra
    misconfiguration as a broken main -- a false-RED alarm. So a run whose output carries a setup
    marker is INFRA (exit 4), and only a gate failure WITHOUT such a marker is RED (exit 3).

    LOUD, on THREE independent always-on channels (no silent fallback anywhere):
      * exit code -- 0 GREEN, 3 RED (gate ran, main broken), 4 INFRA (gate could NOT be run). The
        scheduler records it as LastTaskResult, which scripts\check-scheduled-task-health.ps1 reads
        and fails loud on (3 and 4 are BOTH non-benign there by design; see that script).
      * a red alarm FILE (target\main-health\ALARM-main-red.txt), removed only on a GREEN run.
      * a Windows Event Log Error entry (source LITD-GameEditor-MainHealth, id 1567) -- best effort:
        creating the source needs admin once, and if it has never been created this write is skipped
        with a logged warning. That is NOT hiding failure: the exit code and the red file still fire.

    ROOT-CAUSE ERROR LOGGING, no workarounds: if the gate itself cannot run (git missing, fetch
    fails, worktree cannot be built, gate script absent), that is ALSO a loud failure with a distinct
    exit code (4) and the exact reason logged and alarmed. A gate that cannot run is never silently
    treated as "green".

.PARAMETER TaskName
    The scheduled task name that invokes this payload. Used for the self-report health line (mirrors
    run-build-artifact-gc.ps1 / #1555) and the Event Log message. Default "LITD-GameEditor-MainHealth".

.PARAMETER ResultDir
    Where the Source-of-Truth result + alarm files and the stable detection worktree live.
    Default <repo>\target\main-health.

.PARAMETER LogFile
    Append log. Default <repo>\tmp\main-health.log.

.OUTPUTS
    exit 0  main is GREEN (gate passed on origin/main).
    exit 3  gate RAN and main is RED (a correctness regression reached main, NO setup marker -- LOUD).
    exit 4  gate could NOT be run to an honest verdict (fetch/worktree/checkout failure, OR a
            setup/guard marker in the gate output -- LitdOwnedTarget/Calyx/ORT; ALSO loud).

.EXAMPLE
    pwsh -File scripts\run-main-health-gate.ps1
#>
[CmdletBinding()]
param(
    [string]$TaskName = "LITD-GameEditor-MainHealth",
    [string]$ResultDir,
    [string]$LogFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Pure GREEN/RED/INFRA classifier (#1567) -- the single source of the RED-vs-INFRA decision.
. (Join-Path $PSScriptRoot "MainHealthClassification.ps1")
# Fail-loud warm-target invariant (#1590): the gate cache MUST resolve inside the health checkout.
. (Join-Path $PSScriptRoot "MainHealthWarmTarget.ps1")

# Distinct exit codes -- see .OUTPUTS. Kept non-zero-and-non-benign for RED/INFRA so
# check-scheduled-task-health.ps1 surfaces them (it treats any non-benign LastTaskResult as a fail).
$EXIT_GREEN = 0
$EXIT_RED   = 3
$EXIT_INFRA = 4

$EventSource  = "LITD-GameEditor-MainHealth"
$EventLogName = "Application"
$EventId      = 1567

# #1707/#1590: resolve the PRIMARY worktree root, NOT the calling checkout. When this backstop is
# invoked from a LINKED worktree (an agent isolation worktree under .claude\worktrees, or a
# pre-push-gate running inside one), $PSScriptRoot\.. is that linked worktree -- so the detection
# worktree (target\main-health\worktree) and its warm cache would spawn NESTED inside a caller
# worktree's target tree (the observed .claude\worktrees\agent-*\target\main-health\worktree). Git's
# --git-common-dir points at the SHARED .git of the primary checkout regardless of which linked
# worktree we run from; its parent is the primary worktree root. Spawn the detection worktree there,
# beside the primary checkout -- which is also where the gitignored local-asset-vault lives, so the
# LITD_REPO_ROOT handed to the asset tests below resolves the real vault too. Falls back to the
# script-relative root (unchanged behavior) if git cannot resolve it; the run later hard-fails loud
# if git is truly unusable.
$scriptRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$repoRoot = $scriptRoot
try {
    $commonDir = & git -C $scriptRoot rev-parse --path-format=absolute --git-common-dir 2>$null
    if ($LASTEXITCODE -eq 0 -and $commonDir) {
        $commonDir = ("$commonDir" | Select-Object -First 1).Trim()  # fsv-idioms:allow (single-line git output)
        $mainRoot = Split-Path -Parent ($commonDir.TrimEnd('\', '/'))
        if ($mainRoot -and (Test-Path -LiteralPath (Join-Path $mainRoot ".git"))) {
            $repoRoot = (Resolve-Path -LiteralPath $mainRoot).Path
        }
    }
} catch {}
if (-not $ResultDir) { $ResultDir = Join-Path $repoRoot "target\main-health" }
if (-not $LogFile)   { $LogFile   = Join-Path (Join-Path $repoRoot "tmp") "main-health.log" }

$resultFile   = Join-Path $ResultDir "last-run.json"
$alarmFile    = Join-Path $ResultDir "ALARM-main-red.txt"
$gateOutFile  = Join-Path $ResultDir "last-gate-output.txt"
$worktreeDir  = Join-Path $ResultDir "worktree"
# Warm cache that lives INSIDE the detection worktree. It MUST be a child of the worktree's own
# target\: LitdOwnedTarget.ps1 (#1520) rejects any -TargetDir outside the checkout-under-test's
# target\, so the OUTER repo's shared cache cannot be passed here -- doing so was the #1567 bug
# (TEST_GATE_TARGET_OUTSIDE_REPO -> the gate never ran). RETAINED across scheduled runs because the
# worktree is stable + reset (Sync-Worktree), not deleted; NEVER wiped. See the file header for the
# separate-from-dev-cache tradeoff.
$warmTarget   = Join-Path $worktreeDir "target\fsv\test-gameeditor\cargo-target"
$gateRelPath  = "scripts\pre-push-gate.ps1"

New-Item -ItemType Directory -Force -Path $ResultDir | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $LogFile) | Out-Null

function Write-Log {
    param([string]$Message)
    $line = "{0}  {1}" -f (Get-Date -Format o), $Message
    Add-Content -LiteralPath $LogFile -Value $line
    Write-Host $line
}

function ConvertTo-NormalizedPath {
    param([string]$Path)
    try { return ([System.IO.Path]::GetFullPath($Path)).TrimEnd('\', '/').ToLowerInvariant() }
    catch { return $Path.TrimEnd('\', '/').ToLowerInvariant() }
}

function Resolve-PwshEngine {
    $pwshCmd = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($pwshCmd) { return $pwshCmd.Source }
    $psCmd = Get-Command powershell -ErrorAction SilentlyContinue
    if ($psCmd) { return $psCmd.Source }
    throw "neither pwsh nor powershell found on PATH -- cannot run the correctness gate"
}

function Invoke-Git {
    # Run git, log every output line, and return its exit code. Captures output to a variable
    # BEFORE reading $LASTEXITCODE so no downstream pipeline can clobber the code.
    param([Parameter(Mandatory = $true)][string[]]$GitArgs)
    $out = & git @GitArgs 2>&1
    $code = $LASTEXITCODE
    foreach ($o in @($out)) { Write-Log "  git> $o" }
    return $code
}

function Sync-Worktree {
    # Point the STABLE, REUSED detached worktree at $Sha. Fail-loud (throw) on any git error.
    # Created once; on later runs hard-synced (checkout --force --detach + reset --hard) then given
    # a targeted `git clean` that drops stray untracked SOURCE files but PRESERVES the worktree's
    # own untracked target\ (the warm cargo cache). The worktree is never deleted just to re-sync,
    # so the in-worktree cache stays warm across scheduled runs (#1567).
    param([Parameter(Mandatory = $true)][string]$Sha)

    # Drop any stale worktree registration a previous crashed run may have left behind.
    [void](Invoke-Git -GitArgs @("-C", $repoRoot, "worktree", "prune"))

    $registered = $false
    $list = & git -C $repoRoot worktree list --porcelain 2>&1
    $listCode = $LASTEXITCODE
    if ($listCode -eq 0) {
        $wantNorm = ConvertTo-NormalizedPath $worktreeDir
        foreach ($line in @($list)) {
            if ($line -like "worktree *") {
                $p = $line.Substring("worktree ".Length)
                if ((ConvertTo-NormalizedPath $p) -eq $wantNorm) { $registered = $true }
            }
        }
    }

    if ($registered) {
        Write-Log "worktree already registered at $worktreeDir; hard-syncing to $Sha"
        if ((Invoke-Git -GitArgs @("-C", $worktreeDir, "checkout", "--force", "--detach", $Sha)) -ne 0) {
            throw "worktree checkout to $Sha failed"
        }
        if ((Invoke-Git -GitArgs @("-C", $worktreeDir, "reset", "--hard", $Sha)) -ne 0) {
            throw "worktree reset --hard $Sha failed"
        }
        # Targeted clean: remove stray untracked SOURCE files a previous run left behind so the
        # checkout is a faithful origin/main, but KEEP the warm cache. No -x (so gitignored files,
        # including target\ per '**/target/', are left alone) plus an explicit -e target belt so
        # the cache survives even if -x is ever added. Single -f (not -ff): never recurse into a
        # nested git repo. This is the "does NOT remove the untracked in-worktree target\" step.
        if ((Invoke-Git -GitArgs @("-C", $worktreeDir, "clean", "-fd", "-e", "target")) -ne 0) {
            throw "worktree targeted clean failed"
        }
    } else {
        if (Test-Path -LiteralPath $worktreeDir) {
            Write-Log "removing stale non-worktree directory at $worktreeDir before add"
            Remove-Item -LiteralPath $worktreeDir -Recurse -Force
        }
        Write-Log "adding detached worktree at $worktreeDir -> $Sha"
        if ((Invoke-Git -GitArgs @("-C", $repoRoot, "worktree", "add", "--force", "--detach", $worktreeDir, $Sha)) -ne 0) {
            throw "git worktree add failed"
        }
    }
}

function Get-GateFailure {
    # Parse the captured gate output for the failing step and the most useful error lines.
    param([string]$OutputFile)
    $step = "unknown"
    $lines = @()
    if (Test-Path -LiteralPath $OutputFile) {
        $content = @(Get-Content -LiteralPath $OutputFile)
        foreach ($l in $content) {
            if ($l -match "PRE_PUSH_GATE_FAILED\s+step=(\S+)") { $step = $Matches[1] }
        }
        if ($step -eq "unknown") {
            foreach ($l in $content) {
                if ($l -match "PRE_PUSH_GATE_ABORTED.*message=(.+)$") { $step = ("aborted: " + $Matches[1].Trim()) }
            }
        }
        $lines = @($content | Where-Object {
            $_ -match "PRE_PUSH_GATE_(FAILED|ABORTED)" -or $_ -match "error(\[|:)" -or $_ -match "panicked" -or $_ -match "test result: FAILED"
        } | Select-Object -First 25)
        if ($lines.Count -eq 0) { $lines = @($content | Select-Object -Last 25) }
    }
    return [PSCustomObject]@{ Step = $step; Lines = $lines }
}

function Write-EventLogSafe {
    # Best-effort Event Log write. If the source is not registered and cannot be created (needs admin
    # once), skip with a logged warning. NOT a silent fallback: the exit code and red file still fire.
    param([string]$EntryType, [string]$Message)
    try {
        if (-not [System.Diagnostics.EventLog]::SourceExists($EventSource)) {
            New-EventLog -LogName $EventLogName -Source $EventSource -ErrorAction Stop
        }
        Write-EventLog -LogName $EventLogName -Source $EventSource -EntryType $EntryType -EventId $EventId -Message $Message -ErrorAction Stop
        Write-Log "event-log: wrote $EntryType entry (source=$EventSource id=$EventId)"
    } catch {
        Write-Log ("event-log: WARN could not write ({0}) -- {1}. Primary alarms (exit code + {2}) still fired. Register the source once as admin: install-main-health-task.ps1 -RegisterEventSource" -f $EntryType, $_.Exception.Message, $alarmFile)
    }
}

function Write-Result {
    param(
        [string]$Status, [int]$ExitCode, [string]$MainCommit, [string]$FailingStep,
        [string[]]$ErrorLines, [double]$GateElapsedSeconds, [string]$Reason
    )
    $short = if ($MainCommit) { $MainCommit.Substring(0, [Math]::Min(12, $MainCommit.Length)) } else { $null }
    $obj = [PSCustomObject]@{
        schemaVersion      = 1
        issue              = 1567
        taskName           = $TaskName
        utc                = (Get-Date).ToUniversalTime().ToString("o")
        computerName       = $env:COMPUTERNAME
        status             = $Status
        exitCode           = $ExitCode
        mainCommit         = $MainCommit
        mainCommitShort    = $short
        failingStep        = $FailingStep
        errorLines         = @($ErrorLines)
        reason             = $Reason
        gateElapsedSeconds = $GateElapsedSeconds
        repoRoot           = $repoRoot
        worktree           = $worktreeDir
        warmTarget         = $warmTarget
        logFile            = $LogFile
    }
    $json = $obj | ConvertTo-Json -Depth 5
    Set-Content -LiteralPath $resultFile -Value $json -Encoding UTF8
    Write-Log "result: wrote $resultFile (status=$Status exit=$ExitCode)"
}

function Invoke-Alarm {
    param(
        [string]$Status, [int]$ExitCode, [string]$MainCommit, [string]$FailingStep,
        [string[]]$ErrorLines, [string]$Reason
    )
    $b = New-Object System.Collections.ArrayList
    [void]$b.Add("################################################################")
    [void]$b.Add("## MAIN HEALTH ALARM ($Status) -- origin/main did NOT pass the correctness gate")
    [void]$b.Add("## issue #1567 backstop. A bypassed (--no-verify) or otherwise broken push may")
    [void]$b.Add("## have landed on origin/main. Triage NOW: run scripts\pre-push-gate.ps1 on it.")
    [void]$b.Add("##   main commit : $MainCommit")
    [void]$b.Add("##   status      : $Status (exit $ExitCode)")
    if ($FailingStep) { [void]$b.Add("##   failing step: $FailingStep") }
    if ($Reason)      { [void]$b.Add("##   reason      : $Reason") }
    foreach ($e in @($ErrorLines)) { [void]$b.Add("##   > $e") }
    [void]$b.Add("##   result file : $resultFile")
    [void]$b.Add("##   log file    : $LogFile")
    [void]$b.Add("################################################################")
    $text = ($b.ToArray() -join "`r`n")
    Set-Content -LiteralPath $alarmFile -Value $text -Encoding UTF8
    foreach ($l in $b) { Write-Log $l }
    Write-EventLogSafe -EntryType "Error" -Message $text
}

# ------------------------------------------------------------------------------------------------
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$mainCommit = $null
try {
    Write-Log "===== main-health run start (task=$TaskName repo=$repoRoot) ====="

    # Self-report: did the PREVIOUS run of THIS task pass? (mirrors run-build-artifact-gc.ps1; #1555).
    # While this run is executing, the scheduler's LastTaskResult still reflects the last COMPLETED
    # run, so we surface it into the log a human actually reads.
    $healthCheck = Join-Path $PSScriptRoot "check-scheduled-task-health.ps1"
    if (Test-Path -LiteralPath $healthCheck) {
        Write-Log "-- scheduled-task health (previous run of this task) --"
        # Isolated: a hiccup in the self-report must not be mistaken for THIS run's gate result.
        try { & $healthCheck -TaskName $TaskName *>> $LogFile }
        catch { Write-Log "self-report health check errored (non-fatal): $($_.Exception.Message)" }
    }

    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        throw "git not found on PATH"
    }

    # 1. Fetch origin.
    Write-Log "git fetch origin --prune"
    if ((Invoke-Git -GitArgs @("-C", $repoRoot, "fetch", "origin", "--prune")) -ne 0) {
        throw "git fetch origin failed"
    }

    # 2. Resolve origin/main.
    $rev = & git -C $repoRoot rev-parse origin/main 2>&1
    $revCode = $LASTEXITCODE
    $mainCommit = (@($rev) | Select-Object -First 1 | ForEach-Object { "$_".Trim() })
    if ($revCode -ne 0 -or -not $mainCommit) {
        throw "could not resolve origin/main sha (git exit $revCode): $rev"
    }
    Write-Log "origin/main = $mainCommit"

    # 3. Sync the STABLE, REUSED detached worktree to origin/main (warm in-worktree cache retained).
    Sync-Worktree -Sha $mainCommit

    # 4. Run the gate FROM the worktree (origin/main's own gate logic), into the in-worktree cache.
    $gate = Join-Path $worktreeDir $gateRelPath
    if (-not (Test-Path -LiteralPath $gate -PathType Leaf)) {
        throw "correctness gate not found in origin/main worktree at $gate"
    }

    # Fail-loud BEFORE the multi-minute gate launch: assert the warm cache derives from (resolves
    # inside) the detection worktree's own target\, exactly as the #1520 owned-target guard deep in
    # the test gate will demand. Deriving it from the OUTER repo root instead was the #1567/#1590
    # permanent false-RED (TEST_GATE_TARGET_OUTSIDE_REPO surfaced only minutes into the run). This
    # throws (caught below -> INFRA exit 4, never a false-RED) naming BOTH the resolved target and
    # the worktree root when they mismatch. Any throw here is a setup/guard failure, not a broken main.
    $warmCheck = Assert-MainHealthWarmTargetInsideWorktree -WorktreeRoot $worktreeDir -WarmTarget $warmTarget
    Write-Log ("warm-target invariant OK: resolvedTarget={0} is inside worktreeTargetRoot={1} (outer repoRoot={2})" -f `
        $warmCheck.resolvedTarget, $warmCheck.worktreeTargetRoot, $repoRoot)
    # The asset vault (local-asset-vault) is a large gitignored directory that exists only beside the
    # PRIMARY checkout; a linked worktree gets no copy. Point editor asset tests at the REAL repo
    # root so they resolve the real vault instead of failing setup inside the worktree (#1543/#1569).
    # NEVER copy the vault into the worktree -- a robocopy /MIR through a junction is data-loss #1569.
    $env:LITD_REPO_ROOT = $repoRoot
    $engineExe = Resolve-PwshEngine
    Write-Log "running gate: $engineExe -NoProfile -File $gate -TargetDir $warmTarget (LITD_REPO_ROOT=$repoRoot)"
    & $engineExe -NoProfile -ExecutionPolicy Bypass -File $gate -TargetDir $warmTarget *> $gateOutFile
    $gateExit = $LASTEXITCODE
    $gateLines = @(Get-Content -LiteralPath $gateOutFile -ErrorAction SilentlyContinue)
    foreach ($o in $gateLines) { Write-Log "  gate> $o" }

    $stopwatch.Stop()
    $elapsed = [Math]::Round($stopwatch.Elapsed.TotalSeconds, 1)

    # 5. Classify: GREEN / RED / INFRA. RED vs INFRA is decided by MainHealthClassification.ps1 on
    #    the gate output, NOT by exit code alone -- a setup/guard marker (LitdOwnedTarget/Calyx/ORT)
    #    is INFRA, never a false-RED alarm about main's correctness.
    $classification = Get-MainHealthClassification -GateExit $gateExit -GateLines $gateLines

    if ($classification.Kind -eq "GREEN") {
        Write-Log "MAIN HEALTH GREEN: origin/main $mainCommit passed the correctness gate in $elapsed s"
        Write-Result -Status "GREEN" -ExitCode $EXIT_GREEN -MainCommit $mainCommit `
            -FailingStep $null -ErrorLines @() -GateElapsedSeconds $elapsed -Reason "gate passed"
        if (Test-Path -LiteralPath $alarmFile) {
            Remove-Item -LiteralPath $alarmFile -Force
            Write-Log "cleared stale alarm file $alarmFile (main is green again)"
        }
        Write-EventLogSafe -EntryType "Information" -Message "MAIN HEALTH GREEN: origin/main $mainCommit passed the correctness gate ($elapsed s)."
        exit $EXIT_GREEN
    }

    $parsed = Get-GateFailure -OutputFile $gateOutFile

    if ($classification.Kind -eq "INFRA") {
        # The gate ran but a setup/guard marker means it could NOT reach an honest correctness
        # verdict. Announce INFRA (exit 4), NOT RED -- an infra misconfiguration is not a broken main.
        $reason = "correctness gate could NOT run to an honest verdict on origin/main: setup/guard marker '$($classification.InfraMarker)' in gate output (gate exit $gateExit)"
        Write-Log "MAIN HEALTH INFRA: $reason"
        Write-Result -Status "INFRA_ERROR" -ExitCode $EXIT_INFRA -MainCommit $mainCommit `
            -FailingStep ("infra: " + $classification.InfraMarker) -ErrorLines $parsed.Lines `
            -GateElapsedSeconds $elapsed -Reason $reason
        Invoke-Alarm -Status "INFRA_ERROR" -ExitCode $EXIT_INFRA -MainCommit $mainCommit `
            -FailingStep ("infra: " + $classification.InfraMarker) -ErrorLines $parsed.Lines -Reason $reason
        exit $EXIT_INFRA
    }

    # Gate RAN and failed with NO setup marker -> main is genuinely RED.
    $reason = "correctness gate FAILED on origin/main (gate exit $gateExit)"
    Write-Result -Status "RED" -ExitCode $EXIT_RED -MainCommit $mainCommit `
        -FailingStep $parsed.Step -ErrorLines $parsed.Lines -GateElapsedSeconds $elapsed -Reason $reason
    Invoke-Alarm -Status "RED" -ExitCode $EXIT_RED -MainCommit $mainCommit `
        -FailingStep $parsed.Step -ErrorLines $parsed.Lines -Reason $reason
    exit $EXIT_RED

} catch {
    # The gate could NOT be run. That is ALSO loud -- distinct exit code, logged root cause, alarmed.
    $stopwatch.Stop()
    $elapsed = [Math]::Round($stopwatch.Elapsed.TotalSeconds, 1)
    $reason = $_.Exception.Message
    Write-Log "MAIN HEALTH INFRA ERROR: the correctness gate could NOT be run -- $reason"
    Write-Result -Status "INFRA_ERROR" -ExitCode $EXIT_INFRA -MainCommit $mainCommit `
        -FailingStep "infra" -ErrorLines @($reason) -GateElapsedSeconds $elapsed -Reason $reason
    Invoke-Alarm -Status "INFRA_ERROR" -ExitCode $EXIT_INFRA -MainCommit $mainCommit `
        -FailingStep "infra" -ErrorLines @($reason) -Reason $reason
    exit $EXIT_INFRA
}
