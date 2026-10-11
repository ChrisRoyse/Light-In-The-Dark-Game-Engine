#requires -Version 5.1
<#
.SYNOPSIS
    Fail-loud warm-target invariant for the main-health backstop (#1590).

.DESCRIPTION
    run-main-health-gate.ps1 (#1567) hands the test gate a warm cargo cache via -TargetDir. That
    cache MUST resolve INSIDE the detection worktree's OWN target\ directory: the worktree is the
    checkout the gate tests, and the #1520 owned-target guard (Resolve-LitdOwnedTargetPath, applied
    deep inside test-gameeditor.ps1 -> test-rust-package.ps1) confines -TargetDir to <checkout>\target
    and rejects anything outside it with TEST_GATE_TARGET_OUTSIDE_REPO.

    The original #1567/#1590 defect derived the warm cache from the OUTER repo root
    (<repoRoot>\target\fsv\test-gameeditor\cargo-target) while the gate ran inside a throwaway
    worktree at <repoRoot>\target\main-health\worktree. The cache was then a SIBLING under
    <repoRoot>\target -- outside <worktree>\target -- so every scheduled run aborted at the test gate
    with TEST_GATE_TARGET_OUTSIDE_REPO and main was reported permanently RED (a false-RED that
    verifies nothing). #1592 corrected the derivation to be worktree-relative; this module ASSERTS
    that invariant BEFORE the multi-minute gate launch so any regression of the derivation fails loud
    -- naming BOTH the resolved target and the health-checkout root -- at a glance, instead of
    resurfacing as an opaque guard throw minutes into a scheduled run.

    Kept as a pure, side-effect-free function (like MainHealthClassification.ps1) so it is
    unit-testable without cargo. It dot-sources LitdOwnedTarget.ps1 and reuses the SAME guard the
    real test gate applies, so the pre-flight check is exact, not an approximation.
#>

Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "LitdOwnedTarget.ps1")

function Assert-MainHealthWarmTargetInsideWorktree {
    # Prove the warm target derives from (resolves inside) the health checkout's own target\ root.
    # On mismatch, throw naming BOTH the resolved target and the worktree root. On success, return a
    # readback object describing the resolved paths so callers can log/verify the invariant held.
    param(
        [Parameter(Mandatory = $true)][string]$WorktreeRoot,
        [Parameter(Mandatory = $true)][string]$WarmTarget
    )

    $worktreeTargetRoot = [System.IO.Path]::GetFullPath((Join-Path $WorktreeRoot "target"))
    $resolvedTarget = [System.IO.Path]::GetFullPath($WarmTarget)

    try {
        # SAME guard the real test gate applies (#1520). Reusing it makes this pre-flight exact.
        [void](Resolve-LitdOwnedTargetPath -RepoRoot $WorktreeRoot -TargetDir $WarmTarget)
    } catch {
        throw ("MAIN_HEALTH_WARM_TARGET_OUTSIDE_WORKTREE: the gate warm target does not derive from " +
            "the health checkout root, so the #1520 owned-target guard deep in the test gate would " +
            "reject it and the backstop would report a permanent false-RED (#1567/#1590). " +
            "resolvedTarget=$resolvedTarget worktreeRoot=$WorktreeRoot " +
            "worktreeTargetRoot=$worktreeTargetRoot -- underlying guard: $($_.Exception.Message)")
    }

    return [pscustomobject]@{
        worktreeRoot       = $WorktreeRoot
        worktreeTargetRoot = $worktreeTargetRoot
        resolvedTarget     = $resolvedTarget
        insideWorktree     = $true
    }
}
