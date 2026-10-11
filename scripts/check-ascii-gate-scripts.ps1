#requires -Version 5.1
<#
.SYNOPSIS
    ASCII-only gate (#1570): every git-tracked gate script under scripts/*.ps1 and
    .githooks/* must contain ONLY ASCII bytes (codepoints <= U+007F).

.DESCRIPTION
    Doctrine (#800 sec.6 / AGENTS.md): gate scripts must be ASCII-only. The reason is
    concrete, not stylistic. The scripts are stored UTF-8 WITHOUT a BOM. They run under
    pwsh (PowerShell 7, UTF-8 by default) but the .githooks/pre-commit hook FALLS BACK to
    Windows PowerShell 5.1 when pwsh is absent. PS 5.1 with no BOM decodes the file as the
    system ANSI codepage, so every multi-byte UTF-8 character (an em-dash U+2014, a smart
    quote, an arrow, a box-drawing rule) becomes garbage that breaks the enclosing string
    literal and the block around it. The gate then dies with a MISLEADING parse error
    instead of its real diagnostic -- the same false-signal class this repo keeps fighting
    (#1543/#1536/#1545/#1556). A verification that breaks under a different interpreter is
    worse than none: it manufactures false confidence.

    Nothing enforced the rule, so the em-dashes crept back in (#1570). This gate is the
    enforcement: it fails loud, naming file:line:column and the offending codepoint, so the
    doctrine is mechanically checked instead of remembered.

    Scope is exactly the doctrine scope: tracked scripts/*.ps1 and .githooks/* (NOT
    Calyx/scripts, which follows its own conventions inside GameEditor's Calyx copy).

.EXAMPLE
    powershell -NoProfile -File scripts\check-ascii-gate-scripts.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path

Push-Location -LiteralPath $repoRoot
try {
    $tracked = @(& git ls-files -- "scripts/*.ps1" ".githooks/*")
    if ($LASTEXITCODE -ne 0) {
        throw "ASCII_GATE_FAILED: step=git-ls-files exit_code=$LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$failures = New-Object System.Collections.Generic.List[string]
$filesScanned = 0

foreach ($rel in $tracked) {
    $full = Join-Path $repoRoot ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
        # Tracked but deleted from disk; git status will surface it.
        continue
    }
    $filesScanned += 1

    # Read as UTF-8 lines so a multi-byte character is reported as its true codepoint
    # (U+XXXX), not as separate raw bytes. Invalid UTF-8 decodes to U+FFFD, still > 127,
    # still caught.
    $lines = @(Get-Content -LiteralPath $full -Encoding UTF8)
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        for ($c = 0; $c -lt $line.Length; $c++) {
            $code = [int]$line[$c]
            if ($code -gt 127) {
                $lineNo = $i + 1
                $colNo = $c + 1
                $cp = 'U+{0:X4}' -f $code
                $failures.Add(("NON_ASCII  {0}:{1}:{2}  {3} '{4}'" -f $rel, $lineNo, $colNo, $cp, $line[$c]))
            }
        }
    }
}

Write-Host "ASCII_GATE files_scanned=$filesScanned scope=scripts/*.ps1,.githooks/*"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    Write-Host "  Fix: replace each non-ASCII character with an ASCII equivalent"
    Write-Host "       (em-dash U+2014 -> '-' or '--'; smart quotes -> straight ' or `"; arrows -> '->';"
    Write-Host "        ellipsis U+2026 -> '...'; box-drawing U+2500 -> '-'; section U+00A7 -> 'sec.')."
    Write-Host "       Gate scripts are stored without a BOM, so Windows PowerShell 5.1 mangles any"
    Write-Host "       non-ASCII byte into a parse error (#800 sec.6, #1570). Keep them ASCII-only."
    throw "ASCII_GATE_FAILED count=$($failures.Count): tracked gate scripts contain non-ASCII bytes (#1570). Fix as listed above."
}
Write-Host "ASCII_GATE_OK every tracked scripts/*.ps1 and .githooks/* file is ASCII-only."
