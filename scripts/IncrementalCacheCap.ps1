<#
    Continuous enforcement of the incremental-compilation cache cap (#1554).

    THE BUG (#1554)
    ---------------
    The 10 GB incremental cap (#1530) was enforced ONLY once a day at 03:30 by the
    LITD-GameEditor-BuildArtifactGC scheduled task. `target\debug\incremental` grows ~0.7 GB/min
    under concurrent multi-agent builds, so measured live it hit 50 GB across 476 crate dirs (5x
    over cap) with the GC eight hours from its next run. A cap evaluated once per day on a resource
    that grows tens of GB/day is not a cap.

    WHY IT MATTERS (not disk -- C: has >1 TB free)
    ----------------------------------------------
    1. Correctness: a huge many-session incremental tree with concurrent writers is the substrate
       for the #1536 stale-work-product corruption class (undefined symbol: anon.*.llvm). Bounding
       the substrate makes that class rarer.
    2. Speed: cargo NEVER GCs target\ (rust-lang/cargo#12633 unimplemented). rustc only GCs WITHIN
       a crate's own incremental dir, and only when that crate is recompiled -- orphaned crate dirs
       from other source-path hashes (multi-worktree churn) are never revisited, so a dir-COUNT cap
       catches bloat a byte cap can miss. On Windows a large cache also taxes EVERY incremental
       build: rust-lang/rust#137560 measured codegen_copy_artifacts_from_incr_cache at 83% of a
       12 s rebuild.

    THE FIX
    -------
    Enforce the cap CONTINUOUSLY at the point of growth, not on a daily timer. This module is the
    single shared enforcement primitive used by BOTH:
      * scripts\test-rust-package.ps1 -- checks BEFORE every gated cargo build, under the gate mutex
        (exclusive access to that target tree), so the cap holds intraday on the hot path.
      * scripts\prune-build-artifacts.ps1 step 11 -- the out-of-band GC backstop, now runnable on a
        tight cadence (install-build-artifact-gc-task.ps1 -RepeatMinutes) for growth from raw cargo
        invocations that never touch the gate.

    Both call the SAME Invoke-LitdIncrementalCacheCap so the byte cap, the dir-count cap, the LOUD
    structured log, and the in-use safety guard can never diverge.

    LOUDNESS: a prune emits INCREMENTAL_CACHE_PRUNED bytes=<n> ... reason=over-cap. A silent prune
    is exactly how everyone came to believe a cap existed when it did not (#1554). Every evaluation
    also emits a one-line INCREMENTAL_CACHE_CHECK so the fact the cap was evaluated is itself
    counted, not assumed.

    SAFETY: pruning the whole incremental dir under a live cargo/rustc that is building into the
    same tree would half-write work products -- inducing the very #1536 corruption we bound the
    substrate to reduce. The -SkipIfInUse guard refuses to prune a tree a foreign process is
    building into (the gate is safe without it because it holds the tree's mutex; the GC is not,
    so it passes -SkipIfInUse). Above the cap we remove ONLY <profile>\incremental (deps\ and the
    rest of target\ stay), so the only cost is one non-incremental build for that profile -- the
    sanctioned behavior from #1530 / #1498.
#>

Set-StrictMode -Version Latest

function Get-LitdIncrementalDirState {
    <#
        Measure ONE incremental dir: total bytes (real Get-ChildItem sum, never Measure-Object
        -Line) and crate-dir count (its immediate child directories, one per crate-session-lineage).
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$IncDir
    )

    if (-not (Test-Path -LiteralPath $IncDir -PathType Container)) {
        return [pscustomobject]@{ path = $IncDir; exists = $false; bytes = [long]0; dir_count = 0 }
    }
    $dirCount = @(Get-ChildItem -LiteralPath $IncDir -Directory -Force -ErrorAction SilentlyContinue).Count
    $bytes = [long]0
    try {
        $bytes = [long]((Get-ChildItem -LiteralPath $IncDir -Recurse -File -Force -ErrorAction SilentlyContinue |
                Measure-Object -Property Length -Sum).Sum)
    } catch { $bytes = [long]0 }
    return [pscustomobject]@{ path = $IncDir; exists = $true; bytes = $bytes; dir_count = $dirCount }
}

function Test-LitdTargetTreeInUse {
    <#
        True when a process OTHER than this one has $TargetRoot on its command line or is executing
        from within it -- i.e. a live cargo/rustc/build script writing into the tree. Same technique
        as prune-build-artifacts.ps1 step 10's in-use guard. Only called when a prune is imminent,
        so the CIM query cost is off the common under-cap path.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$TargetRoot
    )

    $root = [System.IO.Path]::GetFullPath($TargetRoot)
    # NOTE: return the count, not the object array. A `return , @($users)` unary-comma-wraps the
    # result so `@(...)` at the call site is ALWAYS a 1-element (nested) array -> Count is always 1
    # -> the guard would ALWAYS report "in use" and NEVER prune. Return a plain int the caller tests.
    $users = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
            $_.ProcessId -ne $PID -and (
                ($_.CommandLine -and $_.CommandLine.IndexOf($root, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) -or
                ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($root + "\", [System.StringComparison]::OrdinalIgnoreCase))
            )
        })
    return [int]$users.Count
}

function Remove-LitdIncrementalDir {
    <#
        Remove ONE incremental dir with the long-path/AV robocopy-mirror fallback the pruner uses.
        Returns $true only when the dir is gone afterward.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$IncDir
    )

    try {
        Remove-Item -LiteralPath $IncDir -Recurse -Force -ErrorAction Stop
        return -not (Test-Path -LiteralPath $IncDir)
    } catch {
        try {
            $empty = Join-Path $env:TEMP ("litd-empty-" + [guid]::NewGuid().ToString("N"))
            New-Item -ItemType Directory -Path $empty -Force | Out-Null
            & robocopy $empty $IncDir /MIR /NFL /NDL /NJH /NJS /NC /NS /NP *> $null
            Remove-Item -LiteralPath $IncDir -Recurse -Force -ErrorAction Stop
            Remove-Item -LiteralPath $empty -Recurse -Force -ErrorAction SilentlyContinue
            return -not (Test-Path -LiteralPath $IncDir)
        } catch {
            return $false
        }
    }
}

function Invoke-LitdIncrementalCacheCap {
    <#
        Enforce the byte + dir-count cap on the incremental dir(s) of ONE cargo target tree, for
        each requested profile (debug/release). Above EITHER cap the whole <profile>\incremental
        dir is removed (deps\ untouched -> only cost is one non-incremental build for that profile).

        Emits, per profile that exists:
          INCREMENTAL_CACHE_CHECK  ... over_cap=<bool>          (every evaluation -- proves the cap ran)
          INCREMENTAL_CACHE_PRUNED bytes=<n> ... reason=over-cap (only on a prune -- LOUD and counted)
          INCREMENTAL_CACHE_OVER_CAP ... action=skipped-in-use  (over cap but a live build holds the tree)

        Returns one record per profile that existed (so callers can total reclaimed bytes / assert
        state). Never throws on a stuck dir -- it reports removed=$false so the caller stays green.

        -SkipIfInUse : refuse to prune when a foreign process is building into $TargetDir. The GC
                       passes this (it does not coordinate with raw cargo); the gate does not need
                       it (it holds the tree's mutex) but may pass it for defense in depth.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$TargetDir,

        [Parameter(Mandatory = $true)]
        [long]$MaxBytes,

        [int]$MaxDirCount = 0,

        [string[]]$Profiles = @("debug", "release"),

        [switch]$SkipIfInUse,

        [switch]$DryRun,

        [string]$Trigger = "unspecified"
    )

    $records = New-Object System.Collections.ArrayList

    foreach ($buildProfile in @($Profiles)) {
        $incDir = Join-Path (Join-Path $TargetDir $buildProfile) "incremental"
        $state = Get-LitdIncrementalDirState -IncDir $incDir
        if (-not $state.exists) { continue }

        $overBytes = $state.bytes -gt $MaxBytes
        $overDirs = ($MaxDirCount -gt 0) -and ($state.dir_count -gt $MaxDirCount)
        $overCap = $overBytes -or $overDirs
        $detail = if ($overBytes -and $overDirs) { "bytes-and-dirs" } elseif ($overBytes) { "bytes" } elseif ($overDirs) { "dirs" } else { "none" }

        Write-Host ("INCREMENTAL_CACHE_CHECK profile={0} bytes={1} dirs={2} cap_bytes={3} cap_dirs={4} over_cap={5} detail={6} trigger={7} path={8}" -f `
                $buildProfile, $state.bytes, $state.dir_count, $MaxBytes, $MaxDirCount, $overCap, $detail, $Trigger, $incDir)

        if (-not $overCap) {
            [void]$records.Add([pscustomobject]@{
                    profile = $buildProfile; path = $incDir; existed = $true
                    bytes = $state.bytes; dir_count = $state.dir_count
                    over_cap = $false; detail = "none"; pruned = $false; removed = $false; skipped_in_use = $false
                })
            continue
        }

        if ($SkipIfInUse) {
            $liveProcesses = [int](Test-LitdTargetTreeInUse -TargetRoot $TargetDir)
            if ($liveProcesses -gt 0) {
                Write-Host ("INCREMENTAL_CACHE_OVER_CAP profile={0} bytes={1} dirs={2} cap_bytes={3} cap_dirs={4} reason=over-cap detail={5} action=skipped-in-use live_processes={6} trigger={7} path={8}" -f `
                        $buildProfile, $state.bytes, $state.dir_count, $MaxBytes, $MaxDirCount, $detail, $liveProcesses, $Trigger, $incDir)
                [void]$records.Add([pscustomobject]@{
                        profile = $buildProfile; path = $incDir; existed = $true
                        bytes = $state.bytes; dir_count = $state.dir_count
                        over_cap = $true; detail = $detail; pruned = $false; removed = $false; skipped_in_use = $true
                    })
                continue
            }
        }

        if ($DryRun) {
            Write-Host ("INCREMENTAL_CACHE_PRUNED profile={0} bytes={1} dirs={2} cap_bytes={3} cap_dirs={4} reason=over-cap detail={5} trigger={6} path={7} removed=dry-run mode=dry-run" -f `
                    $buildProfile, $state.bytes, $state.dir_count, $MaxBytes, $MaxDirCount, $detail, $Trigger, $incDir)
            [void]$records.Add([pscustomobject]@{
                    profile = $buildProfile; path = $incDir; existed = $true
                    bytes = $state.bytes; dir_count = $state.dir_count
                    over_cap = $true; detail = $detail; pruned = $false; removed = $false; skipped_in_use = $false
                })
            continue
        }

        $removed = Remove-LitdIncrementalDir -IncDir $incDir
        Write-Host ("INCREMENTAL_CACHE_PRUNED profile={0} bytes={1} dirs={2} cap_bytes={3} cap_dirs={4} reason=over-cap detail={5} trigger={6} path={7} removed={8}" -f `
                $buildProfile, $state.bytes, $state.dir_count, $MaxBytes, $MaxDirCount, $detail, $Trigger, $incDir, $removed)
        if (-not $removed) {
            Write-Warning "INCREMENTAL_CACHE_PRUNE_FAILED could not remove $incDir (held open by a process or AV); next run will retry"
        }
        [void]$records.Add([pscustomobject]@{
                profile = $buildProfile; path = $incDir; existed = $true
                bytes = $state.bytes; dir_count = $state.dir_count
                over_cap = $true; detail = $detail; pruned = $removed; removed = $removed; skipped_in_use = $false
            })
    }

    return @($records)
}
