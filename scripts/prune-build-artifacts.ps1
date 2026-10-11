#requires -Version 5.1
<#
.SYNOPSIS
    Bound this repo's Cargo build artifacts on a high-churn, multi-agent Windows host.

.DESCRIPTION
    Cargo never garbage-collects superseded artifacts, and this repo's verified-build /
    test gates deliberately give each source revision and each verified issue its OWN
    isolated target tree (target\test-*\source-<hash>, target\fsv\<issue>\..., target-issue*).
    On a busy host those multiply until the disk fills - this is what grew C:\code to >1 TB.

    The existing Calyx/scripts/target-gc.sh only bounds ONE main target dir and only runs
    from cron on the Linux build host. This script is the Windows-native, whole-repo
    equivalent. It is SAFE: it only ever removes regenerable build output, never source.

    What it removes:
      1. Stray isolated verified-build dirs:  target-*  (target-issue97, target-clippy-*, ...)
      2. Per-crate stray targets:             crates\<crate>\target  (a workspace build must
                                              use the ONE workspace target - these are strays)
      3. Superseded keyed build trees:        target\test-*\source-<hash>  - keep newest N
      4. Old FSV run dirs:                    target\fsv\<issue>\<timestamp>  - keep newest N
      5. Aged FSV report files:               target\fsv\test-rust-package\*  older than N days
      6. Nested target-in-target build trees: target\...\<sub>\target
      7. (optional) cargo-sweep --maxsize on the two main workspace targets, if installed.
      8. Loose isolated build trees in target\ root: target\<name> that carry CACHEDIR.TAG
                                              or .fingerprint (regenerable), older than N days.
      9. Aged test temp dirs in %TEMP%: litd-*/calyx-* scratch older than N hours (the test
                                              gate hard-kills hung runs, so RAII cannot bound these).
     12. Merged agent isolation worktrees:   .claude\worktrees\<agent> whose branch is merged into
                                             origin/main AND is unlocked, clean and aged - removed
                                             junction-safely (never a live/dirty/unmerged one; #1707).
     10. Abandoned persistent gate caches:   target\...\cargo-target trees untouched for N days.
                                              These are the test gate's WARM incremental build
                                              trees (kept on purpose post-#1498 so runs are
                                              incremental, not cold). Active gates touch theirs
                                              every run so they never age out; only a dead
                                              lineage (one-off -TargetDir, retired gate) does.
                                              An in-use cache is never evicted regardless of age.

    The main workspace targets themselves (target\, Calyx\target\) and any other genuine
    workspace target (e.g. Calyx\fuzz\target) are kept - only their superseded CONTENTS are
    pruned.

.PARAMETER RepoRoot
    Repo to clean. Defaults to the parent of this script. Point it at a sibling repo
    (e.g. C:\code\poly) to clean that one with the same rules.

.PARAMETER KeepPerKey
    How many newest source-<hash> trees to keep under each target\test-* key. Default 3.

.PARAMETER FsvKeepPerIssue
    How many newest run dirs to keep under each target\fsv\<issue>. Default 2.

.PARAMETER ReportMaxAgeDays
    Delete loose FSV report files older than this many days. Default 14.

.PARAMETER LooseTargetMaxAgeDays
    Prune loose isolated build trees in target\ root (CACHEDIR.TAG/.fingerprint carriers)
    older than this many days. Default 3.

.PARAMETER GateCacheMaxAgeDays
    Evict a whole persistent gate build cache (target\...\cargo-target) that has not been
    touched in this many days. These caches are the test gate's WARM incremental build trees
    (test-calyx / test-gameeditor pass target\fsv\test-*\cargo-target); they are deliberately
    REUSED across runs so cargo can fingerprint-skip the expensive 95% of the graph (wgpu,
    naga, rapier, 40+ Calyx crates at O3) instead of a full cold rebuild every run - see #1498
    and scripts/test-rust-package.ps1. An active gate touches its cache on every run, so it
    never ages out; only a genuinely abandoned lineage (a one-off -TargetDir, a retired gate)
    does. Default 14 - a full cold rebuild is expensive, so retain caches longer than loose
    scratch. An in-use cache (a live process holding its path) is never evicted regardless of age.

.PARAMETER TempMaxAgeHours
    Prune this repo's aged test scratch dirs in %TEMP% (litd-*/calyx-*) older than this many
    hours. Default 24 - long enough to never touch an in-flight test run.

.PARAMETER IncrementalMaxSizeGB
    Hard size cap for each main target's `debug\incremental` / `release\incremental` cache.
    Cargo NEVER bounds the incremental cache itself: every rebuild adds a new session and old
    ones are not aggressively GC'd, so on a high-churn agent host it grows without limit (it
    hit 58 GB / 97k files here - #1530). Above this cap the whole `incremental` dir is removed;
    it is pure regenerable per-crate cache (NOT deps/, which stay), so the only cost is that the
    next build for that crate is non-incremental once. Independent of cargo-sweep (which is
    optional and was not installed, the reason the cache grew unbounded). On Windows an oversized
    incremental cache also slows fingerprinting/linking/AV scanning every build - see
    rust-lang/rust#137560 - so bounding it protects feedback-loop speed, not just disk. Default 10.

    #1554: this cap is ALSO enforced continuously on the hot path (scripts\test-rust-package.ps1
    checks it under the gate mutex before every build), because a daily sweep let the cache reach
    50 GB / 476 dirs intraday. This script is now the out-of-band BACKSTOP for growth from raw
    cargo runs that never touch the gate; run it on a tight cadence via
    install-build-artifact-gc-task.ps1 -RepeatMinutes. The prune here SKIPS any target tree a live
    cargo/rustc is building into (so a frequent run never wipes a cache mid-build and induces the
    #1536 corruption it exists to reduce) and emits the same LOUD INCREMENTAL_CACHE_PRUNED log.

.PARAMETER IncrementalMaxDirCount
    Hard cap on the NUMBER of crate dirs under each `incremental` cache. Cargo never GCs target\
    (rust-lang/cargo#12633) and rustc only GCs WITHIN a crate's own incremental dir when that crate
    is rebuilt -- orphaned crate dirs from other source-path hashes (multi-worktree agent churn, 476
    dirs observed in #1554) are never revisited and never collected, a hazard the byte cap can miss
    when the dirs are individually small. Above this count the whole `incremental` dir is removed,
    same as the byte cap. 0 disables the count check. Default 400.

.PARAMETER IncrementalOnly
    Run ONLY step 11 (the incremental cache cap) and skip every other prune stage. This is the cheap
    tight-cadence backstop the scheduled task uses (install-build-artifact-gc-task.ps1
    -RepeatMinutes): a full prune recurses the whole target tree, which is wasteful to repeat every
    few minutes, whereas the incremental cap is what actually needs sub-daily enforcement.

.PARAMETER WorktreeMinAgeHours
    Only prune a .claude\worktrees\<agent> worktree whose top-level mtime is older than this many
    hours (a live agent may be mid-slice). Default 24. Belt-and-suspenders on top of the lock/dirty
    guards.

.PARAMETER NoWorktreePrune
    Skip the merged-agent-worktree prune stage (step 12) entirely.

.PARAMETER SweepMaxSize
    cargo-sweep size cap for the main targets (e.g. "40GB"). Default: env
    CALYX_TARGET_MAXSIZE or "40GB". Ignored if cargo-sweep is absent or -NoSweep is set.

.PARAMETER NoSweep
    Skip the cargo-sweep pass on the main targets.

.PARAMETER DryRun
    Report what would be removed (with sizes) without deleting anything.

.EXAMPLE
    pwsh -File scripts\prune-build-artifacts.ps1 -DryRun

.EXAMPLE
    pwsh -File scripts\prune-build-artifacts.ps1 -RepoRoot C:\code\poly
#>
[CmdletBinding()]
param(
    [string]$RepoRoot,
    [int]$KeepPerKey = 3,
    [int]$FsvKeepPerIssue = 2,
    [int]$ReportMaxAgeDays = 14,
    [int]$LooseTargetMaxAgeDays = 3,
    [int]$GateCacheMaxAgeDays = 14,
    [int]$TempMaxAgeHours = 24,
    [int]$IncrementalMaxSizeGB = 10,
    [int]$IncrementalMaxDirCount = 400,
    [switch]$IncrementalOnly,
    [string]$SweepMaxSize,
    [switch]$NoSweep,
    [int]$WorktreeMinAgeHours = 24,
    [switch]$NoWorktreePrune,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "IncrementalCacheCap.ps1")
. (Join-Path $PSScriptRoot "WorktreeGc.ps1")

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
} else {
    $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
}
if ($KeepPerKey -lt 1) { $KeepPerKey = 1 }
if ($FsvKeepPerIssue -lt 1) { $FsvKeepPerIssue = 1 }
if (-not $SweepMaxSize) {
    $SweepMaxSize = if ($env:CALYX_TARGET_MAXSIZE) { $env:CALYX_TARGET_MAXSIZE } else { "40GB" }
}

$driveLetter = (Split-Path -Qualifier $RepoRoot).TrimEnd(':')
function Get-FreeBytes { (Get-PSDrive $driveLetter).Free }

$script:removedCount = 0
$script:failedCount = 0
$script:dryBytes = 0
$script:removedBytes = 0

function Get-TreeBytes {
    param([string]$Path)
    try {
        return [long]((Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum).Sum)
    } catch { return 0 }
}

function Remove-Tree {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string]$Reason = ""
    )
    if (-not (Test-Path -LiteralPath $Path)) { return }
    if ($DryRun) {
        $bytes = Get-TreeBytes -Path $Path
        $script:dryBytes += $bytes
        "  [dry-run] {0,8:N2} GB  {1}  <- {2}" -f ($bytes / 1GB), $Path, $Reason | Write-Host
        return
    }
    # Measure before deleting so the summary reports what THIS script actually reclaimed,
    # not the drive's free-space delta (which is contaminated by concurrent builds/agents
    # on this multi-agent host and can read wildly high or negative - #1494).
    $bytes = Get-TreeBytes -Path $Path
    try {
        Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
        $script:removedCount++
        $script:removedBytes += $bytes
        "  removed  $Path  <- $Reason" | Write-Host
    } catch {
        # Fallback for long paths / stubborn trees: mirror an empty dir over it via robocopy.
        try {
            $empty = Join-Path $env:TEMP ("litd-empty-" + [guid]::NewGuid().ToString("N"))
            New-Item -ItemType Directory -Path $empty -Force | Out-Null
            & robocopy $empty $Path /MIR /NFL /NDL /NJH /NJS /NC /NS /NP *> $null
            Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
            Remove-Item -LiteralPath $empty -Recurse -Force -ErrorAction SilentlyContinue
            $script:removedCount++
            $script:removedBytes += $bytes
            "  removed  $Path  <- $Reason (robocopy fallback)" | Write-Host
        } catch {
            $script:failedCount++
            Write-Warning "could not remove ${Path}: $($_.Exception.Message)"
        }
    }
}

