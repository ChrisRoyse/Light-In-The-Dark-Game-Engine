[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# #1528: install the repo git hooks by pointing core.hooksPath at the tracked
# .githooks directory. Git does not clone hooks, so every clone runs this once to
# activate the pre-commit format gate. Fails loud on any error - never silently
# leaves the repo without an enforced gate.

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$hooksDir = Join-Path $repoRoot ".githooks"
$preCommit = Join-Path $hooksDir "pre-commit"
# #1553: the pre-push correctness gate (cargo check + test gate + examples + lint) must also be
# present, else `core.hooksPath` would activate a push path with no correctness gate.
$prePush = Join-Path $hooksDir "pre-push"

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw "INSTALL_HOOKS_FAILED: git was not found on PATH"
}
foreach ($hook in @($preCommit, $prePush)) {
    if (-not (Test-Path -LiteralPath $hook -PathType Leaf)) {
        throw "INSTALL_HOOKS_FAILED: expected hook not found at $hook"
    }
}

Push-Location -LiteralPath $repoRoot
try {
    & git config core.hooksPath .githooks
    if ($LASTEXITCODE -ne 0) {
        throw "INSTALL_HOOKS_FAILED: 'git config core.hooksPath .githooks' exit_code=$LASTEXITCODE"
    }
    $configured = (& git config --get core.hooksPath)
    if ($configured -ne ".githooks") {
        throw "INSTALL_HOOKS_FAILED: core.hooksPath readback='$configured' expected='.githooks'"
    }
    Write-Host "GIT_HOOKS_INSTALLED core.hooksPath=$configured pre_commit=$preCommit pre_push=$prePush"
} finally {
    Pop-Location
}
