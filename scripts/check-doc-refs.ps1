#requires -Version 5.1
<#
.SYNOPSIS
    Doc-reference gate (#1465): every docs2/, docs/, or prompts/ path referenced by a
    git-TRACKED markdown file must itself be git-TRACKED.

.DESCRIPTION
    Root cause of #1465: binding doctrine files (prompts/fsv.md,
    docs2/STRUCTURED_DATA_CALYX_INTELLIGENCE.md, docs2/hardeningprompt.md, ...) were
    referenced by committed documents while existing ONLY in one local working tree.
    They were never committed, so a single disk failure destroyed some of them forever.
    An on-disk existence check would have passed right up to the loss; the invariant
    that actually matters is TRACKED-NESS. This gate therefore:

      1. Enumerates git-tracked .md files OUTSIDE Calyx/ (Calyx/ docs reference
         Calyx-Dev's own tree and tracker and are exempt).
      2. Extracts every reference matching (docs2|docs|prompts)/<path>.(md|ps1|txt).
         Prefixed paths such as Calyx/docs2/... are NOT matched (lookbehind).
      3. FAILS if any referenced path is absent from `git ls-files` - even if the
         file exists on disk. Existence is not the invariant; being committed is.

    Intentional references to paths that are legitimately untracked (e.g. examples,
    generated files) go in scripts/doc-refs-allowlist.txt: one repo-relative
    forward-slash path per line, '#' comments and blank lines ignored. Keep it empty
    unless a reference is deliberately external to the git index.

.EXAMPLE
    powershell -NoProfile -File scripts\check-doc-refs.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$allowlistPath = Join-Path $PSScriptRoot "doc-refs-allowlist.txt"

Push-Location -LiteralPath $repoRoot
try {
    $trackedAll = @(& git ls-files)
    if ($LASTEXITCODE -ne 0) {
        throw "DOC_REFS_GATE_FAILED: step=git-ls-files exit_code=$LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$trackedSet = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($p in $trackedAll) { [void]$trackedSet.Add($p) }

$mdFiles = @($trackedAll | Where-Object { $_ -like '*.md' -and $_ -notlike 'Calyx/*' })

$allow = New-Object 'System.Collections.Generic.HashSet[string]'
if (Test-Path -LiteralPath $allowlistPath -PathType Leaf) {
    foreach ($line in (Get-Content -LiteralPath $allowlistPath)) {
        $t = $line.Trim()
        if ($t -eq '' -or $t.StartsWith('#')) { continue }
        [void]$allow.Add($t)
    }
}

# A reference is a repo-root-relative path starting with docs2/, docs/, or prompts/
# ending in .md/.ps1/.txt. The lookbehind rejects matches embedded in a longer path
# (e.g. Calyx/docs2/x.md) or word (e.g. mydocs/x.md).
$refPattern = '(?<![\w.\\/-])(?:docs2|docs|prompts)/[\w./-]*?\.(?:md|ps1|txt)'

$failures = New-Object System.Collections.Generic.List[string]
$refsChecked = 0

foreach ($rel in $mdFiles) {
    $full = Join-Path $repoRoot ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        # Tracked but deleted from disk; nothing to scan (git status will surface it).
        continue
    }
    $lineNo = 0
    foreach ($line in (Get-Content -LiteralPath $full)) {
        $lineNo += 1
        foreach ($m in [regex]::Matches($line, $refPattern)) {
            $ref = $m.Value
            $refsChecked += 1
            if ($allow.Contains($ref)) { continue }
            if (-not $trackedSet.Contains($ref)) {
                $failures.Add("DANGLING  ${rel}:${lineNo} references '$ref' which is NOT git-tracked (on-disk existence does not count - #1465). Commit the referenced file, fix the reference, or allowlist it in scripts/doc-refs-allowlist.txt.")
            }
        }
    }
}

Write-Host "DOC_REFS_GATE md_files_scanned=$($mdFiles.Count) refs_checked=$refsChecked allowlisted=$($allow.Count)"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "DOC_REFS_GATE_FAILED count=$($failures.Count): tracked markdown references untracked docs2/docs/prompts paths (#1465). Fix as listed above."
}
Write-Host "DOC_REFS_GATE_OK every referenced docs2/docs/prompts path is git-tracked."