# Non-recursive helper: children of $Parent matching a wildcard, only if $Parent exists.
function Get-Children {
    param([string]$Parent, [string]$Filter = "*")
    if (-not (Test-Path -LiteralPath $Parent -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $Parent -Directory -Filter $Filter -Force -ErrorAction SilentlyContinue)
}

$freeBefore = Get-FreeBytes
Write-Host "=== prune-build-artifacts ==="
Write-Host "repo            : $RepoRoot"
Write-Host "keep per key    : $KeepPerKey   fsv keep/issue: $FsvKeepPerIssue   report age: ${ReportMaxAgeDays}d"
$modeLabel = if ($DryRun) { "DRY-RUN (no deletions)" } else { "DELETE" }
if ($IncrementalOnly) { $modeLabel += "  [INCREMENTAL-ONLY: step 11 cap only]" }
Write-Host "mode            : $modeLabel"
Write-Host ("free before     : {0:N2} GB on {1}:" -f ($freeBefore / 1GB), $driveLetter)
Write-Host ""

# Workspace roots we understand (root + vendored Calyx). Others are left untouched.
$workspaceRoots = @($RepoRoot, (Join-Path $RepoRoot "Calyx")) |
    Where-Object { Test-Path -LiteralPath (Join-Path $_ "Cargo.toml") }

# #1554: -IncrementalOnly runs ONLY the incremental cap (step 11). The full recursive prune below
# (steps 1-10) is wasteful to repeat on the tight sub-daily cadence the incremental cap needs, so
# the scheduled task's frequent run passes -IncrementalOnly and the daily run does the full sweep.
if (-not $IncrementalOnly) {

# -- 1. Stray isolated verified-build dirs: <workspace>\target-* and <crate>\target-* --
Write-Host "-- stray target-* (isolated verified builds) --"
foreach ($ws in $workspaceRoots) {
    foreach ($d in (Get-Children -Parent $ws -Filter "target-*")) {
        Remove-Tree -Path $d.FullName -Reason "isolated verified-build target"
    }
    foreach ($crate in (Get-Children -Parent (Join-Path $ws "crates"))) {
        foreach ($d in (Get-Children -Parent $crate.FullName -Filter "target-*")) {
            Remove-Tree -Path $d.FullName -Reason "isolated per-crate verified-build target"
        }
    }
}

# -- 2. Per-crate stray targets: <workspace>\crates\<crate>\target --
Write-Host "-- per-crate stray target dirs --"
foreach ($ws in $workspaceRoots) {
    foreach ($crate in (Get-Children -Parent (Join-Path $ws "crates"))) {
        $t = Join-Path $crate.FullName "target"
        if (Test-Path -LiteralPath $t -PathType Container) {
            Remove-Tree -Path $t -Reason "per-crate target (workspace build uses the shared target)"
        }
    }
}

# -- 3/4/5/6. Content retention inside each MAIN workspace target --
Write-Host "-- keyed source-* retention / fsv retention / nested targets --"
foreach ($ws in $workspaceRoots) {
    $mainTarget = Join-Path $ws "target"
    if (-not (Test-Path -LiteralPath $mainTarget -PathType Container)) { continue }

    # 3. target\test-*\source-<hash>  (and any other key holding source-* dirs): keep newest N.
    foreach ($keyDir in (Get-Children -Parent $mainTarget)) {
        $sources = @(Get-Children -Parent $keyDir.FullName -Filter "source-*" |
            Sort-Object LastWriteTimeUtc -Descending)
        if ($sources.Count -gt $KeepPerKey) {
            foreach ($old in ($sources | Select-Object -Skip $KeepPerKey)) {
                Remove-Tree -Path $old.FullName -Reason "superseded source tree (keeping newest $KeepPerKey)"
            }
        }
    }

    # 4/5. target\fsv : per-issue run dirs keep newest N; aged loose report files.
    $fsvRoot = Join-Path $mainTarget "fsv"
    if (Test-Path -LiteralPath $fsvRoot -PathType Container) {
        foreach ($issue in (Get-Children -Parent $fsvRoot)) {
            # Run dirs live in TWO layouts on this repo:
            #   modern (litd_engine::fsv_artifacts::begin_issue_run): <issue>\runs\run-<nanos>-<pid>-<n>
            #   legacy (script-authored FSV):                        <issue>\<yyyyMMdd-...>
            # The old rule only matched the legacy '^\d{8}' dirs, so 184 issues on the modern
            # 'runs\' layout (2,656 run dirs) were NEVER pruned. Handle both, keep newest N.
            $candidates = New-Object System.Collections.ArrayList
            $runsSub = Join-Path $issue.FullName "runs"
            if (Test-Path -LiteralPath $runsSub -PathType Container) {
                foreach ($r in (Get-ChildItem -LiteralPath $runsSub -Directory -Force -ErrorAction SilentlyContinue)) {
                    [void]$candidates.Add($r)
                }
            }
            foreach ($r in (Get-ChildItem -LiteralPath $issue.FullName -Directory -Force -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -match '^\d{8}' })) {
                [void]$candidates.Add($r)
            }
            # Never delete the run the immutable latest-manifest.json still points at, regardless
            # of its age - that is the live source-of-truth an open issue may cite.
            $protectedRunId = $null
            $manifestPath = Join-Path $issue.FullName "latest-manifest.json"
            if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
                try { $protectedRunId = (Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json).run_id } catch {}
            }
            $runs = @($candidates | Sort-Object LastWriteTimeUtc -Descending)
            if ($runs.Count -gt $FsvKeepPerIssue) {
                $kept = 0
                foreach ($run in $runs) {
                    if ($kept -lt $FsvKeepPerIssue -or ($protectedRunId -and $run.Name -eq $protectedRunId)) {
                        $kept++
                        continue
                    }
                    Remove-Tree -Path $run.FullName -Reason "old FSV run (keeping newest $FsvKeepPerIssue per issue + latest-manifest target)"
                }
            }
        }
        # Loose aged report/log files anywhere under fsv (test-rust-package writes these).
        if (-not $DryRun) {
            $cutoff = (Get-Date).AddDays(-$ReportMaxAgeDays)
            $aged = @(Get-ChildItem -LiteralPath $fsvRoot -Recurse -File -Force -ErrorAction SilentlyContinue |
                Where-Object { $_.LastWriteTime -lt $cutoff -and ($_.Extension -in '.json', '.log') })
            foreach ($f in $aged) {
                try { Remove-Item -LiteralPath $f.FullName -Force -ErrorAction Stop } catch {}
            }
            if ($aged.Count -gt 0) { "  removed $($aged.Count) FSV report file(s) older than ${ReportMaxAgeDays}d" | Write-Host }
        }
    }

    # 6. Nested target-in-target build trees: any dir named 'target' below the main target.
    $nested = @(Get-ChildItem -LiteralPath $mainTarget -Directory -Recurse -Filter "target" -Force -ErrorAction SilentlyContinue)
    foreach ($n in $nested) {
        # Skip if an ancestor was already deleted this run (path no longer exists).
        if (Test-Path -LiteralPath $n.FullName) {
            Remove-Tree -Path $n.FullName -Reason "nested target inside a target tree"
        }
    }
}

