#requires -Version 5.1
<#
.SYNOPSIS
    Ratchet gate for the #800 sec.6 "<500 lines per .rs file" modularization doctrine (#1466).

.DESCRIPTION
    Cargo/rustc do not enforce a per-file line budget, so files silently creep over 500 lines
    (the audit found 33; a later scan, 44). A one-shot "split everything" is a giant risky sprint,
    so this uses the standard ratchet pattern (imbue-ai/ratchets, Notion): a committed baseline
    lists every file that ALREADY violates the rule with its current line count; the gate then
    enforces that the set of violations only ever DECREASES:

      - a file NOT in the baseline that reaches >=500 lines  -> FAIL (a NEW violation)
      - a baselined file that GROWS beyond its recorded count -> FAIL (regression of a known one)
      - a baselined file now UNDER 500                        -> FAIL: the win must be locked in by
                                                                removing its row (run with -Update),
                                                                so it can never regress
      - a baseline row whose file no longer exists            -> FAIL (stale row; -Update fixes it)

    New/unlisted code must therefore be modular from day one, and the 44 legacy files can be split
    "leisurely" worst-first, each split shrinking the baseline until it is empty (the #1466 accept
    gate). Put on the hot path via scripts/check-format.ps1 so no commit can reintroduce the debt.

.PARAMETER Update
    Rewrite the baseline from the current tree (record every current >=500-line file at its current
    count, drop rows for files now under 500 or deleted). Use after a legitimate split to lock the
    improvement in, or to re-baseline a deliberate reduction. Review the diff before committing.

.EXAMPLE
    pwsh -File scripts\check-line-count.ps1
.EXAMPLE
    pwsh -File scripts\check-line-count.ps1 -Update
#>
[CmdletBinding()]
param([switch]$Update)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$baselinePath = Join-Path $PSScriptRoot "line-count-baseline.tsv"
$limit = 500  # #800 sec.6: a .rs file must be < 500 lines; >= 500 is a violation.

# Repo-relative, forward-slash path for a file (matches the baseline's stored form).
function Get-RelPath([string]$fullPath) {
    $rel = $fullPath.Substring($repoRoot.Length).TrimStart('\', '/')
    return $rel -replace '\\', '/'
}

# Every tracked .rs file under crates\, excluding build output.
$rsFiles = Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Recurse -Filter *.rs -File -ErrorAction Stop |
    Where-Object { $_.FullName -notmatch '[\\/]target[\\/]' }

# Current violations: rel-path -> line count for every file at or over the limit.
$current = @{}
foreach ($file in $rsFiles) {
    $count = (Get-Content -LiteralPath $file.FullName).Count
    if ($count -ge $limit) {
        $current[(Get-RelPath $file.FullName)] = $count
    }
}

if ($Update) {
    $header = @(
        "# Line-count ratchet baseline (#1466). One row per .rs file currently >=500 lines (violates",
        "# #800 sec.6 <500-line doctrine). Format: <repo-relative-path>\t<current_line_count>. The gate",
        "# (scripts/check-line-count.ps1) fails if a NON-listed file reaches 500 lines, or a LISTED file",
        "# GROWS beyond its recorded count. When a listed file is split below 500, DELETE its row to lock",
        "# the win in (it can then never regress). Goal: shrink this file to empty (#1466 accept gate)."
    )
    $rows = $current.Keys | Sort-Object | ForEach-Object { "$_`t$($current[$_])" }
    Set-Content -LiteralPath $baselinePath -Value ($header + $rows) -Encoding UTF8
    Write-Host "LINE_COUNT_BASELINE_UPDATED entries=$($current.Count) file=$baselinePath"
    return
}

# Load the committed baseline (skip comment/blank lines).
if (-not (Test-Path -LiteralPath $baselinePath)) {
    throw "LINE_COUNT_BASELINE_MISSING: $baselinePath not found (run -Update to create it)."
}
$baseline = @{}
foreach ($line in (Get-Content -LiteralPath $baselinePath)) {
    if ($line -match '^\s*#' -or $line.Trim() -eq '') { continue }
    $parts = $line -split "`t"
    if ($parts.Count -ne 2) { throw "LINE_COUNT_BASELINE_MALFORMED row: '$line'" }
    $baseline[$parts[0].Trim()] = [int]$parts[1].Trim()
}

$failures = New-Object System.Collections.Generic.List[string]

# 1. Check every current file against the baseline.
foreach ($rel in ($current.Keys | Sort-Object)) {
    $count = $current[$rel]
    if ($baseline.ContainsKey($rel)) {
        if ($count -gt $baseline[$rel]) {
            $failures.Add("GREW      $rel is $count lines (baseline $($baseline[$rel])). Split it back down; a baselined file may shrink but never grow.")
        }
    } else {
        $failures.Add("NEW       $rel is $count lines (>= $limit) and is not baselined. New code must be < $limit lines (#800 sec.6) - split it into modules.")
    }
}

# 2. Baselined files that are now compliant (or gone) must be de-listed to lock the ratchet.
foreach ($rel in ($baseline.Keys | Sort-Object)) {
    if (-not $current.ContainsKey($rel)) {
        $full = Join-Path $repoRoot ($rel -replace '/', '\')
        if (Test-Path -LiteralPath $full) {
            $failures.Add("LOCK-IN   $rel is now under $limit lines - remove its baseline row (run -Update) so it can never regress.")
        } else {
            $failures.Add("STALE     $rel is baselined but no longer exists - remove its row (run -Update).")
        }
    }
}

Write-Host "LINE_COUNT_GATE files_scanned=$($rsFiles.Count) violations_baselined=$($baseline.Count) violations_now=$($current.Count)"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "LINE_COUNT_GATE_FAILED count=$($failures.Count): the <500-line ratchet (#1466/#800 sec.6) was violated. Fix as above; after a legitimate split run scripts\check-line-count.ps1 -Update to re-baseline."
}
Write-Host "LINE_COUNT_GATE_OK all .rs files are either < $limit lines or at/below their ratcheted baseline."
