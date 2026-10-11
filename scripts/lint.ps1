[CmdletBinding()]
param(
    # Fail on clippy warnings (`-D warnings`). This is the intended pre-commit
    # gate behavior. Omit for a non-gating "show me the lint issues" report that
    # still fails on genuine clippy *errors* and on cargo-deny violations.
    [switch]$Deny,

    # The routine gate lints production library and binary targets only. Use this
    # explicit opt-in for exhaustive unit-test, integration-test, example, and bench
    # target linting when doing a full audit.
    [switch]$AllTargets,

    # Optional exact GameEditor package selection for a dependency-light targeted gate.
    # Calyx clippy is omitted only when this is supplied; cargo-deny still runs for both
    # workspaces and env-contract coverage always runs.
    [string[]]$GameEditorPackages,

    # Optional stable Cargo target cache. The publish gate passes its warm gate cache here so
    # clippy reuses the preceding production/example compilation instead of rebuilding into the
    # unrelated workspace target tree. Standalone lint invocations retain Cargo's default target.
    [string]$TargetDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# The publish gate crosses a child PowerShell process, where an array-valued parameter is
# serialized as one comma-delimited token. Normalize that transport form once so direct array
# callers and child-process callers have identical package selection semantics.
if ($null -ne $GameEditorPackages -and $GameEditorPackages.Count -eq 1 -and
    $GameEditorPackages[0].Contains(",")) {
    $GameEditorPackages = @(
        $GameEditorPackages[0].Split(",", [System.StringSplitOptions]::RemoveEmptyEntries) |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_ -ne "" }
    )
}

# Local lint gate (#1492): runs clippy + cargo-deny over both the GameEditor and
# the vendored Calyx workspaces. There is no CI at the GameEditor root - local FSV
# is the gate (#800) - so this is the pre-commit quality gate that check-format.ps1
# does not cover. Fail loud on any missing tool or any gate violation; never skip a
# tool silently.

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$resolvedTargetDir = $null
if ($TargetDir) {
    $targetResolver = Join-Path $PSScriptRoot "LitdOwnedTarget.ps1"
    if (-not (Test-Path -LiteralPath $targetResolver -PathType Leaf)) {
        throw "LINT_GATE_FAILED: target resolver not found: $targetResolver"
    }
    . $targetResolver
    $resolvedTargetDir = Resolve-LitdOwnedTargetPath -RepoRoot $repoRoot -TargetDir $TargetDir
    New-Item -ItemType Directory -Force -Path $resolvedTargetDir | Out-Null
    Write-Host "LINT_GATE_SOURCE_OF_TRUTH target_dir=$resolvedTargetDir"
}

# #1809: 5.1-safe native-command invocation. Under Windows PowerShell 5.1 a native tool's
# stderr line can raise a script-terminating NativeCommandError while $ErrorActionPreference
# is 'Stop', misreporting clean clippy/cargo-deny output (which both write to stderr) as a
# failure before $LASTEXITCODE is read. Invoke-GateNativeStream streams output live and returns
# the real exit code without that abort. Dot-sourced (functions only).
$gateNativeHelper = Join-Path $PSScriptRoot "gate-native-command.ps1"
if (-not (Test-Path -LiteralPath $gateNativeHelper -PathType Leaf)) {
    throw "LINT_GATE_FAILED: helper not found: $gateNativeHelper"
}
. $gateNativeHelper

# Env-contract drift gate (#1492 item 4): every production env var read in crates/*/src must be
# in docs/env-vars.md. Runs first - it needs no toolchain, so the contract is checked even on a
# host without clippy/cargo-deny installed.
Write-Host "== env-vars doc coverage =="
& (Join-Path $PSScriptRoot "check-env-vars.ps1")

# Each workspace is gated by cargo-deny. A workspace either ships its own in-tree
# deny.toml (GameEditor root) OR is covered by an external policy file kept in tracked
# scripts/ (Calyx - GameEditor's own permanently-customized copy whose advisory/license gate
# this repo owns, #1517). There is NO third case: a workspace with neither fails loud below,
# never a silent skip.
#
# FsvFeatures (#1816): a curated feature combo the GameEditor workspace is ALSO clippy-checked
# under, on top of the default-feature pass. Exhaustive target linting is opt-in via -AllTargets;
# the routine push gate separately compiles every FSV example with check-examples.ps1. We
# deliberately do NOT use --all-features (it enables cuda, which needs a CUDA toolkit) nor a full
# feature powerset; per cargo-hack / Microsoft Rust engineering guidance, lint one curated combo
# that covers the targets, keeping the routine feedback loop bounded. Calyx defines no `fsv`
# feature, so it gets no extra pass ($null).
$workspaces = @(
    @{ Name = "GameEditor"; Root = $repoRoot;                     DenyConfig = $null;                                        FsvFeatures = @("fsv", "calyx-surface") },
    @{ Name = "Calyx";      Root = (Join-Path $repoRoot "Calyx"); DenyConfig = (Join-Path $PSScriptRoot "calyx-deny.toml"); FsvFeatures = $null }
)

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "LINT_GATE_FAILED: cargo was not found on PATH"
}
# clippy ships as a rustup component (see rust-toolchain.toml).
$clippyProbe = Invoke-GateNativeCommand -FilePath "cargo" -ArgumentList @("clippy", "--version")
if ($clippyProbe.ExitCode -ne 0) {
    throw "LINT_GATE_FAILED: cargo-clippy not available. Install with: rustup component add clippy"
}
if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) {
    throw "LINT_GATE_FAILED: cargo-deny not found on PATH. Install with: cargo install --locked cargo-deny"
}