# -- 8. Loose isolated build trees dumped in the main target\ root --
# Agents have historically dropped one-off isolated build/check trees straight into
# target\ (e.g. target\issue1307-litd-check, target\codex-1045). These match none of the
# managed layouts above and so accumulated forever. Identify them SAFELY: a loose child of
# target\ that is itself a Cargo build tree (carries CACHEDIR.TAG or a .fingerprint dir) is
# regenerable output, never source or evidence. Only prune ones older than N days, and never
# the managed dirs. Evidence files (*.md/*.png/*.log) are deliberately left untouched - see
# AGENTS.md for the target\ artifact convention that stops this at the source (#1501).
Write-Host "-- loose isolated build trees in target\ root --"
$managedTargetChildren = @("debug", "release", "doc", "package", "tmp", "fsv",
    "test-gameeditor", "test-calyx", ".fingerprint", ".rustc_info.json", "CACHEDIR.TAG")
foreach ($ws in $workspaceRoots) {
    $mainTarget = Join-Path $ws "target"
    if (-not (Test-Path -LiteralPath $mainTarget -PathType Container)) { continue }
    $looseCutoff = (Get-Date).AddDays(-$LooseTargetMaxAgeDays)
    foreach ($child in (Get-Children -Parent $mainTarget)) {
        if ($managedTargetChildren -contains $child.Name) { continue }
        $isBuildTree = (Test-Path -LiteralPath (Join-Path $child.FullName "CACHEDIR.TAG")) -or
            (Test-Path -LiteralPath (Join-Path $child.FullName ".fingerprint") -PathType Container)
        if ($isBuildTree -and $child.LastWriteTime -lt $looseCutoff) {
            Remove-Tree -Path $child.FullName -Reason "loose isolated build tree in target\ root (older than ${LooseTargetMaxAgeDays}d)"
        }
    }
}

