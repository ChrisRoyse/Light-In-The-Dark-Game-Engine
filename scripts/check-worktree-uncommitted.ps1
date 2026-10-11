#requires -Version 5.1
<#
.SYNOPSIS
    Pre-cleanup tripwire (#1716): list any .claude/worktrees/* isolation worktree that holds
    UNCOMMITTED work, and fail loud so that at-risk work is committed before any bulk cleanup.

.DESCRIPTION
    Claude Code isolation worktrees under .claude/worktrees/ are EPHEMERAL: the harness removes a
    worktree it created when that worktree carries no COMMITTED changes, and the auto-clean
    heuristic does NOT count untracked/uncommitted files as "changed". A worktree full of
    authored-but-uncommitted work therefore looks "unchanged" and gets swept, destroying the work
    (#1716 lost ~3 agent-hours of test-green content this way; same class as #1569).

    This script is the audit that must run BEFORE any bulk worktree pruning (manual or via the
    build-artifact GC): it enumerates every .claude/worktrees/* worktree and reports each one that
    has uncommitted content (tracked-modified OR untracked, per `git status --porcelain`; gitignored
    build output like target\ is correctly ignored by git and never counted). Output is fail-loud:
    each dirty worktree is named with its uncommitted file count and a bounded sample of paths, and
    the script exits non-zero. A clean scan (no dirty worktrees) exits 0.

    It NEVER deletes or mutates anything -- it is read-only. It is the safety interlock that makes
    the destructive prune safe: commit/push the flagged worktrees first, then prune.

.PARAMETER RepoRoot
    Repo to audit. Defaults to the MAIN repo root resolved from git (so it audits the same
    .claude/worktrees regardless of which checkout invokes it), falling back to the script's parent.

.PARAMETER MaxFilesListed
    Max uncommitted file paths to print per dirty worktree (the full count is always reported).
    Default 10.

.EXAMPLE
    pwsh -File scripts\check-worktree-uncommitted.ps1

.OUTPUTS
    exit 0  no .claude/worktrees/* worktree has uncommitted work (safe to prune).
    exit 1  at least one worktree has uncommitted work (LOUD: names each + file count). Commit first.
    exit 2  could not run the audit (git missing / not a repo) -- also loud, never silently "clean".
#>
[CmdletBinding()]
param(
    [string]$RepoRoot,
    [int]$MaxFilesListed = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-MainRepoRoot {
    # Resolve the MAIN worktree root, not the calling checkout: git-common-dir points at the shared
    # .git of the primary checkout, whose parent is the main repo root. This makes the audit target
    # the same .claude/worktrees whether invoked from the primary checkout or a linked worktree.
    param([string]$Start)
    try {
        $common = & git -C $Start rev-parse --path-format=absolute --git-common-dir 2>$null
        if ($LASTEXITCODE -eq 0 -and $common) {
            $common = ("$common" | Select-Object -First 1).Trim()
            $parent = Split-Path -Parent ($common.TrimEnd('\', '/'))
            if ($parent -and (Test-Path -LiteralPath $parent -PathType Container)) {
                return (Resolve-Path -LiteralPath $parent).Path
            }
        }
    } catch {}
    return (Resolve-Path -LiteralPath $Start).Path
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "WORKTREE_UNCOMMITTED_TRIPWIRE_ERROR: git not found on PATH -- cannot audit worktrees"
    exit 2
}

if (-not $RepoRoot) {
    $RepoRoot = Resolve-MainRepoRoot -Start (Join-Path $PSScriptRoot "..")
} else {
    $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
}

$worktreesDir = Join-Path $RepoRoot ".claude\worktrees"

Write-Host "=== check-worktree-uncommitted ==="
Write-Host "repo root : $RepoRoot"
Write-Host "scanning  : $worktreesDir"
Write-Host ""

if (-not (Test-Path -LiteralPath $worktreesDir -PathType Container)) {
    Write-Host "no .claude\worktrees directory -- nothing to audit."
    exit 0
}

# Source of truth for which .claude/worktrees dirs are real, registered git worktrees.
$registered = @{}
$list = & git -C $RepoRoot worktree list --porcelain 2>&1
if ($LASTEXITCODE -eq 0) {
    foreach ($line in @($list)) {
        if ("$line" -like "worktree *") {
            $p = "$line".Substring("worktree ".Length)
            try { $registered[([System.IO.Path]::GetFullPath($p)).TrimEnd('\', '/').ToLowerInvariant()] = $true } catch {}
        }
    }
}

$dirtyWorktrees = New-Object System.Collections.ArrayList
$scanned = 0
$skipped = New-Object System.Collections.ArrayList

foreach ($dir in (Get-ChildItem -LiteralPath $worktreesDir -Directory -Force -ErrorAction SilentlyContinue)) {
    $full = $dir.FullName
    $norm = ([System.IO.Path]::GetFullPath($full)).TrimEnd('\', '/').ToLowerInvariant()
    if (-not $registered.ContainsKey($norm)) {
        # Not a registered worktree (stale dir, or list failed). Report as a skip, not clean.
        [void]$skipped.Add($full)
        continue
    }
    $scanned++
    # --porcelain lists tracked-modified AND untracked source files; gitignored build output
    # (target\, per **/target/) is NOT listed, so a warm cache never trips the tripwire.
    $status = & git -C $full status --porcelain --untracked-files=all 2>&1
    $statusCode = $LASTEXITCODE
    if ($statusCode -ne 0) {
        [void]$skipped.Add($full + "  (git status exit $statusCode)")
        continue
    }
    $entries = @(@($status) | Where-Object { "$_".Trim().Length -gt 0 })
    if ($entries.Count -gt 0) {
        [void]$dirtyWorktrees.Add([pscustomobject]@{ Path = $full; Count = $entries.Count; Entries = $entries })
    }
}

foreach ($wt in $dirtyWorktrees) {
    Write-Host ("DIRTY  {0}  ({1} uncommitted file(s))" -f $wt.Path, $wt.Count)
    $shown = 0
    foreach ($e in $wt.Entries) {
        if ($shown -ge $MaxFilesListed) {
            Write-Host ("       ... and {0} more" -f ($wt.Count - $shown))
            break
        }
        Write-Host ("       {0}" -f "$e")
        $shown++
    }
}

if ($skipped.Count -gt 0) {
    Write-Host ""
    Write-Host "-- not audited (unregistered / status error) --"
    foreach ($s in $skipped) { Write-Host ("  SKIP  {0}" -f $s) }
}

Write-Host ""
Write-Host "=== summary ==="
Write-Host ("worktrees audited : {0}" -f $scanned)
Write-Host ("dirty (at risk)   : {0}" -f $dirtyWorktrees.Count)
Write-Host ("not audited       : {0}" -f $skipped.Count)

if ($dirtyWorktrees.Count -gt 0) {
    Write-Host ""
    Write-Host ("WORKTREE_UNCOMMITTED_TRIPWIRE: {0} worktree(s) hold uncommitted work that a cleanup would DESTROY (#1716)." -f $dirtyWorktrees.Count)
    Write-Host "Commit + push each dirty worktree BEFORE pruning. Do NOT sweep these."
    exit 1
}

Write-Host ""
Write-Host "OK: no .claude\worktrees worktree holds uncommitted work -- safe to prune."
exit 0
