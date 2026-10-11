#requires -Version 5.1
<#
.SYNOPSIS
    Ratchet gate against raw std::env::temp_dir() usage (#1535 temp-dir leak class).

.DESCRIPTION
    A raw `std::env::temp_dir().join("litd-...")` in test code has no cleanup, so the directory
    leaks under %TEMP% forever (a full-suite run left 612 stale `litd-*` dirs; #1495 migrated only
    7 of ~50 helpers). The sanctioned primitive is `litd_engine::test_temp::TestTempDir`, which
    `remove_dir_all`s its path on Drop.

    Cargo/rustc do not forbid raw temp_dir, so this uses the same ratchet as check-line-count.ps1
    (imbue-ai/ratchets): a committed baseline lists every .rs file that currently contains raw
    `env::temp_dir()` with its occurrence count (the sanctioned `test_temp.rs` is the ONE allowed
    home and is always excluded). The gate then enforces the set only ever DECREASES:

      - a file NOT in the baseline that gains an `env::temp_dir()` call -> FAIL (new leak risk)
      - a baselined file whose count GROWS                              -> FAIL (regression)
      - a baselined file now at 0 occurrences                           -> FAIL: lock the win in by
                                                                          removing its row (-Update)
      - a baseline row whose file no longer exists                      -> FAIL (stale; -Update fixes)

    New code must therefore use TestTempDir from day one; the ~50 legacy helpers migrate worst-first
    (each removing its row) until the baseline is empty (the #1535 accept gate). Runtime files that
    legitimately need env::temp_dir() are simply baselined (frozen, reviewed on any bump).

.PARAMETER Update
    Rewrite the baseline from the current tree. Run after migrating a helper to TestTempDir (or when
    intentionally adding a reviewed runtime usage). Review the diff before committing.

.EXAMPLE
    pwsh -File scripts\check-temp-dir-hygiene.ps1
.EXAMPLE
    pwsh -File scripts\check-temp-dir-hygiene.ps1 -Update
#>
[CmdletBinding()]
param([switch]$Update)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$baselinePath = Join-Path $PSScriptRoot "temp-dir-hygiene-baseline.tsv"
$pattern = 'env::temp_dir\(\)'  # matches both `std::env::temp_dir()` and `env::temp_dir()`.

# Repo-relative, forward-slash path (matches the baseline's stored form).
function Get-RelPath([string]$fullPath) {
    $rel = $fullPath.Substring($repoRoot.Length).TrimStart('\', '/')
    return $rel -replace '\\', '/'
}

# Every .rs file under crates\, excluding build output and the sanctioned TestTempDir home.
$rsFiles = Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Recurse -Filter *.rs -File -ErrorAction Stop |
    Where-Object { $_.FullName -notmatch '[\\/]target[\\/]' } |
    Where-Object { $_.Name -ne 'test_temp.rs' }

# Current usages: rel-path -> count of lines calling env::temp_dir(), for every file with >= 1.
# Strip each line's trailing `//` comment BEFORE matching so a fix note that merely MENTIONS
# `env::temp_dir()` (e.g. "// migrated away from env::temp_dir()") is not counted as a usage.
$current = @{}
foreach ($file in $rsFiles) {
    $count = @(
        Get-Content -LiteralPath $file.FullName |
            ForEach-Object { $_ -replace '//.*$', '' } |
            Select-String -Pattern $pattern
    ).Count
    if ($count -ge 1) {
        $current[(Get-RelPath $file.FullName)] = $count
    }
}

if ($Update) {
    $header = @(
        "# Raw env::temp_dir() ratchet baseline (#1535). One row per .rs file still calling raw",
        "# std::env::temp_dir() (test_temp.rs, the sanctioned TestTempDir home, is excluded). Format:",
        "# <repo-relative-path>\t<occurrence_count>. The gate (scripts/check-temp-dir-hygiene.ps1)",
        "# fails if a NON-listed file gains a call, or a LISTED file's count GROWS. When a file is",
        "# migrated to TestTempDir (0 calls), DELETE its row to lock the win. Goal: empty (#1535)."
    )
    $rows = $current.Keys | Sort-Object | ForEach-Object { "$_`t$($current[$_])" }
    Set-Content -LiteralPath $baselinePath -Value ($header + $rows) -Encoding UTF8
    Write-Host "TEMP_DIR_HYGIENE_BASELINE_UPDATED entries=$($current.Count) file=$baselinePath"
    return
}

if (-not (Test-Path -LiteralPath $baselinePath)) {
    throw "TEMP_DIR_HYGIENE_BASELINE_MISSING: $baselinePath not found (run -Update to create it)."
}
$baseline = @{}
foreach ($line in (Get-Content -LiteralPath $baselinePath)) {
    if ($line -match '^\s*#' -or $line.Trim() -eq '') { continue }
    $parts = $line -split "`t"
    if ($parts.Count -ne 2) { throw "TEMP_DIR_HYGIENE_BASELINE_MALFORMED row: '$line'" }
    $baseline[$parts[0].Trim()] = [int]$parts[1].Trim()
}

$failures = New-Object System.Collections.Generic.List[string]

foreach ($rel in ($current.Keys | Sort-Object)) {
    $count = $current[$rel]
    if ($baseline.ContainsKey($rel)) {
        if ($count -gt $baseline[$rel]) {
            $failures.Add("GREW      $rel has $count env::temp_dir() calls (baseline $($baseline[$rel])). Use litd_engine::test_temp::TestTempDir (auto-cleans on Drop); a baselined file may shrink but never grow.")
        }
    } else {
        $failures.Add("NEW       $rel calls raw env::temp_dir() ($count) and is not baselined. Use litd_engine::test_temp::TestTempDir instead (it removes its dir on Drop; raw temp_dir leaks - #1535).")
    }
}

foreach ($rel in ($baseline.Keys | Sort-Object)) {
    if (-not $current.ContainsKey($rel)) {
        $full = Join-Path $repoRoot ($rel -replace '/', '\')
        if (Test-Path -LiteralPath $full) {
            $failures.Add("LOCK-IN   $rel no longer calls env::temp_dir() - remove its baseline row (run -Update) so the leak can never regress.")
        } else {
            $failures.Add("STALE     $rel is baselined but no longer exists - remove its row (run -Update).")
        }
    }
}

Write-Host "TEMP_DIR_HYGIENE_GATE files_scanned=$($rsFiles.Count) usages_baselined=$($baseline.Count) usages_now=$($current.Count)"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "TEMP_DIR_HYGIENE_GATE_FAILED count=$($failures.Count): the raw env::temp_dir() ratchet (#1535) was violated. Fix as above; after a legitimate migration run scripts\check-temp-dir-hygiene.ps1 -Update to re-baseline."
}
Write-Host "TEMP_DIR_HYGIENE_GATE_OK no new raw env::temp_dir() usage; the leak class only shrinks."