# -- 10. Abandoned persistent gate caches: target\...\cargo-target untouched for N days --
# Post-#1498 the test gate KEEPS its target tree instead of wiping it after every run, so the
# next run of the same gate is incremental (cargo fingerprint-skips wgpu/naga/rapier/40+ Calyx
# crates) instead of a full cold rebuild. The trade is that these warm caches must be bounded
# out-of-band HERE rather than on the hot path. An active gate rewrites artifacts in its cache
# on every run (LastWriteTime stays fresh) so it is never a candidate; only an abandoned lineage
# - a one-off -TargetDir, a retired gate - ages past the cutoff. Evicting one just makes that
# gate's next run cold once. An in-use cache (a live process holding its path, i.e. a gate
# building into it right now) is skipped regardless of age so we never race a running build.
Write-Host "-- abandoned persistent gate caches (cargo-target) --"
$gateCacheCutoff = (Get-Date).AddDays(-$GateCacheMaxAgeDays)
foreach ($ws in $workspaceRoots) {
    $mainTarget = Join-Path $ws "target"
    if (-not (Test-Path -LiteralPath $mainTarget -PathType Container)) { continue }
    $caches = @(Get-ChildItem -LiteralPath $mainTarget -Directory -Recurse -Filter "cargo-target" -Force -ErrorAction SilentlyContinue)
    foreach ($cache in $caches) {
        if (-not (Test-Path -LiteralPath $cache.FullName)) { continue }  # ancestor already pruned
        # Freshness = newest artifact mtime in the tree, not just the dir stamp, so a cache that
        # is actively built into (but whose top dir stamp lags) is correctly seen as recent.
        $newest = $cache.LastWriteTimeUtc
        $newestChild = Get-ChildItem -LiteralPath $cache.FullName -Recurse -File -Force -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
        if ($newestChild -and $newestChild.LastWriteTimeUtc -gt $newest) { $newest = $newestChild.LastWriteTimeUtc }
        if ($newest -ge $gateCacheCutoff.ToUniversalTime()) { continue }  # still warm - keep
        # In-use guard: never evict a cache a live process is building into.
        $inUse = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
            $_.ProcessId -ne $PID -and (
                ($_.CommandLine -and $_.CommandLine.IndexOf($cache.FullName, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) -or
                ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($cache.FullName + "\", [System.StringComparison]::OrdinalIgnoreCase))
            )
        })
        if ($inUse.Count -gt 0) {
            Write-Host "  in-use (skipped): $($cache.FullName)  <- $($inUse.Count) live process(es)"
            continue
        }
        Remove-Tree -Path $cache.FullName -Reason "abandoned gate cache (no artifact touched in ${GateCacheMaxAgeDays}d)"
    }
}

