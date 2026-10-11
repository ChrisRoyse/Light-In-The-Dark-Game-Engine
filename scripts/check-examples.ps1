#requires -Version 5.1
<#
.SYNOPSIS
    Compile-check every example target in the GameEditor workspace.

.DESCRIPTION
    The test gate (test-rust-package.ps1) runs `cargo test --tests`, which deliberately does
    NOT build the ~230 FSV example binaries - that is the single biggest per-gate compile-time
    saving (#1497). Examples therefore need an out-of-band compile check so they cannot silently
    rot. This runs `cargo check --examples` for the crates that own examples. It is a CHECK
    (no codegen/link), so it is fast and never runs the examples.

    Fail-loud: any example that stops compiling makes this exit non-zero with cargo's error.
    Run it before a release, after touching engine/editor public API, or on a schedule.

.PARAMETER Package
    Restrict to one package (e.g. litd-engine). Default: all example-owning packages.

.PARAMETER TargetDir
    Optional shared target dir (reuse a warm tree to make the check fast).

.PARAMETER ChangedOnly
    Keep the declaration-coverage audit, but compile examples only when the pushed
    change set contains Rust/Cargo/Calyx input that can affect them. This is the
    routine pre-push fast path; direct invocation without this switch remains the
    exhaustive compile gate.

.PARAMETER BaseRef
    Git ref used with -ChangedOnly to determine the pushed change set. Defaults to
    origin/main and fails loud if the ref cannot be read.

.PARAMETER GoldenOnly
    Compile only the trust-critical golden example list while still auditing declaration
    coverage for every example source. The default remains exhaustive.

.PARAMETER DeclarationOnly
    Audit every example declaration but do not compile examples. This is reserved for the
    parent gate's explicit comment-only source path; direct callers must still opt into
    ChangedOnly so the changed-set decision is visible.

.EXAMPLE
    pwsh -File scripts\check-examples.ps1
.EXAMPLE
    pwsh -File scripts\check-examples.ps1 -Package litd-engine
#>
[CmdletBinding()]
param(
    [string]$Package,
    [string]$TargetDir,
    [switch]$ChangedOnly,
    [string]$BaseRef = "origin/main",
    [switch]$GoldenOnly,
    [switch]$DeclarationOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $repoRoot "Cargo.toml"

if ($DeclarationOnly -and -not $ChangedOnly) {
    throw "CHECK_EXAMPLES_INVALID: DeclarationOnly requires ChangedOnly"
}

$compileExamples = $true
$changedPaths = @()
if ($ChangedOnly) {
    Push-Location -LiteralPath $repoRoot
    try {
    $changedPaths = @(git diff --name-only --diff-filter=ACDMRTUXB "$BaseRef...HEAD")
        $gitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($gitCode -ne 0) {
        throw "CHECK_EXAMPLES_FAILED: could not read changed paths for base ref '$BaseRef' (exit_code=$gitCode)"
    }
    # Example compilation is affected by package Rust/Cargo input, workspace dependency
    # resolution, and the vendored Calyx workspace. Docs/scripts-only changes cannot alter
    # an example binary, but declaration coverage below remains mandatory.
    $compileExamples = @($changedPaths | Where-Object {
        $_ -match '^(Calyx/|crates/|Cargo\.toml$|Cargo\.lock$)'
    }).Count -gt 0
    Write-Host "CHECK_EXAMPLES_CHANGED_ONLY base_ref=$BaseRef changed_paths=$($changedPaths.Count) compile_required=$compileExamples"
}

if ($DeclarationOnly) {
    $compileExamples = $false
    Write-Host "CHECK_EXAMPLES_DECLARATION_ONLY compile_required=false reason=parent-gate-comment-only-rust-change"
}

$goldenExamples = @{
    "litd-engine" = @(
        "issue1112_physics_determinism_fsv", "issue1115_calyx_persistence_fsv",
        "issue1110_rapier_physics_fsv", "ecs_fsv", "issue1047_skilled_rewards_fsv",
        "issue1063_infernal_deeps_fsv", "issue1070_infernal_attrition_fsv",
        "issue1061_undeath_fsv"
    )
    "litd-editor" = @(
        "issue1467_surface_parity_readback", "issue870_scene_composition_fsv",
        "issue872_hud_ui_builder_fsv", "editor_rpg_map_runtime_issue1445_fsv",
        "mcp_world_tools_fsv", "mcp_object_tools_fsv"
    )
    "litd-game" = @(
        "game_positional_audio_fsv", "issue1460_frame_loop_fsv",
        "issue1891_package_map_rts_fsv"
    )
}

# Packages that actually own example targets (crates/<pkg>/examples/*.rs).
$examplePackages = @("litd-engine", "litd-editor", "litd-game")
if ($Package) { $examplePackages = @($Package) }
$compilePackages = @($examplePackages)

if ($ChangedOnly -and $compileExamples -and -not $Package) {
    $fullWorkspaceInput = @($changedPaths | Where-Object {
        $_ -match '^(Calyx/|Cargo\.toml$|Cargo\.lock$)'
    }).Count -gt 0
    if ($fullWorkspaceInput) {
        Write-Host "CHECK_EXAMPLES_PACKAGE_SELECTION mode=full-workspace reason=root-manifest-lock-or-Calyx-input compile_packages=$($compilePackages -join ',')"
    } else {
        $changedDirectories = @(
            $changedPaths |
                Where-Object { $_ -match '^crates/([^/]+)/' } |
                ForEach-Object { [regex]::Match($_, '^crates/([^/]+)/').Groups[1].Value } |
                Sort-Object -Unique
        )
        if ($changedDirectories.Count -eq 0) {
            throw "CHECK_EXAMPLES_FAILED: compile was required but no changed workspace package could be derived"
        }

        Push-Location -LiteralPath $repoRoot
        try {
            $metadataJson = & cargo metadata --format-version 1 --no-deps --manifest-path $manifest
            $metadataCode = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        if ($metadataCode -ne 0) {
            throw "CHECK_EXAMPLES_FAILED: dependency-closure metadata lookup failed exit_code=$metadataCode"
        }
        $metadata = $metadataJson | ConvertFrom-Json
        $memberIds = @($metadata.workspace_members | ForEach-Object { [string]$_ })
        $workspacePackages = @($metadata.packages | Where-Object { $memberIds -contains [string]$_.id })
        $workspaceNames = @($workspacePackages | ForEach-Object { [string]$_.name })
        $directPackages = @($changedDirectories | Where-Object { $workspaceNames -contains $_ })
        $unknownPackages = @($changedDirectories | Where-Object { $workspaceNames -notcontains $_ })
        if ($unknownPackages.Count -gt 0) {
            throw "CHECK_EXAMPLES_FAILED: changed crate directory is not a workspace package: $($unknownPackages -join ', ')"
        }

        $reverse = @{}
        foreach ($workspacePackage in $workspacePackages) {
            $workspaceName = [string]$workspacePackage.name
            foreach ($dependency in @($workspacePackage.dependencies)) {
                $dependencyName = [string]$dependency.name
                if (-not $reverse.ContainsKey($dependencyName)) {
                    $reverse[$dependencyName] = New-Object System.Collections.Generic.List[string]
                }
                $reverse[$dependencyName].Add($workspaceName)
            }
        }
        $selected = New-Object System.Collections.Generic.HashSet[string]
        $queue = New-Object System.Collections.Generic.Queue[string]
        foreach ($directPackage in $directPackages) {
            [void]$selected.Add($directPackage)
            $queue.Enqueue($directPackage)
        }
        while ($queue.Count -gt 0) {
            $dependency = $queue.Dequeue()
            foreach ($dependent in @($reverse[$dependency])) {
                if ($selected.Add($dependent)) {
                    $queue.Enqueue($dependent)
                }
            }
        }
        $compilePackages = @($examplePackages | Where-Object { $selected.Contains($_) })
        if ($compilePackages.Count -eq 0) {
            Write-Host "CHECK_EXAMPLES_PACKAGE_SELECTION changed=$($directPackages -join ',') reverse_closure=$($selected.Count) compile_packages=none reason=no-example-owner"
        } else {
            Write-Host "CHECK_EXAMPLES_PACKAGE_SELECTION changed=$($directPackages -join ',') reverse_closure=$($selected.Count) compile_packages=$($compilePackages -join ',')"
        }
    }
}

$failed = @()
foreach ($pkg in $examplePackages) {
    $exDir = Join-Path $repoRoot "crates\$pkg\examples"
    if (-not (Test-Path -LiteralPath $exDir -PathType Container)) {
        Write-Host "skip $pkg (no examples/ dir)"
        continue
    }
    $exampleFiles = @(Get-ChildItem -LiteralPath $exDir -Filter *.rs -File -ErrorAction SilentlyContinue)
    $count = $exampleFiles.Count

    # Path-included support modules live under examples/fixtures/ (or multi-file
    # example subdirs). Get-ChildItem -File (non-recursive) only audits top-level
    # entry-point .rs files, so shared helpers are not false-flagged as ungated
    # example targets (#1864).
    # Coverage lint (#1497): every example file MUST be declared as an [[example]] block that is
    # gated behind the `fsv` feature. With autoexamples=false a file without a block is an
    # invisible target (never compiled, silently rots); a block without required-features=["fsv"]
    # would leak back into the routine test gate. Fail loud on either drift.
    $pkgManifest = Join-Path $repoRoot "crates\$pkg\Cargo.toml"
    $manifestText = Get-Content -LiteralPath $pkgManifest -Raw
    $gatedExamples = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($block in ($manifestText -split '(?m)^\[\[example\]\]')) {
        $nameMatch = [regex]::Match($block, '(?m)^\s*name\s*=\s*"([^"]+)"')
        if (-not $nameMatch.Success) { continue }
        if ($block -match 'required-features\s*=\s*\[\s*"fsv"\s*\]') {
            [void]$gatedExamples.Add($nameMatch.Groups[1].Value)
        }
    }
    $ungated = @($exampleFiles | Where-Object { -not $gatedExamples.Contains($_.BaseName) } | ForEach-Object { $_.Name })
    if ($ungated.Count -gt 0) {
        throw "EXAMPLE_NOT_GATED ($pkg): these example files lack an [[example]] block with required-features=[""fsv""] in crates/$pkg/Cargo.toml: $($ungated -join ', ')"
    }

    if ($compileExamples -and ($compilePackages -contains $pkg)) {
        if ($GoldenOnly) {
            $compileNames = @($goldenExamples[$pkg])
            Write-Host "== cargo check --features fsv --examples -p $pkg  (golden=$($compileNames.Count), declarations=$count) =="
            $cargoArgs = @("check", "--features", "fsv", "--manifest-path", $manifest, "-p", $pkg)
            foreach ($exampleName in $compileNames) {
                $cargoArgs += @("--example", $exampleName)
            }
            if ($TargetDir) { $cargoArgs += @("--target-dir", $TargetDir) }
            & cargo @cargoArgs
            if ($LASTEXITCODE -ne 0) { $failed += "$pkg/(golden targets)" }
        } else {
            Write-Host "== cargo check --features fsv --examples -p $pkg  ($count example files, all gated) =="
            $cargoArgs = @("check", "--features", "fsv", "--examples", "--manifest-path", $manifest, "-p", $pkg)
            if ($TargetDir) { $cargoArgs += @("--target-dir", $TargetDir) }
            & cargo @cargoArgs
            if ($LASTEXITCODE -ne 0) { $failed += $pkg }
        }
    } elseif ($DeclarationOnly) {
        Write-Host "CHECK_EXAMPLES_COMPILE_SKIPPED package=$pkg reason=declaration-only-comment-rust-change"
    } elseif ($compileExamples) {
        Write-Host "CHECK_EXAMPLES_COMPILE_SKIPPED package=$pkg reason=package-outside-changed-reverse-dependency-closure"
    } else {
        Write-Host "CHECK_EXAMPLES_COMPILE_SKIPPED package=$pkg reason=changed-set-has-no-Rust-Cargo-Calyx-input"
    }
}

if ($failed.Count -gt 0) {
    throw "CHECK_EXAMPLES_FAILED: examples failed to compile in: $($failed -join ', ')"
}
if ($compileExamples) {
    if ($GoldenOnly) {
        Write-Host "check-examples: golden example targets compile; all declarations audited."
    } else {
        Write-Host "check-examples: all example targets compile."
    }
} else {
    $reason = if ($DeclarationOnly) { "comment-only-rust-change" } else { "changed-set-policy" }
    Write-Host "check-examples: declaration coverage passed; compilation was not required reason=$reason."
}
