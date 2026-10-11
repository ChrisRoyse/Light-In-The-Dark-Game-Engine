#requires -Version 5.1
<#
.SYNOPSIS
    Real correctness gate for `git push` (#1553).

.DESCRIPTION
    There is no CI at the GameEditor root (permanent decision #284/#447/#689); local
    hooks are the ONLY enforcement. The pre-commit hook runs check-format.ps1 -FormatOnly
    (rustfmt + ratchets, NO compile) so commits stay fast. Push is the publish boundary
    where correctness must be PROVEN, so this gate runs on pre-push and fails loud with the
    exact remediation command:

      1. cargo check --lib --bins for the changed-package reverse dependency closure
         (or the full workspace when manifests/Calyx change; compile -FormatOnly deliberately skips)
      2. the test gate (scripts/test-gameeditor.ps1) for every workspace package
      3. scripts/check-examples.ps1               (the ~230 FSV example binaries; cf. #1527)
      4. scripts/lint.ps1 -Deny                   (clippy -D warnings + cargo-deny; #1545)

    Steps 1-3 build into the STABLE warm gate cache target\fsv\test-gameeditor\cargo-target
    (AGENTS.md; #1498) so a warm push is incremental, not a cold rebuild. This gate NEVER
    wipes that cache. cargo already serializes concurrent runs on the target's .cargo-lock,
    and the test gate scopes its mutex to that same tree, so this gate does not race other
    cargo runs against the shared cache (#1536).

    The changed-package closure is derived from `cargo metadata` and includes reverse dependents,
    so a public API change still checks every affected consumer. Explicit production checks cover
    the engine/game targets excluded from the package invocation; `-FullTests` restores the
    exhaustive workspace/test path. A pre-push hook is bypassable with `git push --no-verify`. That is by design (#1553): a
    deliberate, visible bypass is categorically different from no gate at all.

    ROOT CAUSE, no fallbacks: every sub-gate failure exits non-zero and prints exactly what
    broke and the command to reproduce/fix it. Nothing is skipped silently.
#>
[CmdletBinding()]
param(
    # Comma/space-separated override of the workspace packages the test gate runs.
    # Default: every GameEditor workspace member, discovered from `cargo metadata`.
    [string[]]$Packages,

    # Optional override of the warm cargo target dir this gate builds into. Default (empty):
    # the STABLE shared warm cache <repoRoot>\target\fsv\test-gameeditor\cargo-target (#1498).
    # run-main-health-gate.ps1 (#1567) runs this same gate from a throwaway origin/main worktree
    # and passes the MAIN repo's shared warm cache here, so a main-health check is INCREMENTAL
    # against the real warm cache rather than a cold rebuild inside the worktree. Must be an
    # absolute path (a relative value is resolved against the current directory). This gate NEVER
    # wipes the target dir either way -- it is a cache to RETAIN.
    [string]$TargetDir,

    # Run every package's exhaustive cfg(test) suite instead of the routine production golden
    # targets. This is intentionally opt-in: the push loop should stay short and deterministic.
    [switch]$FullTests,

    # For docs/scripts-only pushes, run the static/script integrity subset and avoid compiling
    # the Rust graph. Any Rust/Cargo/Calyx input automatically selects the production gate;
    # -FullTests always overrides this policy.
    [switch]$ChangedOnly,
    [string]$BaseRef = "origin/main",

    # Production golden targets are the short edit-cycle proof. Keep their
    # default budget tight; slower hosts/cold caches may override explicitly.
    [ValidateRange(1, 3600)]
    [int]$GoldenBudgetSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $repoRoot "Cargo.toml"

# The STABLE warm gate cache (AGENTS.md; #1498). cargo check, the test gate, and
# check-examples all build into this ONE tree so a warm push is incremental. It is a cache to
# RETAIN, never wiped here.
$warmTarget = if ($TargetDir) {
    [System.IO.Path]::GetFullPath($TargetDir)
} else {
    Join-Path $repoRoot "target\fsv\test-gameeditor\cargo-target"
}

# Run every child gate script under pwsh 7 when available, else the current host. NOT a hardcoded
# `powershell` (Windows PowerShell 5.1): test-gameeditor.ps1 -> CalyxCanonicalPaths.ps1 uses
# [System.IO.Path]::GetRelativePath, which exists only in pwsh 7 (.NET), not in 5.1
# (.NET Framework). Preferring pwsh mirrors the pre-push hook's own interpreter selection, so the
# test gate gets a real signal instead of a spurious MethodNotFound even if the gate itself was
# launched under 5.1.
$pwshCmd = Get-Command pwsh -ErrorAction SilentlyContinue
$psHost = if ($pwshCmd) { $pwshCmd.Source } else { (Get-Process -Id $PID).Path }
if (-not $psHost) {
    throw "PRE_PUSH_GATE_FAILED: could not resolve a PowerShell host executable for child gate scripts"
}
$cargo = (Get-Command cargo -ErrorAction Stop).Source
$goldenOutputRoot = Join-Path $repoRoot "target\fsv\test-gameeditor\pre-push-golden-output"
New-Item -ItemType Directory -Force -Path $goldenOutputRoot | Out-Null
Write-Host "PRE_PUSH_GATE_CHILD_HOST psHost=$psHost"

$overallStopwatch = [System.Diagnostics.Stopwatch]::StartNew()

function Write-GateFailure {
    param(
        [Parameter(Mandatory = $true)][string]$Step,
        [Parameter(Mandatory = $true)][string]$Fix
    )
    Write-Host ""
    Write-Host "================================================================"
    Write-Host "PRE_PUSH_GATE_FAILED step=$Step"
    Write-Host "  Push blocked: the workspace did not pass this correctness gate."
    Write-Host "  Reproduce / fix:"
    foreach ($line in @($Fix -split "`n")) {
        Write-Host "    $line"
    }
    Write-Host ""
    Write-Host "  Bypass (deliberate, visible; use only when you know why): git push --no-verify"
    Write-Host "================================================================"
}

function Invoke-BoundedGoldenCargo {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssfffZ")
    $safeLabel = $Label -replace "[^A-Za-z0-9_.-]", "_"
    $stdoutPath = Join-Path $goldenOutputRoot "$safeLabel-$stamp.stdout.log"
    $stderrPath = Join-Path $goldenOutputRoot "$safeLabel-$stamp.stderr.log"
    $process = $null
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $process = Start-Process -FilePath $cargo -ArgumentList $Arguments -WorkingDirectory $repoRoot `
            -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath `
            -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit([int]([Math]::Min([long]$GoldenBudgetSeconds * 1000, [int]::MaxValue)))) {
            & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
            $process.WaitForExit()
            $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
            $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
            Write-Host $stdout
            Write-Host $stderr
            throw "PRE_PUSH_GATE_FAILED: step=$Label timeout_seconds=$GoldenBudgetSeconds pid=$($process.Id) stdout=$stdoutPath stderr=$stderrPath"
        }
        $sw.Stop()
        $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -LiteralPath $stdoutPath -Raw } else { "" }
        $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -LiteralPath $stderrPath -Raw } else { "" }
        Write-Host $stdout
        Write-Host $stderr
        return [pscustomobject]@{
            ExitCode = $process.ExitCode
            Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 2)
            StdoutPath = $stdoutPath
            StderrPath = $stderrPath
        }
    } finally {
        if ($process -and -not $process.HasExited) {
            & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
        }
        if ($process -and $process.HasExited -and $process.ExitCode -eq 0) {
            Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
        }
    }
}

function Invoke-CargoCheckStep {
    param(
        [string[]]$PackageSelection,
        [switch]$UsePackageSelection,
        [bool]$RunGameProduction = $true
    )

    Push-Location -LiteralPath $repoRoot
    try {
        $cargoArgs = @("check", "--lib", "--bins", "--manifest-path", $manifest, "--target-dir", $warmTarget)
        if ($UsePackageSelection -and $PackageSelection.Count -eq 0) {
            Write-Host "PRE_PUSH_GATE_START step=cargo-check command=skipped-no-unrelated-package-targets"
            $workspaceCode = 0
        } elseif ($PackageSelection -and $PackageSelection.Count -gt 0) {
            foreach ($package in $PackageSelection) {
                $cargoArgs += @("--package", $package)
            }
            Write-Host "PRE_PUSH_GATE_START step=cargo-check command=cargo $($cargoArgs -join ' ')"
        } else {
            # #1895: litd-game and litd-engine test targets rebuild the full replay/engine graph
            # while adding no routine coverage beyond the explicit production checks below. The
            # routine golden package is also checked by its explicit cargo run. Keep the remaining
            # workspace targets here, then compile the real production targets separately.
            # Production-only selection avoids compiling test and example targets here. Those
            # targets have separate explicit gates below, so this check must not duplicate them.
            $cargoArgs = @("check", "--workspace", "--lib", "--bins", "--exclude", "litd-game", "--exclude", "litd-engine", "--exclude", "litd-engine-routine-golden", "--manifest-path", $manifest, "--target-dir", $warmTarget)
            Write-Host "PRE_PUSH_GATE_START step=cargo-check command=cargo $($cargoArgs -join ' ')"
        }
        if (-not ($UsePackageSelection -and $PackageSelection.Count -eq 0)) {
            & cargo @cargoArgs
            $workspaceCode = $LASTEXITCODE
        }
        if ($workspaceCode -eq 0 -and $RunGameProduction) {
            Write-Host "PRE_PUSH_GATE_START step=cargo-check-litd-game-production command=cargo check -p litd-game --bin litd-game --target-dir $warmTarget"
            & cargo check -p litd-game --bin litd-game --manifest-path $manifest --target-dir $warmTarget
            $productionCode = $LASTEXITCODE
        } elseif ($workspaceCode -eq 0) {
            Write-Host "PRE_PUSH_GATE_START step=cargo-check-litd-game-production command=skipped-package-unaffected"
            $productionCode = 0
        } else {
            $productionCode = 0
        }
    } finally {
        Pop-Location
    }
    if ($workspaceCode -ne 0) {
        Write-GateFailure -Step "cargo-check-workspace" -Fix @"
cargo check --workspace --lib --bins --exclude litd-game --exclude litd-engine --exclude litd-engine-routine-golden
(cargo named the failing crate/file above with exit_code=$workspaceCode)
"@
        throw "PRE_PUSH_GATE_FAILED: step=cargo-check-workspace exit_code=$workspaceCode"
    }
    if ($productionCode -ne 0) {
        Write-GateFailure -Step "cargo-check-litd-game-production" -Fix @"
cargo check -p litd-game --bin litd-game
(cargo named the failing crate/file above with exit_code=$productionCode)
"@
        throw "PRE_PUSH_GATE_FAILED: step=cargo-check-litd-game-production exit_code=$productionCode"
    }
    Write-Host "PRE_PUSH_GATE_OK step=cargo-check-workspace"
    if ($RunGameProduction) {
        Write-Host "PRE_PUSH_GATE_OK step=cargo-check-litd-game-production"
    } else {
        Write-Host "PRE_PUSH_GATE_OK step=cargo-check-litd-game-production selection=skipped-package-unaffected"
    }
}

function Get-ChangedPackageSelection {
    param(
        [Parameter(Mandatory = $true)][object]$ChangeSet
    )

    if (-not $ChangeSet.CompileRelevant -or $FullTests) {
        return @()
    }
    if (@($ChangeSet.Paths | Where-Object { $_ -match '^(Calyx/|Cargo\.toml$|Cargo\.lock$)' }).Count -gt 0) {
        return @()
    }

    $changedDirectories = @(
        $ChangeSet.Paths |
            Where-Object { $_ -match '^crates/([^/]+)/' } |
            ForEach-Object { [regex]::Match($_, '^crates/([^/]+)/').Groups[1].Value } |
            Sort-Object -Unique
    )
    if ($changedDirectories.Count -eq 0) {
        return @()
    }

    Push-Location -LiteralPath $repoRoot
    try {
        $metadataJson = & cargo metadata --format-version 1 --no-deps --manifest-path $manifest
        $metadataCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($metadataCode -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: dependency-closure metadata lookup failed exit_code=$metadataCode"
    }
    $metadata = $metadataJson | ConvertFrom-Json
    $memberIds = @($metadata.workspace_members | ForEach-Object { [string]$_ })
    $packages = @($metadata.packages | Where-Object { $memberIds -contains [string]$_.id })
    $names = @($packages | ForEach-Object { [string]$_.name })
    $direct = @($changedDirectories | Where-Object { $names -contains $_ })
    $unknown = @($changedDirectories | Where-Object { $names -notcontains $_ })
    if ($unknown.Count -gt 0) {
        throw "PRE_PUSH_GATE_FAILED: changed crate directory is not a workspace package: $($unknown -join ',')"
    }

    # Select the changed packages and every workspace reverse dependent. This keeps the
    # compile check sound when a public dependency API changes, while avoiding unrelated
    # editor/game packages. Explicit production/golden gates below remain authoritative for
    # the packages intentionally excluded from this compile invocation.
    $reverse = @{}
    foreach ($package in $packages) {
        $packageName = [string]$package.name
        foreach ($dependency in @($package.dependencies)) {
            $dependencyName = [string]$dependency.name
            if (-not $reverse.ContainsKey($dependencyName)) {
                $reverse[$dependencyName] = New-Object System.Collections.Generic.List[string]
            }
            $reverse[$dependencyName].Add($packageName)
        }
    }
    $selected = New-Object System.Collections.Generic.HashSet[string]
    $queue = New-Object System.Collections.Generic.Queue[string]
    foreach ($package in $direct) {
        [void]$selected.Add($package)
        $queue.Enqueue($package)
    }
    while ($queue.Count -gt 0) {
        $dependency = $queue.Dequeue()
        # A package with no workspace reverse dependents is a normal leaf. PowerShell iterates
        # a missing hashtable value as one `$null` item; without this guard that null was added
        # to the package selector and emitted a bare `--package`, aborting the gate (#1895).
        if ($reverse.ContainsKey($dependency)) {
            foreach ($dependent in @($reverse[$dependency])) {
                if ($selected.Add($dependent)) {
                    $queue.Enqueue($dependent)
                }
            }
        }
    }

    # These targets have separate production checks immediately after this step. Avoid
    # compiling their duplicate lib/bin target here, but keep any other reverse dependents.
    $excluded = @(
        "litd-game",
        "litd-engine",
        "litd-editor-routine-golden",
        "litd-engine-routine-golden"
    )
    $result = @($selected | Where-Object { $excluded -notcontains $_ } | Sort-Object)
    Write-Host "PRE_PUSH_GATE_PACKAGE_SELECTION changed=$($direct -join ',') reverse_closure=$($selected.Count) compile_packages=$($result -join ',') explicit_production=litd-game,litd-engine-routine-golden"
    return $result
}

function Invoke-CheckExamplesStep {
    $script = Join-Path $PSScriptRoot "check-examples.ps1"
    if (-not (Test-Path -LiteralPath $script -PathType Leaf)) {
        throw "PRE_PUSH_GATE_FAILED: step=check-examples script_not_found=$script"
    }
    $exampleArgs = @("-TargetDir", $warmTarget)
    if (-not $FullTests) {
        $exampleArgs += @("-ChangedOnly", "-BaseRef", $BaseRef, "-GoldenOnly")
    }
    Write-Host "PRE_PUSH_GATE_START step=check-examples command=$script $($exampleArgs -join ' ')"
    & $psHost -NoProfile -ExecutionPolicy Bypass -File $script @exampleArgs
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-GateFailure -Step "check-examples" -Fix @"
pwsh -File scripts\check-examples.ps1$(if (-not $FullTests) { " -ChangedOnly -BaseRef $BaseRef -GoldenOnly" })
(an FSV example target stopped compiling; exit_code=$code)
"@
        throw "PRE_PUSH_GATE_FAILED: step=check-examples exit_code=$code"
    }
    Write-Host "PRE_PUSH_GATE_OK step=check-examples"
}

function Invoke-EngineGoldenStep {
    Write-Host "PRE_PUSH_GATE_START step=engine-golden command=cargo run --locked -p litd-engine-routine-golden --target-dir $warmTarget"
    $result = Invoke-BoundedGoldenCargo -Label "engine-golden" -Arguments @(
        "run", "--locked", "-p", "litd-engine-routine-golden", "--manifest-path", $manifest, "--target-dir", $warmTarget
    )
    $code = $result.ExitCode
    $seconds = $result.Seconds
    Write-Host "PRE_PUSH_GATE_GOLDEN_TIMING step=engine-golden seconds=$seconds budget=$GoldenBudgetSeconds"
    if ($code -ne 0) {
        Write-GateFailure -Step "engine-golden" -Fix @"
cargo run --locked -p litd-engine-routine-golden --target-dir $warmTarget
(the production-profile engine golden path failed; inspect the structured ENGINE_GOLDEN_* output above)
"@
        throw "PRE_PUSH_GATE_FAILED: step=engine-golden exit_code=$code"
    }
    if ($seconds -gt $GoldenBudgetSeconds) {
        Write-GateFailure -Step "engine-golden" -Fix @"
cargo run --locked -p litd-engine-routine-golden --target-dir $warmTarget
(the production-profile engine golden path exceeded ${GoldenBudgetSeconds}s: elapsed=${seconds}s; inspect compilation or golden-path scope)
"@
        throw "PRE_PUSH_GATE_FAILED: step=engine-golden budget_exceeded elapsed_seconds=$seconds budget_seconds=$GoldenBudgetSeconds"
    }
    Write-Host "PRE_PUSH_GATE_OK step=engine-golden"
}

function Invoke-EditorGoldenStep {
    Write-Host "PRE_PUSH_GATE_START step=editor-golden command=cargo test --locked -p litd-editor-routine-golden --test routine_golden_path --target-dir $warmTarget"
    $result = Invoke-BoundedGoldenCargo -Label "editor-golden" -Arguments @(
        "test", "--locked", "-p", "litd-editor-routine-golden", "--test", "routine_golden_path", "--manifest-path", $manifest, "--target-dir", $warmTarget
    )
    $code = $result.ExitCode
    $seconds = $result.Seconds
    Write-Host "PRE_PUSH_GATE_GOLDEN_TIMING step=editor-golden seconds=$seconds budget=$GoldenBudgetSeconds"
    if ($code -ne 0) {
        Write-GateFailure -Step "editor-golden" -Fix @"
cargo test --locked -p litd-editor-routine-golden --test routine_golden_path --target-dir $warmTarget
(the production-profile editor golden path failed; inspect the test output above)
"@
        throw "PRE_PUSH_GATE_FAILED: step=editor-golden exit_code=$code"
    }
    if ($seconds -gt $GoldenBudgetSeconds) {
        Write-GateFailure -Step "editor-golden" -Fix @"
cargo test --locked -p litd-editor-routine-golden --test routine_golden_path --target-dir $warmTarget
(the production-profile editor golden path exceeded ${GoldenBudgetSeconds}s: elapsed=${seconds}s; inspect compilation or golden-path scope)
"@
        throw "PRE_PUSH_GATE_FAILED: step=editor-golden budget_exceeded elapsed_seconds=$seconds budget_seconds=$GoldenBudgetSeconds"
    }
    Write-Host "PRE_PUSH_GATE_OK step=editor-golden"
}

function Invoke-TestGateStep {
    param([Parameter(Mandatory = $true)][string]$Package)

    $script = Join-Path $PSScriptRoot "test-gameeditor.ps1"
    if (-not (Test-Path -LiteralPath $script -PathType Leaf)) {
        throw "PRE_PUSH_GATE_FAILED: step=test-gate script_not_found=$script"
    }
    # Routine pushes are production-golden checks. A package test that hangs or
    # accidentally selects the monolithic cfg(test) graph must fail loudly and
    # return control to the edit loop; exhaustive validation remains explicit
    # via -FullTests with the historical 900-second budget.
    $testTimeoutSeconds = if ($FullTests) { 900 } else { 120 }
    Write-Host "PRE_PUSH_GATE_START step=test-gate:$Package command=$script -Package $Package -TargetDir $warmTarget -TestTimeoutSeconds $testTimeoutSeconds"
    $testArgs = @("-Package", $Package, "-TargetDir", $warmTarget, "-TestTimeoutSeconds", $testTimeoutSeconds)
    & $psHost -NoProfile -ExecutionPolicy Bypass -File $script @testArgs
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-GateFailure -Step "test-gate:$Package" -Fix @"
pwsh -File scripts\test-gameeditor.ps1 -Package $Package
(a test failed or a test binary did not build; exit_code=$code -- see the TEST_GATE_REPORT_READBACK path above for the failing test)
"@
        throw "PRE_PUSH_GATE_FAILED: step=test-gate:$Package exit_code=$code"
    }
    Write-Host "PRE_PUSH_GATE_OK step=test-gate:$Package"
}

function Invoke-LintStep {
    param(
        [string]$LintTargetDir,
        [string[]]$LintPackages
    )

    $script = Join-Path $PSScriptRoot "lint.ps1"
    if (-not (Test-Path -LiteralPath $script -PathType Leaf)) {
        throw "PRE_PUSH_GATE_FAILED: step=lint script_not_found=$script"
    }
    $lintArgs = @("-Deny")
    if ($LintTargetDir) {
        $lintArgs += @("-TargetDir", $LintTargetDir)
    }
    if ($LintPackages -and $LintPackages.Count -gt 0) {
        # Serialize the array as one comma-delimited token across the child PowerShell process.
        # Passing separate tokens makes the second package look like an unexpected positional
        # argument to lint.ps1's [string[]] parameter.
        $lintArgs += @("-GameEditorPackages", ($LintPackages -join ","))
    }
    if ($FullTests) {
        $lintArgs += "-AllTargets"
    }
    Write-Host "PRE_PUSH_GATE_START step=lint command=$script $($lintArgs -join ' ')"
    & $psHost -NoProfile -ExecutionPolicy Bypass -File $script @lintArgs
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-GateFailure -Step "lint" -Fix @"
        pwsh -File scripts\lint.ps1 -Deny$(if ($LintTargetDir) { " -TargetDir $LintTargetDir" })$(if ($LintPackages) { " -GameEditorPackages $($LintPackages -join ',')" })$(if ($FullTests) { ' -AllTargets' })
(clippy -D warnings or cargo-deny reported a violation; exit_code=$code)
"@
        throw "PRE_PUSH_GATE_FAILED: step=lint exit_code=$code"
    }
    Write-Host "PRE_PUSH_GATE_OK step=lint"
}

function Get-CompileRelevantChangeSet {
    Push-Location -LiteralPath $repoRoot
    try {
        $paths = @(git diff --name-only --diff-filter=ACDMRTUXB "$BaseRef...HEAD")
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($code -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: could not read changed paths for base ref '$BaseRef' (exit_code=$code)"
    }
    $deletedTestOnly = $false
    if (@($paths).Count -gt 0) {
        $changeRecords = @(git diff --name-status --diff-filter=ACDMRTUXB "$BaseRef...HEAD")
        $recordCode = $LASTEXITCODE
        if ($recordCode -ne 0) {
            throw "PRE_PUSH_GATE_FAILED: could not read changed-status records for '$BaseRef' (exit_code=$recordCode)"
        }
        $deletedTestOnly = @($changeRecords).Count -gt 0 -and @($changeRecords | Where-Object {
            $_ -notmatch "^D\t(?:.*[/\\])?(?:[^/\\]+_tests\.rs|tests[/\\].+)$"
        }).Count -eq 0
    }
    $compileRelevant = @($paths | Where-Object {
        $_ -match '^(Calyx/|crates/|Cargo\.toml$|Cargo\.lock$)'
    }).Count -gt 0
    if ($deletedTestOnly) {
        $compileRelevant = $false
    }
    $mode = if ($deletedTestOnly) {
        "deleted-test-only"
    } elseif (-not $compileRelevant) {
        "static-only"
    } elseif (@($paths).Count -gt 0 -and @($paths | Where-Object { $_ -notmatch '^crates/litd-game-cli/' }).Count -eq 0) {
        "game-cli"
    } else {
        "full"
    }
    Write-Host "PRE_PUSH_GATE_CHANGED_ONLY base_ref=$BaseRef changed_paths=$($paths.Count) compile_required=$compileRelevant mode=$mode"
    [pscustomobject]@{ Paths = $paths; CompileRelevant = $compileRelevant; Mode = $mode; DeletedTestOnly = $deletedTestOnly }
}

function Test-CommentOnlyRustChangeSet {
    param([Parameter(Mandatory = $true)][object]$ChangeSet)

    # A Rust line comment is non-code; a block comment is deliberately not classified here
    # because a malformed delimiter can change the token stream. Doc comments remain included
    # because Rust turns them into doc attributes and rustdoc is not part of this gate.
    $rustPaths = @($ChangeSet.Paths | Where-Object { $_ -match '\.rs$' })
    if ($rustPaths.Count -eq 0 -or $rustPaths.Count -ne @($ChangeSet.Paths).Count) {
        return $false
    }
    $diffArgs = @("diff", "--unified=0", "--no-ext-diff", "$BaseRef...HEAD", "--") + $rustPaths
    Push-Location -LiteralPath $repoRoot
    try {
        $diff = @(git @diffArgs)
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($code -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: could not inspect Rust diff for comment-only classification (exit_code=$code)"
    }
    $contentLines = @($diff | Where-Object {
        $_.Length -gt 0 -and (($_[0] -eq "+") -or ($_[0] -eq "-")) -and
        -not $_.StartsWith("+++") -and -not $_.StartsWith("---")
    })
    if ($contentLines.Count -eq 0) {
        return $false
    }
    foreach ($line in $contentLines) {
        $body = $line.Substring(1).TrimStart()
        if ($body.Length -eq 0 -or $body.StartsWith("//")) {
            continue
        }
        return $false
    }
    return $true
}

function Invoke-GameCliFastPath {
    Write-Host "PRE_PUSH_GATE_START step=targeted-game-cli mode=dependency-light-golden-path"
    Push-Location -LiteralPath $repoRoot
    try {
        Write-Host "PRE_PUSH_GATE_START step=targeted-game-cli:cargo-check command=cargo check --locked -p litd-game-cli --lib --target-dir $warmTarget"
        & cargo check --locked -p litd-game-cli --lib --manifest-path $manifest --target-dir $warmTarget
        if ($LASTEXITCODE -ne 0) {
            throw "PRE_PUSH_GATE_FAILED: targeted litd-game-cli cargo check exit_code=$LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    Invoke-TestGateStep -Package "litd-game-cli"
    $lintScript = Join-Path $PSScriptRoot "lint.ps1"
    Write-Host "PRE_PUSH_GATE_START step=targeted-game-cli:lint command=$lintScript -Deny -TargetDir $warmTarget -GameEditorPackages litd-game-cli"
    & $psHost -NoProfile -ExecutionPolicy Bypass -File $lintScript -Deny -TargetDir $warmTarget -GameEditorPackages litd-game-cli
    if ($LASTEXITCODE -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: targeted litd-game-cli lint exit_code=$LASTEXITCODE"
    }
    Write-Host "PRE_PUSH_GATE_OK step=targeted-game-cli"
}

function Invoke-StaticOnlyStep {
    param(
        [switch]$CommentOnlyRust,
        [switch]$DeletedTestOnly
    )

    $mode = if ($CommentOnlyRust) { "comment-only-rust" } elseif ($DeletedTestOnly) { "deleted-test-only" } else { "static-only" }
    Write-Host "PRE_PUSH_GATE_START step=static-only mode=$mode command=script integrity and declaration gates"
    $staticScripts = @(
        "check-ascii-gate-scripts.ps1",
        "check-windows-platform.ps1",
        "check-fsv-idioms.ps1",
        "check-doctrine-refs.ps1",
        "check-doc-refs.ps1"
    )
    foreach ($name in $staticScripts) {
        $script = Join-Path $PSScriptRoot $name
        if (-not (Test-Path -LiteralPath $script -PathType Leaf)) {
            throw "PRE_PUSH_GATE_FAILED: static script not found: $script"
        }
        Write-Host "PRE_PUSH_GATE_START step=static-only:$name command=$script"
        & $psHost -NoProfile -ExecutionPolicy Bypass -File $script
        if ($LASTEXITCODE -ne 0) {
            throw "PRE_PUSH_GATE_FAILED: step=static-only:$name exit_code=$LASTEXITCODE"
        }
    }
    $examples = Join-Path $PSScriptRoot "check-examples.ps1"
    $exampleArgs = @("-ChangedOnly", "-BaseRef", $BaseRef)
    if ($CommentOnlyRust -or $DeletedTestOnly) { $exampleArgs += "-DeclarationOnly" }
    & $psHost -NoProfile -ExecutionPolicy Bypass -File $examples @exampleArgs
    if ($LASTEXITCODE -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: step=static-only:check-examples exit_code=$LASTEXITCODE"
    }
    Write-Host "PRE_PUSH_GATE_OK step=static-only"
}

function Get-WorkspacePackages {
    Push-Location -LiteralPath $repoRoot
    try {
        $json = & cargo metadata --format-version 1 --no-deps --manifest-path $manifest
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($code -ne 0) {
        throw "PRE_PUSH_GATE_FAILED: step=package-discovery cargo-metadata exit_code=$code"
    }
    $meta = $json | ConvertFrom-Json
    $memberIds = @($meta.workspace_members | ForEach-Object { [string]$_ })
    $names = @(
        $meta.packages |
            Where-Object { $memberIds -contains [string]$_.id } |
            ForEach-Object { [string]$_.name } |
            Sort-Object -Unique
    )
    if ($names.Count -eq 0) {
        throw "PRE_PUSH_GATE_FAILED: step=package-discovery no_workspace_packages"
    }
    return $names
}

try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "PRE_PUSH_GATE_FAILED: cargo was not found on PATH"
    }
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw "PRE_PUSH_GATE_FAILED: workspace manifest not found at $manifest"
    }

    $changeSet = if ($ChangedOnly -and -not $FullTests) {
        Get-CompileRelevantChangeSet
    } else {
        [pscustomobject]@{ Paths = @(); CompileRelevant = $true; DeletedTestOnly = $false }
    }

    $commentOnlyRust = $false
    $deletedTestOnly = [bool]$changeSet.DeletedTestOnly
    if ($ChangedOnly -and -not $FullTests -and $changeSet.CompileRelevant) {
        if (Test-CommentOnlyRustChangeSet -ChangeSet $changeSet) {
            $commentOnlyRust = $true
            $changeSet.CompileRelevant = $false
            $changeSet.Mode = "comment-only-rust"
            Write-Host "PRE_PUSH_GATE_CHANGED_ONLY comment_only_rust=true compile_required=false reason=all-Rust-diff-lines-are-line-comments-or-blank"
        }
    }

    if ($ChangedOnly -and -not $FullTests -and $changeSet.Mode -eq "game-cli") {
        Invoke-GameCliFastPath
        $overallStopwatch.Stop()
        Write-Host ""
        Write-Host "PRE_PUSH_GATE_OK all_steps_passed=true mode=targeted-game-cli elapsed_seconds=$([math]::Round($overallStopwatch.Elapsed.TotalSeconds, 1))"
        exit 0
    }

    $effectivePackages = if ($Packages -and $Packages.Count -gt 0) {
        # Accept both space-separated (-Packages a b c) and comma-separated (-Packages a,b,c);
        # with `-File`, a comma-joined value arrives as one string, so split it defensively.
        @($Packages | ForEach-Object { $_ -split "," } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    } elseif ($ChangedOnly -and -not $FullTests -and -not $changeSet.CompileRelevant) {
        @()
    } else {
        @(Get-WorkspacePackages)
    }

    $changedPackageSelection = @()
    $changedDirectPackages = @()
    $changedPackageScope = @()
    $useChangedPackageSelection = $false
    if ($ChangedOnly -and -not $FullTests -and $changeSet.CompileRelevant) {
        $hasFullWorkspaceInput = @($changeSet.Paths | Where-Object {
            $_ -match '^(Calyx/|Cargo\.toml$|Cargo\.lock$)'
        }).Count -gt 0
        if ($hasFullWorkspaceInput) {
            Write-Host "PRE_PUSH_GATE_PACKAGE_SELECTION mode=full-workspace reason=root-manifest-lock-or-Calyx-input"
        } else {
            $changedDirectPackages = @(
                $changeSet.Paths |
                    Where-Object { $_ -match '^crates/([^/]+)/' } |
                    ForEach-Object { [regex]::Match($_, '^crates/([^/]+)/').Groups[1].Value } |
                    Sort-Object -Unique
            )
            $changedPackageSelection = @(Get-ChangedPackageSelection -ChangeSet $changeSet)
            $changedPackageScope = @($changedDirectPackages + $changedPackageSelection | Sort-Object -Unique)
            $useChangedPackageSelection = $true
        }
    }

    # #1895: litd-engine's production golden executable is run separately without its monolithic
    # cfg(test) graph; litd-game is production-verified by the workspace cargo check and
    # the manual replay FSV; its package test target recompiles the full engine graph for little
    # additional golden-path value. The litd-editor lib test target likewise rebuilds the full
    # editor/Calyx test graph and hit its 900-second gate timeout. The nominal editor MCP golden
    # targets also pull the ML-heavy calyx-surface graph (tokenizers/candle) and are not routine
    # feedback-loop checks. Production lib/bin compilation remains above; all these test targets
    # stay available via -FullTests.
    $testGateExcludedPackages = if ($FullTests) {
        # These are explicitly production-verified above; retaining them here avoids
        # compiling duplicate cfg(test) targets even in an exhaustive suite run.
        @("litd-game", "litd-engine-routine-golden")
    } else {
        @(
            "litd-game",
            "litd-editor",
            "litd-editor-mcp",
            "litd-editor-routine-golden",
            "litd-engine-routine-golden",
            "litd-engine"
        )
    }
    $testGatePackages = @($effectivePackages | Where-Object {
        -not $testGateExcludedPackages.Contains([string]$_)
    })
    if ($useChangedPackageSelection) {
        $testGatePackages = @($testGatePackages | Where-Object {
            $changedPackageScope -contains [string]$_
        })
    }

    Write-Host "PRE_PUSH_GATE_SOURCE_OF_TRUTH repo_root=$repoRoot warm_target=$warmTarget"
    Write-Host "PRE_PUSH_GATE_PLAN steps=cargo-check,check-examples,test-gate,lint packages=$($effectivePackages -join ',')"
    $testPlan = if ($FullTests) { "full-cfg-test-suites" } else { "production-golden-targets-where-defined" }
    Write-Host "PRE_PUSH_GATE_TEST_PLAN mode=$testPlan"
    $exclusionReason = if ($FullTests) { "production-check-and-golden-executable" } else { "routine-production-golden-selection" }
    Write-Host "PRE_PUSH_GATE_TEST_EXCLUSIONS packages=$($testGateExcludedPackages -join ',') reason=$exclusionReason"
    $runEngineGolden = $true
    $runEditorGolden = $false
    $runGameProduction = $true
    $lintPackages = @()
    $skipLintForRoutineGoldenTestOnly = $false
    if ($useChangedPackageSelection) {
        $runEngineGolden = @($changedPackageScope | Where-Object {
            $_ -in @("litd-engine", "litd-engine-routine-golden", "litd-hash")
        }).Count -gt 0
        $runEditorGolden = @($changedPackageScope | Where-Object {
            # The lightweight editor golden target depends on litd-editor-mcp. When the
            # production MCP surface changes, the broad cfg(test) target is intentionally
            # excluded from routine pushes, so the dedicated golden must run as the
            # executable-level proof for that dependency too.
            $_ -in @("litd-editor-mcp", "litd-editor-routine-golden")
        }).Count -gt 0
        $runGameProduction = @($changedPackageScope | Where-Object {
            $_ -in @("litd-game", "litd-engine", "litd-hash", "litd-engine-routine-golden")
        }).Count -gt 0
        # Targeted clippy is safe for a crate-local source change because cargo still compiles
        # that package's full dependency closure; examples/golden binaries exercise the feature
        # combinations separately. A routine-golden test-only package has no production lib/bin
        # target; its dedicated editor golden step below supplies the relevant executable check.
        $lintPackages = @($changedDirectPackages | Where-Object {
            $_ -notin @("litd-editor-routine-golden", "litd-engine-routine-golden")
        } | Sort-Object -Unique)
        $skipLintForRoutineGoldenTestOnly =
            $changedDirectPackages.Count -gt 0 -and
            @($changedDirectPackages | Where-Object {
                $_ -notin @("litd-editor-routine-golden", "litd-engine-routine-golden")
            }).Count -eq 0 -and
            $lintPackages.Count -eq 0 -and
            @($changeSet.Paths | Where-Object {
                $_ -match '^(Calyx/|Cargo\.toml$|Cargo\.lock$)'
            }).Count -eq 0
        Write-Host "PRE_PUSH_GATE_ENGINE_GOLDEN_SELECTION run=$runEngineGolden changed_scope=$($changedPackageScope -join ',')"
        Write-Host "PRE_PUSH_GATE_EDITOR_GOLDEN_SELECTION run=$runEditorGolden changed_scope=$($changedPackageScope -join ',')"
        Write-Host "PRE_PUSH_GATE_GAME_PRODUCTION_SELECTION run=$runGameProduction changed_scope=$($changedPackageScope -join ',')"
        Write-Host "PRE_PUSH_GATE_TEST_PACKAGE_SELECTION affected=$($testGatePackages -join ',')"
        Write-Host "PRE_PUSH_GATE_LINT_PACKAGE_SELECTION packages=$($lintPackages -join ',') targeted=$([bool]($lintPackages.Count -gt 0))"
        Write-Host "PRE_PUSH_GATE_LINT_SELECTION routine_golden_only=$skipLintForRoutineGoldenTestOnly"
    }

    if ($ChangedOnly -and -not $FullTests -and -not $changeSet.CompileRelevant) {
        Invoke-StaticOnlyStep -CommentOnlyRust:$commentOnlyRust -DeletedTestOnly:$deletedTestOnly
        $overallStopwatch.Stop()
        $completedMode = if ($commentOnlyRust) { "comment-only-rust" } elseif ($deletedTestOnly) { "deleted-test-only" } else { "static-only" }
        Write-Host ""
        Write-Host "PRE_PUSH_GATE_OK all_steps_passed=true mode=$completedMode elapsed_seconds=$([math]::Round($overallStopwatch.Elapsed.TotalSeconds, 1))"
        exit 0
    }

    Invoke-CargoCheckStep -PackageSelection $changedPackageSelection -UsePackageSelection:$useChangedPackageSelection -RunGameProduction:$runGameProduction
    if ($runEngineGolden) {
        Invoke-EngineGoldenStep
    } else {
        Write-Host "PRE_PUSH_GATE_OK step=engine-golden selection=skipped-package-unaffected"
    }
    if ($runEditorGolden) {
        Invoke-EditorGoldenStep
    } else {
        Write-Host "PRE_PUSH_GATE_OK step=editor-golden selection=skipped-package-unaffected"
    }
    Invoke-CheckExamplesStep
    foreach ($pkg in $testGatePackages) {
        Invoke-TestGateStep -Package $pkg
    }
    if ($skipLintForRoutineGoldenTestOnly) {
        Write-Host "PRE_PUSH_GATE_OK step=lint selection=skipped-routine-golden-only reason=dedicated-golden-target-already-ran"
    } else {
        Invoke-LintStep -LintTargetDir $warmTarget -LintPackages $lintPackages
    }

    $overallStopwatch.Stop()
    Write-Host ""
    Write-Host "PRE_PUSH_GATE_OK all_steps_passed=true packages=$($effectivePackages.Count) elapsed_seconds=$([math]::Round($overallStopwatch.Elapsed.TotalSeconds, 1))"
    exit 0
} catch {
    $overallStopwatch.Stop()
    Write-Host ""
    Write-Host "PRE_PUSH_GATE_ABORTED elapsed_seconds=$([math]::Round($overallStopwatch.Elapsed.TotalSeconds, 1)) message=$($_.Exception.Message)"
    exit 1
}