# -- 9. Aged test temp dirs in %TEMP% --
# This repo's test helpers create per-run scratch dirs under %TEMP% (litd-*, and Calyx
# litd_/calyx_ prefixes) and never clean them up - and because the test gate hard-kills
# hung runs with Stop-Process -Force, no in-process RAII cleanup can be guaranteed. This
# out-of-band sweep is the robust bound: remove test scratch dirs older than N hours (long
# enough to never touch an in-flight run). Prefixes are anchored so only this repo's test
# scratch is ever matched (#1495).
Write-Host "-- aged test temp dirs in %TEMP% --"
$tempRoot = [System.IO.Path]::GetTempPath()
if (Test-Path -LiteralPath $tempRoot -PathType Container) {
    $tempCutoff = (Get-Date).AddHours(-$TempMaxAgeHours)
    $tempPrefixes = @("litd-", "litd_", "calyx-", "calyx_")
    $tempDirs = @(Get-ChildItem -LiteralPath $tempRoot -Directory -Force -ErrorAction SilentlyContinue |
        Where-Object {
            $name = $_.Name
            ($tempPrefixes | Where-Object { $name.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase) }) -and
            $_.LastWriteTime -lt $tempCutoff
        })
    foreach ($d in $tempDirs) {
        Remove-Tree -Path $d.FullName -Reason "aged test temp dir (older than ${TempMaxAgeHours}h)"
    }
    if ($tempDirs.Count -eq 0) { Write-Host "  none older than ${TempMaxAgeHours}h" }
}

