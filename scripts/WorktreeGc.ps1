#requires -Version 5.1
<#
.SYNOPSIS
    Junction-safe GC of MERGED agent isolation worktrees under .claude\worktrees (#1707).

.DESCRIPTION
    Stale .claude\worktrees\* agent worktrees accumulate, each carrying its own multi-hundred-k-file
    target\ tree (the #1707 645k-file search-timeout driver). This module prunes ONLY worktrees that
    are provably safe to remove, with hard guards so a live agent or authored-but-uncommitted work is
    NEVER swept (the #1716 data-loss class):

      - LOCKED worktree                 -> KEEP (a `git worktree lock` means the harness/agent owns it).
      - DIRTY (uncommitted/untracked)   -> KEEP (authored work not yet committed; #1716). NEVER prune.
      - UNMERGED (HEAD not an ancestor of origin/main) -> KEEP (carries commits not on mainline).
      - RECENT (dir mtime within -MinAgeHours) -> KEEP (a live agent may be mid-slice).
      - the primary checkout / anything outside .claude\worktrees -> never a candidate.

    Only a worktree that is unlocked AND clean AND merged AND aged is removed. Removal is
    JUNCTION-SAFE per AGENTS.md / #1569: any top-level directory reparse point (e.g. a
    local-asset-vault junction) is UNLINKED first with a non-recursive Directory.Delete (removes the
    link, never its target), THEN `git worktree remove --force` drops the tree. We NEVER robocopy /MIR
    or recursively purge a worktree (that follows junctions and wiped the vault to 0 files in #1569).
    A local-asset-vault top-level count tripwire brackets the whole stage and alarms LOUD if the count
    drops (proof no junction was purged through). `git worktree prune` runs at the end to drop stale
    registrations.

    Only the worktree ROOT is scanned for reparse points (non-recursive): a vault junction lives at
    the worktree root, and a full recursive reparse walk would re-introduce the #1707 645k-file walk.
#>

Set-StrictMode -Version Latest

function ConvertTo-LitdNormPath {
    param([string]$Path)
    try { return ([System.IO.Path]::GetFullPath($Path)).TrimEnd('\', '/').ToLowerInvariant() }
    catch { return $Path.TrimEnd('\', '/').ToLowerInvariant() }
}

function Get-LitdVaultCount {
    param([string]$VaultDir)
    if (-not (Test-Path -LiteralPath $VaultDir -PathType Container)) { return -1 }  # not materialized -> inert
    return @(Get-ChildItem -LiteralPath $VaultDir -Force -ErrorAction SilentlyContinue).Count
}

function Get-LitdWorktreeRecords {
    # Parse `git worktree list --porcelain` into records: Path, Head, Branch, Locked.
    param([string]$RepoRoot)
    $out = & git -C $RepoRoot worktree list --porcelain 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git worktree list failed: $out" }
    $records = New-Object System.Collections.ArrayList
    $cur = $null
    foreach ($line in @($out)) {
        $s = "$line"
        if ($s -like "worktree *") {
            if ($cur) { [void]$records.Add($cur) }
            $cur = [pscustomobject]@{ Path = $s.Substring("worktree ".Length); Head = $null; Branch = $null; Locked = $false }
        } elseif ($s -like "HEAD *" -and $cur) {
            $cur.Head = $s.Substring("HEAD ".Length).Trim()
        } elseif ($s -like "branch *" -and $cur) {
            $cur.Branch = $s.Substring("branch ".Length).Trim()
        } elseif ($s -eq "detached" -and $cur) {
            $cur.Branch = "(detached)"
        } elseif ($s -like "locked*" -and $cur) {
            $cur.Locked = $true
        }
    }
    if ($cur) { [void]$records.Add($cur) }
    return $records
}

function Remove-LitdWorktreeJunctionSafe {
    # Unlink top-level directory junctions/symlinks (non-recursive, link only), then git worktree remove.
    param([string]$RepoRoot, [string]$WorktreePath)
    foreach ($child in (Get-ChildItem -LiteralPath $WorktreePath -Directory -Force -ErrorAction SilentlyContinue)) {
        if (($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            # Non-recursive delete removes ONLY the junction link, never the target it points at (#1569).
            try { [System.IO.Directory]::Delete($child.FullName, $false) }
            catch { Write-Warning "could not unlink junction $($child.FullName): $($_.Exception.Message)" }
        }
    }
    $rm = & git -C $RepoRoot worktree remove --force $WorktreePath 2>&1
    if ($LASTEXITCODE -ne 0) {
        # Do NOT fall back to robocopy /MIR (follows junctions -> #1569). Report loud and leave it.
        throw "git worktree remove failed for ${WorktreePath}: $rm"
    }
}

function Invoke-LitdMergedWorktreePrune {
    <#
      Prune merged+clean+unlocked+aged worktrees under <RepoRoot>\.claude\worktrees. Returns a summary
      object { Removed; Kept; Failed; VaultBefore; VaultAfter; VaultBreach }. -DryRun reports only.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [int]$MinAgeHours = 24,
        [switch]$DryRun
    )
    $summary = [pscustomobject]@{ Removed = 0; Kept = 0; Failed = 0; VaultBefore = -1; VaultAfter = -1; VaultBreach = $false }
    $worktreesRoot = Join-Path $RepoRoot ".claude\worktrees"
    if (-not (Test-Path -LiteralPath $worktreesRoot -PathType Container)) {
        Write-Host "  no .claude\worktrees directory -- nothing to prune"
        return $summary
    }

    # Resolve a mainline ref to test 'merged' against. Fail SAFE: if neither origin/main nor main
    # resolves, prune NOTHING (we cannot prove any branch is merged).
    $mainRef = $null
    foreach ($ref in @("origin/main", "main")) {
        $sha = & git -C $RepoRoot rev-parse --verify --quiet "$ref^{commit}" 2>$null
        if ($LASTEXITCODE -eq 0 -and $sha) { $mainRef = ("$sha" | Select-Object -First 1).Trim(); break }  # fsv-idioms:allow (single sha)
    }
    if (-not $mainRef) {
        Write-Warning "cannot resolve origin/main or main -- skipping worktree prune (cannot prove merged)"
        return $summary
    }

    $vaultDir = Join-Path $RepoRoot "local-asset-vault"
    $summary.VaultBefore = Get-LitdVaultCount -VaultDir $vaultDir

    $worktreesRootNorm = ConvertTo-LitdNormPath $worktreesRoot
    $cutoff = (Get-Date).AddHours(-$MinAgeHours)
    $records = Get-LitdWorktreeRecords -RepoRoot $RepoRoot

    foreach ($wt in $records) {
        $path = $wt.Path
        $norm = ConvertTo-LitdNormPath $path
        # Only worktrees strictly UNDER .claude\worktrees are candidates (never the primary checkout).
        if (-not $norm.StartsWith($worktreesRootNorm + [System.IO.Path]::DirectorySeparatorChar) -and
            -not $norm.StartsWith(($worktreesRootNorm + '/'))) { continue }
        if (-not (Test-Path -LiteralPath $path -PathType Container)) { continue }

        if ($wt.Locked) { $summary.Kept++; Write-Host "  KEEP  $path  <- locked (harness/agent-owned)"; continue }

        $mtime = (Get-Item -LiteralPath $path -Force).LastWriteTime
        if ($mtime -ge $cutoff) { $summary.Kept++; Write-Host "  KEEP  $path  <- recent activity (mtime within ${MinAgeHours}h)"; continue }

        $status = & git -C $path status --porcelain --untracked-files=all 2>&1
        if ($LASTEXITCODE -ne 0) { $summary.Kept++; Write-Host "  KEEP  $path  <- git status error (not safe to prune)"; continue }
        $dirty = @(@($status) | Where-Object { "$_".Trim().Length -gt 0 })
        if ($dirty.Count -gt 0) { $summary.Kept++; Write-Host "  KEEP  $path  <- DIRTY ($($dirty.Count) uncommitted file(s)) [#1716 guard]"; continue }

        # Merged = worktree HEAD is an ancestor of (or equal to) the mainline tip.
        [void](& git -C $RepoRoot merge-base --is-ancestor $wt.Head $mainRef 2>$null)
        if ($LASTEXITCODE -ne 0) { $summary.Kept++; Write-Host "  KEEP  $path  <- UNMERGED (HEAD not an ancestor of $mainRef)"; continue }

        if ($DryRun) {
            $summary.Removed++
            Write-Host "  [dry-run] would remove  $path  <- merged+clean+unlocked+aged (branch $($wt.Branch))"
            continue
        }
        try {
            Remove-LitdWorktreeJunctionSafe -RepoRoot $RepoRoot -WorktreePath $path
            $summary.Removed++
            Write-Host "  removed  $path  <- merged+clean+unlocked+aged (branch $($wt.Branch))"
        } catch {
            $summary.Failed++
            Write-Warning "worktree prune failed for ${path}: $($_.Exception.Message)"
        }
    }

    if (-not $DryRun) { [void](& git -C $RepoRoot worktree prune 2>&1) }

    # Vault tripwire: a top-level count DROP proves a junction was purged through (#1569 data loss).
    $summary.VaultAfter = Get-LitdVaultCount -VaultDir $vaultDir
    if ($summary.VaultBefore -gt 0 -and $summary.VaultAfter -ge 0 -and $summary.VaultAfter -lt $summary.VaultBefore) {
        $summary.VaultBreach = $true
        Write-Warning ("VAULT_TRIPWIRE_BREACH: local-asset-vault top-level count dropped {0} -> {1} during worktree prune -- a directory junction was PURGED THROUGH (#1569 data loss). Re-materialize per docs/assets/asset-vault-runbook.md." -f $summary.VaultBefore, $summary.VaultAfter)
    } elseif ($summary.VaultBefore -ge 0) {
        Write-Host ("  vault tripwire OK: local-asset-vault top-level count {0} unchanged ({1})" -f $summary.VaultBefore, $summary.VaultAfter)
    } else {
        Write-Host "  vault tripwire inert: local-asset-vault not materialized on this host"
    }
    return $summary
}