# Run one clippy pass over a workspace. $Features is a (possibly empty) feature list; empty
# means the default-feature pass. Fails loud (never a silent skip) on any non-zero exit.
function Invoke-ClippyPass {
    param(
        [Parameter(Mandatory)] [string]$WorkspaceName,
        [string[]]$Features
    )
    $passArgs = @("clippy", "--locked")
    if ($GameEditorPackages -and $WorkspaceName -eq "GameEditor") {
        foreach ($package in $GameEditorPackages) {
            $passArgs += @("--package", $package)
        }
    } else {
        $passArgs += "--workspace"
    }
    if ($AllTargets) {
        $passArgs += "--all-targets"
    } else {
        $passArgs += @("--lib", "--bins")
    }
    $label = "default features"
    if ($Features) {
        $passArgs += @("--features", ($Features -join ","))
        $label = "--features $($Features -join ',')"
    }
    if ($resolvedTargetDir) {
        # Cargo accepts --target-dir anywhere in the command options; append before invocation
        # so every clippy feature pass shares the same owned warm cache.
        $passArgs += @("--target-dir", $resolvedTargetDir)
    }
    if ($Deny) {
        $passArgs += @("--", "-D", "warnings")
    }
    $targetLabel = if ($AllTargets) { "all targets" } else { "production lib/bin targets" }
    if ($GameEditorPackages -and $WorkspaceName -eq "GameEditor") {
        $targetLabel += "; packages=$($GameEditorPackages -join ',')"
    }
    Write-Host "== clippy: $WorkspaceName ($label; $targetLabel) =="
    # #1809: stream via the 5.1-safe helper -- clippy writes all progress/diagnostics to
    # stderr, which would abort the gate under EAP=Stop on PS 5.1 before the exit code is read.
    $clippyExit = Invoke-GateNativeStream -FilePath "cargo" -ArgumentList $passArgs
    if ($clippyExit -ne 0) {
        throw "LINT_GATE_FAILED: workspace=$WorkspaceName step=clippy features='$label' exit_code=$clippyExit"
    }
}

foreach ($ws in $workspaces) {
    if (-not (Test-Path -LiteralPath (Join-Path $ws.Root "Cargo.toml") -PathType Leaf)) {
        throw "LINT_GATE_FAILED: workspace=$($ws.Name) manifest not found under $($ws.Root)"
    }
    Push-Location -LiteralPath $ws.Root
    try {
        $targetedGameEditor = $GameEditorPackages -and $ws.Name -eq "GameEditor"
        $skipCalyxClippy = $GameEditorPackages -and $ws.Name -eq "Calyx"
        if ($skipCalyxClippy) {
            Write-Host "== clippy: Calyx (skipped; targeted GameEditor package gate) =="
        } else {
            # Default-feature clippy pass (all workspaces, or selected GameEditor packages).
            Invoke-ClippyPass -WorkspaceName $ws.Name -Features @()
            # Curated feature-combo pass remains for the normal full GameEditor gate.
            if ($ws.FsvFeatures -and -not $targetedGameEditor) {
                Invoke-ClippyPass -WorkspaceName $ws.Name -Features $ws.FsvFeatures
            }
        }

        # cargo-deny license policy (#1073) + advisories/bans. Every workspace is gated:
        # an in-tree deny.toml if present, else an external policy from scripts/, else
        # fail loud. Never skip cargo-deny silently for a whole workspace (#1517).
        $inTreeDeny = Join-Path $ws.Root "deny.toml"
        if (Test-Path -LiteralPath $inTreeDeny -PathType Leaf) {
            Write-Host "== cargo-deny: $($ws.Name) (in-tree deny.toml) =="
            # #1809: cargo-deny writes its human-readable report to stderr; stream via the
            # 5.1-safe helper so a clean report (exit 0) is not misread as a failure on PS 5.1.
            $denyExit = Invoke-GateNativeStream -FilePath "cargo-deny" -ArgumentList @("check")
            if ($denyExit -ne 0) {
                throw "LINT_GATE_FAILED: workspace=$($ws.Name) step=cargo-deny exit_code=$denyExit"
            }
        } elseif ($ws.DenyConfig) {
            if (-not (Test-Path -LiteralPath $ws.DenyConfig -PathType Leaf)) {
                throw "LINT_GATE_FAILED: workspace=$($ws.Name) mapped cargo-deny policy not found at $($ws.DenyConfig)"
            }
            $wsManifest = Join-Path $ws.Root "Cargo.toml"
            Write-Host "== cargo-deny: $($ws.Name) (external policy $($ws.DenyConfig)) =="
            # #1809: stream via the 5.1-safe helper (cargo-deny reports on stderr).
            $denyExit = Invoke-GateNativeStream -FilePath "cargo-deny" -ArgumentList @("--manifest-path", $wsManifest, "--config", $ws.DenyConfig, "check")
            if ($denyExit -ne 0) {
                throw "LINT_GATE_FAILED: workspace=$($ws.Name) step=cargo-deny exit_code=$denyExit"
            }
        } else {
            throw "LINT_GATE_FAILED: workspace=$($ws.Name) has no in-tree deny.toml and no mapped policy file. Add scripts/$($ws.Name.ToLower())-deny.toml and wire it into `$workspaces. Refusing to skip cargo-deny silently (#1517)."
        }
    } finally {
        Pop-Location
    }
}

Write-Host "LINT_GATE_PASSED"
