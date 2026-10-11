#requires -Version 5.1
<#
.SYNOPSIS
    Integrity gate for doctrine-referenced binding files (#1465).

.DESCRIPTION
    Binding doctrine documents (prompts/fsv.md cited by #800, the docs2 handbooks, etc.)
    were cited as authoritative by GitHub issue bodies but were never guaranteed to be
    tracked by git. In #1465 exactly this diverged: prompts/fsv.md was silently deleted by
    the 94aa58c8 workspace-sync commit while #800 still cited it as binding, and
    docs2/STRUCTURED_DATA_CALYX_INTELLIGENCE.md was lost entirely because it lived in a
    single working tree and was never committed. Nothing detected the drift between "what
    doctrine cites" and "what a clean checkout actually contains".

    This gate closes that class. It reads scripts/doctrine-refs-manifest.tsv (one repo-
    relative path per line) and asserts every listed path is tracked by git. A path that is
    not tracked FAILS with the path named, so a doctrine-cited file that is deleted, renamed
    away, or only ever existed in a working tree can never pass the pre-commit gate again.

    Minimal-sufficient by design: "tracked" is the whole invariant doctrine needs (a tracked
    file is present on every clean checkout), so the check is a single `git ls-files
    --error-unmatch` per row -- fast, no build, runs on every gate invocation.

.EXAMPLE
    pwsh -File scripts\check-doctrine-refs.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifestPath = Join-Path $PSScriptRoot "doctrine-refs-manifest.tsv"

if (-not (Test-Path -LiteralPath $manifestPath)) {
    throw "DOCTRINE_REFS_MANIFEST_MISSING: $manifestPath not found."
}

$paths = New-Object System.Collections.Generic.List[string]
foreach ($line in (Get-Content -LiteralPath $manifestPath)) {
    $trimmed = $line.Trim()
    if ($trimmed -eq '' -or $trimmed.StartsWith('#')) { continue }
    $paths.Add($trimmed)
}

$trackedPaths = @(& git -C $repoRoot ls-files)
if ($LASTEXITCODE -ne 0) {
    throw "DOCTRINE_REFS_GATE_FAILED: step=git-ls-files exit_code=$LASTEXITCODE"
}
$trackedSet = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($path in $trackedPaths) {
    [void]$trackedSet.Add(($path -replace '\\', '/'))
}

$failures = New-Object System.Collections.Generic.List[string]

foreach ($rel in $paths) {
    # Working-tree presence is not enough -- an un-added file is the #1465 defect.
    if (-not $trackedSet.Contains(($rel -replace '\\', '/'))) {
        $failures.Add("UNTRACKED $rel is cited as binding doctrine but is NOT tracked by git. Commit it (git add $rel) or, if the reference is retired, remove its row from scripts/doctrine-refs-manifest.tsv.")
    }
}

Write-Host "DOCTRINE_REFS_GATE files_checked=$($paths.Count)"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "DOCTRINE_REFS_GATE_FAILED count=$($failures.Count): a doctrine-referenced binding file is missing from the tracked tree (#1465). A clean checkout would not contain it, so the doctrine that cites it is broken. Fix as above."
}
Write-Host "DOCTRINE_REFS_GATE_OK all $($paths.Count) doctrine-referenced files are tracked."
