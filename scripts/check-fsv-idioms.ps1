#requires -Version 5.1
<#
.SYNOPSIS
    FSV-idiom gate (#1556): ban two PowerShell idioms that silently produce WRONG
    verification results across tracked *.md and *.ps1 (outside Calyx/).

.DESCRIPTION
    An FSV instruction or accept gate is itself code and must itself be verified. Two
    idioms lie:

      (A) `Measure-Object -Line` does NOT count blank lines, so it UNDERCOUNTS. Any
          line/row count built on it reports a success it has not earned (it hid 18 of 30
          violations in the #1466 accept gate). It is essentially never correct for
          counting. Use the true counter instead:
            - object streams:  @(...).Count
            - file lines:      @(Get-Content -Raw $f -split "`n").Count   or   wc -l

      (B) `| Select-Object -First N` short-circuits the pipeline. When a NATIVE command is
          piped into it, PowerShell stops the upstream early and $LASTEXITCODE is left
          stale -- a FAILING command then reads as exit 0. This corrupts the exit code
          every gate decides on. Capture first, THEN read the code:
            $out = & cmd 2>&1 | Out-String; $code = $LASTEXITCODE

    Select-Object -First is LEGITIMATE for taking N data items from an in-memory stream
    (arrays, cmdlet output, JSON). It is ONLY a bug when a $LASTEXITCODE read follows it
    and matters. So (B) flags a `Select-Object -First` ONLY when a `$LASTEXITCODE`
    reference appears within the next few lines. A benign use whose nearby $LASTEXITCODE
    is genuinely unrelated can be marked with an inline `# fsv-idioms:allow` comment on the
    Select-Object line -- but NEVER weaken correctness just to silence the lint.

    Any line carrying the marker `fsv-idioms:allow` is skipped. The two lint scripts
    themselves are skipped (they necessarily quote the banned idioms in their messages).

.EXAMPLE
    powershell -NoProfile -File scripts\check-fsv-idioms.ps1
#>
[CmdletBinding()]
param(
    # How many lines after a `Select-Object -First` to scan for a $LASTEXITCODE read.
    [int]$ProximityLines = 5
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path

# Lint scripts quote the banned idioms in their own help/messages; exclude them by name.
$selfExclude = @("check-fsv-idioms.ps1", "check-ascii-gate-scripts.ps1")
$allowMarker = "fsv-idioms:allow"

Push-Location -LiteralPath $repoRoot
try {
    $tracked = @(& git ls-files -- "*.md" "*.ps1")
    if ($LASTEXITCODE -ne 0) {
        throw "FSV_IDIOM_GATE_FAILED: step=git-ls-files exit_code=$LASTEXITCODE"
    }
} finally {
    Pop-Location
}

# Calyx/ is GameEditor's customized Calyx copy with its own conventions and tracker; not policed here.
$targets = @($tracked | Where-Object {
    $_ -notlike 'Calyx/*' -and ($selfExclude -notcontains (Split-Path -Leaf $_))
})

$measurePattern = 'Measure-Object\b[^\r\n]*?-Line\b'
$selectPattern = 'Select-Object\b[^\r\n]*?-First\b'
$exitCodePattern = '\$LASTEXITCODE'

$failures = New-Object System.Collections.Generic.List[string]
$filesScanned = 0

foreach ($rel in $targets) {
    $full = Join-Path $repoRoot ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { continue }
    $filesScanned += 1

    $lines = @(Get-Content -LiteralPath $full)
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        if ($line -match [regex]::Escape($allowMarker)) { continue }
        $lineNo = $i + 1

        # (A) blank-line-blind counter
        if ($line -match $measurePattern) {
            $failures.Add(("BLIND_COUNT   {0}:{1}  'Measure-Object -Line' undercounts (skips blank lines); it is essentially never correct for counting. Use @(...).Count for object streams, or wc -l / (Get-Content -Raw split-on-newline).Count for file lines (#1556)." -f $rel, $lineNo))
        }

        # (B) exit-code-corrupting Select-Object -First followed by a $LASTEXITCODE read
        if ($line -match $selectPattern) {
            $windowEnd = [Math]::Min($i + $ProximityLines, $lines.Count - 1)
            $offender = $null
            for ($j = $i + 1; $j -le $windowEnd; $j++) {
                if ($lines[$j] -match [regex]::Escape($allowMarker)) { continue }
                if ($lines[$j] -match $exitCodePattern) { $offender = $j + 1; break }
            }
            if ($null -ne $offender) {
                $failures.Add(("EXITCODE_CORRUPT  {0}:{1}  'Select-Object -First' short-circuits the pipeline; the `$LASTEXITCODE read at line {2} may read a stale/zero code, so a failing native command passes. Capture first: `$out = & cmd 2>&1 | Out-String; `$code = `$LASTEXITCODE (#1556). If the Select-Object is on in-memory data and this `$LASTEXITCODE is unrelated, add an inline '# {3}' comment." -f $rel, $lineNo, $offender, $allowMarker))
            }
        }
    }
}

Write-Host "FSV_IDIOM_GATE files_scanned=$filesScanned scope=*.md,*.ps1 (excl Calyx/) proximity=$ProximityLines"
if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "FSV_IDIOM_GATE_FAILED count=$($failures.Count): tracked docs/scripts use blind measurement idioms (#1556). Fix as listed above."
}
Write-Host "FSV_IDIOM_GATE_OK no blank-line-blind counts and no exit-code-corrupting Select-Object -First found."