# -- 12. Merged agent isolation worktrees under .claude\worktrees (#1707) --
# Stale agent worktrees each carry their own ~276k-file target\ (the #1707 645k-file search-timeout
# driver). Prune ONLY those that are provably safe: unlocked AND clean (no uncommitted/untracked work,
# #1716) AND merged (HEAD an ancestor of origin/main) AND aged (mtime older than -WorktreeMinAgeHours).
# Removal is junction-safe (top-level junctions unlinked first, never robocopy /MIR through them) and a
# local-asset-vault count tripwire brackets the stage (#1569). See scripts\WorktreeGc.ps1.
if (-not $NoWorktreePrune) {
    Write-Host "-- merged agent worktrees under .claude\worktrees --"
    $wtSummary = Invoke-LitdMergedWorktreePrune -RepoRoot $RepoRoot -MinAgeHours $WorktreeMinAgeHours -DryRun:$DryRun
    if (-not $DryRun) { $script:removedCount += $wtSummary.Removed; $script:failedCount += $wtSummary.Failed }
    Write-Host ("  worktree prune: removed {0}  kept {1}  failed {2}" -f $wtSummary.Removed, $wtSummary.Kept, $wtSummary.Failed)
}

} # end if (-not $IncrementalOnly) -- steps 1-10 + worktree prune; step 11 (incremental cap) always runs below

