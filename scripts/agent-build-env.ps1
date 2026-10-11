#requires -Version 5.1
<#
.SYNOPSIS
    Fail-loud shared-disk build-admission gate for parallel agent worktrees (#1819).

.DESCRIPTION
    THE PROBLEM (#1819). During parallel multi-agent work ~8 git worktrees under
    .claude\worktrees each build their OWN target\ (3-4 GB apiece) on the same C: volume.
    litd-engine's graph pulls candle-transformers + wasmtime/cranelift (via calyx-registry,
    non-optional), so every test/example BINARY needs multi-GB of LLVM codegen. When several
    worktrees codegen at once the volume hits 100% and a build dies with `no space on device`
    MID-COMPILE -- which also corrupts a sibling's incremental cache. FSV (#800) then cannot
    run: work compiles but cannot be executed.

    WHY NOT A SHARED BUILD CACHE (both obvious "share it" fixes are wrong here -- measured):
      * Shared CARGO_TARGET_DIR: Cargo's per-package metadata hash omits the worktree PATH,
        so two worktrees share ONE fingerprint slot per workspace/path crate and one SILENTLY
        reuses -- and runs -- another worktree's compiled code with no error (rust-lang/cargo
        #16642, lx-industries/openblob#522) -- a false-green in an FSV-gated repo. It also
        serializes every build behind one .cargo-lock (rust-lang/cargo#16804), and this repo's
        own gate already rejects a cross-checkout target with TEST_GATE_TARGET_OUTSIDE_REPO
        (Resolve-LitdOwnedTargetPath, #1520/#1567/#1590).
      * sccache (content-keyed wrapper): MEASURED on this host (#1819 FSV) it produces ZERO
        cache hits across worktrees -- Cargo passes per-worktree absolute --out-dir /
        -L dependency= / --extern paths that sccache folds into its cache key, so two
        different worktree paths never share a key (sccache's real strength is same-path CI
        rebuilds, not divergent worktrees). It does NOT reduce the per-worktree recompile.
    So there is no safe shared BUILD cache for divergent worktrees today (the structural fix,
    an out-of-tree content-addressed build dir, is rust-lang/cargo#16147/#5931 -- not stable).

    THE FIX (issue candidate #2: admission control). Since each worktree unavoidably builds
    its own target\, the durable, correct, fail-loud control is to never let a heavy build
    START when the shared disk cannot hold its codegen: an agent dot-sources this before a
    build; it reads the real free space (source of truth = Get-PSDrive), OPTIONALLY reclaims
    via the sanctioned GC, and then either arms the shell or REFUSES loudly
    (AGENT_BUILD_DISK_ADMISSION_DENIED) so the agent waits / reclaims instead of ENOSPC-ing
    mid-codegen and corrupting a sibling cache. Disk is bounded from the other side by
    scripts\prune-build-artifacts.ps1 (aged temp trees, merged-worktree targets, the
    incremental-cache cap). There is NO silent fallback; the refusal is loud and actionable.

    Usage (DOT-SOURCE so the check gates THIS shell before you build):
        . scripts\agent-build-env.ps1                 # refuse loudly if disk is too low
        . scripts\agent-build-env.ps1 -ReclaimIfLow   # run the GC first, then re-check

    Companion disk-saving idiom for READ-ONLY checks (not a shared build): when a worktree
    has no warm target, a `cargo check` can build into the MAIN checkout's warm target via
    CARGO_TARGET_DIR to avoid spinning a fresh multi-GB tree. That is SAFE only for check of
    non-divergent (origin/main-identical) source; NEVER build/run divergent worktree code
    into a shared target (the cross-contamination above). This script does not set it for you.

.PARAMETER MinFreeGB
    Admission floor. Below this many GB free on the repo drive, refuse to arm (a heavy build
    would ENOSPC mid-codegen). Default 15 -- comfortably above the multi-GB candle/wasmtime
    codegen working set plus headroom for concurrent agents.

.PARAMETER ReclaimIfLow
    If below MinFreeGB, run scripts\prune-build-artifacts.ps1 once and re-measure before
    deciding. Without this switch a low disk is an immediate loud refusal naming the command.

.PARAMETER Quiet
    Suppress the human-readable banner; still emits the structured readback object.

.OUTPUTS
    A [pscustomobject] readback (armed, drive, free_gb, min_free_gb, reclaimed_gb, ...).

.EXAMPLE
    . scripts\agent-build-env.ps1 -ReclaimIfLow
#>
[CmdletBinding()]
param(
    [double]$MinFreeGB = 15,
    [switch]$ReclaimIfLow,
    [switch]$Quiet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-AgentBuildLine {
    param([string]$Line)
    if (-not $Quiet) { Write-Host $Line }
}

# Warn (do not fail) if run as a child process instead of dot-sourced: a child-process run
# gates only the child, so a subsequent cargo in the caller's shell is NOT admission-checked.
if ($MyInvocation.InvocationName -ne ".") {
    Write-Warning ("AGENT_BUILD_ENV_NOT_DOT_SOURCED: run as '. scripts\agent-build-env.ps1' so the " +
        "admission check gates the shell you then build in; a child-process run checks nothing for " +
        "your shell.")
}

# Resolve the repo drive (the volume that fills). Prefer the git worktree root.
$repoRoot = $null
try {
    $top = (& git rev-parse --show-toplevel 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -eq 0 -and $top) { $repoRoot = $top }
} catch { }
if (-not $repoRoot) { $repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path }
$driveLetter = (Split-Path -Qualifier ([System.IO.Path]::GetFullPath($repoRoot))).TrimEnd(":")

function Get-FreeGB { param([string]$Drive) [math]::Round((Get-PSDrive $Drive).Free / 1GB, 2) }

$freeBefore = Get-FreeGB -Drive $driveLetter
$reclaimedGB = 0.0
Write-AgentBuildLine ("AGENT_BUILD_DISK_CHECK drive={0}: free_gb={1} min_free_gb={2} reclaim_if_low={3}" -f `
        $driveLetter, $freeBefore, $MinFreeGB, [bool]$ReclaimIfLow)

$freeGB = $freeBefore
if ($freeGB -lt $MinFreeGB -and $ReclaimIfLow) {
    $prune = Join-Path $PSScriptRoot "prune-build-artifacts.ps1"
    if (Test-Path -LiteralPath $prune) {
        Write-AgentBuildLine "AGENT_BUILD_DISK_LOW reclaiming via scripts\prune-build-artifacts.ps1 ..."
        & pwsh -NoProfile -File $prune | Out-Null
        $freeGB = Get-FreeGB -Drive $driveLetter
        $reclaimedGB = [math]::Round($freeGB - $freeBefore, 2)
        Write-AgentBuildLine ("AGENT_BUILD_DISK_RECHECK drive={0}: free_gb={1} reclaimed_gb={2}" -f `
                $driveLetter, $freeGB, $reclaimedGB)
    } else {
        Write-Warning "AGENT_BUILD_PRUNE_MISSING path=$prune (cannot reclaim)"
    }
}

if ($freeGB -lt $MinFreeGB) {
    throw ("AGENT_BUILD_DISK_ADMISSION_DENIED drive=${driveLetter}: free_gb=$freeGB min_free_gb=$MinFreeGB -- " +
        "refusing to arm a heavy build that would ENOSPC mid-codegen and corrupt a sibling incremental cache " +
        "(#1819). Reclaim first: pwsh -File scripts\prune-build-artifacts.ps1  (or re-run this with " +
        "-ReclaimIfLow), or wait for concurrent agent builds to finish.")
}

Write-AgentBuildLine (("AGENT_BUILD_ENV_ARMED drive={0}: free_gb={1} min_free_gb={2} reclaimed_gb={3} " +
        "target_dir=per-worktree incremental=on (no shared CARGO_TARGET_DIR / no sccache -- both unsafe/ineffective here, see header)") -f `
        $driveLetter, $freeGB, $MinFreeGB, $reclaimedGB)

[pscustomobject]@{
    armed        = $true
    drive        = $driveLetter
    free_gb      = $freeGB
    free_before  = $freeBefore
    min_free_gb  = $MinFreeGB
    reclaimed_gb = $reclaimedGB
    repo_root    = $repoRoot
}
