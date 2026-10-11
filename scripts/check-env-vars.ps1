#requires -Version 5.1
<#
.SYNOPSIS
    Fail loud if a production environment variable is read in source but not documented
    in docs/env-vars.md.

.DESCRIPTION
    #1492 item 4 documented the runtime env contract in docs/env-vars.md, but a hand-written
    table drifts the moment a new var is added (exactly what happened when LITD_EDITOR_PROJECT_ROOT
    was introduced for MCP path confinement and went undocumented). This makes the contract
    self-enforcing: it scans non-test engine/editor sources for every environment variable that is
    actually read - both literal `env::var("NAME")` calls AND `*_ENV: &str = "NAME"` constants
    (the const indirection is what a naive literal grep misses) - and asserts each appears in
    docs/env-vars.md. A new undocumented production var fails the gate.

    Test-only variables are excluded by construction: `*_tests.rs` files are not scanned, so a var
    read only from tests never needs a production-doc entry.

    No external tools required (pure source scan), so it runs anywhere and composes into
    scripts/lint.ps1 as a fast pre-commit step.

.EXAMPLE
    pwsh -File scripts\check-env-vars.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$docPath = Join-Path $repoRoot "docs\env-vars.md"
if (-not (Test-Path -LiteralPath $docPath -PathType Leaf)) {
    throw "ENV_DOC_CHECK_FAILED: docs/env-vars.md not found at $docPath"
}
$docText = Get-Content -LiteralPath $docPath -Raw

# Only scan production sources: crates/*/src, excluding test files (*_tests.rs). A var read
# solely from a *_tests.rs helper is test-only and intentionally not part of the runtime contract.
$srcFiles = Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Recurse -File -Filter *.rs |
    Where-Object { $_.FullName -match '\\src\\' -and $_.Name -notlike '*_tests.rs' }

# A var name is "ours" - a LITD_/CALYX_/ORT_ namespaced var or a known OS var we document.
$knownOsVars = @("LOCALAPPDATA", "RUST_LOG")
function Test-OwnedVar([string]$name) {
    return ($name -match '^(LITD|CALYX|ORT)_') -or ($knownOsVars -contains $name)
}

$found = [System.Collections.Generic.SortedSet[string]]::new()
foreach ($file in $srcFiles) {
    $text = Get-Content -LiteralPath $file.FullName -Raw
    # Literal env::var / env::var_os / env::set_var calls.
    foreach ($m in [regex]::Matches($text, 'env::(?:var|var_os|set_var)\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')) {
        $name = $m.Groups[1].Value
        if (Test-OwnedVar $name) { [void]$found.Add($name) }
    }
    # `*_ENV*: &str = "NAME"` constants (the indirection a literal grep misses).
    foreach ($m in [regex]::Matches($text, '(?:const|static)\s+\w*ENV\w*\s*:\s*&str\s*=\s*"([A-Za-z_][A-Za-z0-9_]*)"')) {
        $name = $m.Groups[1].Value
        if (Test-OwnedVar $name) { [void]$found.Add($name) }
    }
}

if ($found.Count -eq 0) {
    throw "ENV_DOC_CHECK_FAILED: scanned sources but found no environment variables - the scan regex is likely broken."
}

$missing = @()
foreach ($name in $found) {
    # Documented if the exact name appears anywhere in the doc (it is written as `NAME` in a table).
    if ($docText -notmatch [regex]::Escape($name)) { $missing += $name }
}

if ($missing.Count -gt 0) {
    throw ("ENV_DOC_CHECK_FAILED: these production env vars are read in crates/*/src but are NOT in " +
        "docs/env-vars.md: $($missing -join ', '). Document each (name -> read by -> effect -> unset default), " +
        "or, if a var is genuinely test-only, ensure it is read only from a *_tests.rs file.")
}

Write-Host "ENV_DOC_CHECK_PASSED: all $($found.Count) production env vars read in crates/*/src are documented in docs/env-vars.md"