# -- 11. Bound the incremental compilation cache in each main target --
# Cargo NEVER bounds debug\incremental / release\incremental: every rebuild adds a session and
# old ones are not GC'd, so on this high-churn agent host it grew to 58 GB / 97k files (#1530) -
# the single biggest target-size and feedback-loop drag on Windows (fingerprint/link/AV overhead,
# rust-lang/rust#137560). Above the cap, remove the whole incremental dir: it is pure regenerable
# per-crate cache (deps\ is untouched), so the only cost is one non-incremental build for the
# affected crate. Runs unconditionally - NOT gated on cargo-sweep being installed (its absence is
# exactly why the cache grew unbounded).
Write-Host "-- incremental cache cap (${IncrementalMaxSizeGB} GB / ${IncrementalMaxDirCount} dirs) --"
$incCapBytes = [long]$IncrementalMaxSizeGB * 1GB
foreach ($ws in $workspaceRoots) {
    $targetRoot = Join-Path $ws "target"
    if (-not (Test-Path -LiteralPath $targetRoot -PathType Container)) { continue }
    # -SkipIfInUse: this is the out-of-band backstop, which does NOT hold the gate mutex, so it must
    # never wipe an incremental tree a live cargo/rustc is building into (#1554). The shared helper
    # emits the LOUD INCREMENTAL_CACHE_PRUNED log and, above cap, removes ONLY <profile>\incremental
    # (deps\ stays -> one non-incremental build). Reported reclaimed bytes feed this run's summary.
    $capRecords = @(Invoke-LitdIncrementalCacheCap `
            -TargetDir $targetRoot `
            -MaxBytes $incCapBytes `
            -MaxDirCount $IncrementalMaxDirCount `
            -SkipIfInUse `
            -DryRun:$DryRun `
            -Trigger "prune-build-artifacts")
    foreach ($rec in $capRecords) {
        if ($rec.over_cap -and $DryRun) {
            $script:dryBytes += [long]$rec.bytes
        } elseif ($rec.pruned) {
            $script:removedCount++
            $script:removedBytes += [long]$rec.bytes
        }
    }
}

# -- 7. cargo-sweep the main targets (best-effort) --
if (-not $NoSweep -and -not $DryRun -and -not $IncrementalOnly) {
    Write-Host "-- cargo-sweep main targets (cap $SweepMaxSize) --"
    if (Get-Command cargo-sweep -ErrorAction SilentlyContinue) {
        foreach ($ws in $workspaceRoots) {
            if (Test-Path -LiteralPath (Join-Path $ws "target") -PathType Container) {
                try {
                    Push-Location $ws
                    & cargo sweep --maxsize $SweepMaxSize $ws
                } catch {
                    Write-Warning "cargo-sweep failed in ${ws}: $($_.Exception.Message)"
                } finally {
                    Pop-Location
                }
            }
        }
    } else {
        Write-Host "  cargo-sweep not on PATH - skipping (install: cargo install cargo-sweep)"
    }
}

# -- Summary --
Write-Host ""
Write-Host "=== summary ==="
if ($DryRun) {
    "would free : {0:N2} GB" -f ($script:dryBytes / 1GB) | Write-Host
    "run without -DryRun to apply." | Write-Host
} else {
    $freeAfter = Get-FreeBytes
    "removed dirs : $($script:removedCount)   failed: $($script:failedCount)" | Write-Host
    "reclaimed    : {0:N2} GB  (bytes actually removed by this run)" -f ($script:removedBytes / 1GB) | Write-Host
    "free after   : {0:N2} GB on {1}:  (net drive delta {2:+0.00;-0.00} GB includes concurrent activity)" -f `
        ($freeAfter / 1GB), $driveLetter, (($freeAfter - $freeBefore) / 1GB) | Write-Host
}
