[CmdletBinding()]
param(
    [switch]$Write,
    # Fast path for the git pre-commit hook (#1528): run ONLY the rustfmt steps
    # (GameEditor workspace + every Calyx package) and skip the heavy
    # `cargo check --all-targets` compile and the side-store-eradication gate.
    # This keeps the per-commit gate to a couple of seconds so formatting drift
    # is blocked at commit time without paying a full type-check on every commit.
    [switch]$FormatOnly,

    # Package-scoped cargo fmt calls are independent, but keeping the fan-out bounded avoids
    # turning the fast gate into a CPU/IO storm when another agent is compiling. The package
    # boundary is intentional: a workspace-wide invocation can exceed Windows' command-line
    # limit from a deep worktree (#1720).
    [ValidateRange(1, 16)]
    [int]$FormatConcurrency = 4,

    # The pre-commit hook uses this mode so a script-only or single-crate commit does not pay
    # for rustfmt across both workspaces. Static ratchets still scan the full tracked tree.
    [switch]$ChangedOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$gameEditorManifest = Join-Path $repoRoot "Cargo.toml"
$calyxManifest = Join-Path $repoRoot "Calyx\Cargo.toml"
$calyxRoot = Split-Path -Parent $calyxManifest
$script:formatGatePackageRoots = @{}

# #1563: transient-vs-real discrimination + bounded retry for the `cargo check` step.
# The heavy `cargo check --workspace --all-targets` step shares the warm target cache with
# other agents; a concurrent aborted build can surface a bare exit=-1 (no diagnostics) that
# must NOT read as a compile regression. This helper retries the transient signature and
# fails a REAL compile error (diagnostics present) immediately. Dot-sourced (functions only).
$cargoCheckRetryHelper = Join-Path $PSScriptRoot "gate-cargo-check-retry.ps1"
if (-not (Test-Path -LiteralPath $cargoCheckRetryHelper -PathType Leaf)) {
    throw "FORMAT_GATE_FAILED: helper not found: $cargoCheckRetryHelper"
}
. $cargoCheckRetryHelper

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "FORMAT_GATE_FAILED: cargo was not found on PATH"
}

foreach ($manifest in @($gameEditorManifest, $calyxManifest)) {
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw "FORMAT_GATE_FAILED: manifest not found: $manifest"
    }
}

function Get-WorkspacePackageNames {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WorkspaceRoot,

        [Parameter(Mandatory = $true)]
        [string]$WorkspaceName
    )

    Push-Location -LiteralPath $WorkspaceRoot
    try {
        $metadataJson = & cargo metadata --format-version 1 --no-deps
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw "FORMAT_GATE_FAILED: workspace=$WorkspaceName step=cargo-metadata exit_code=$exitCode cwd=$WorkspaceRoot"
        }
    } finally {
        Pop-Location
    }

    try {
        $metadata = $metadataJson | ConvertFrom-Json
    } catch {
        throw "FORMAT_GATE_FAILED: workspace=$WorkspaceName step=parse-metadata message=$($_.Exception.Message)"
    }

    $workspaceMemberIds = @($metadata.workspace_members | ForEach-Object { [string]$_ })
    $workspacePackages = @(
        $metadata.packages |
            Where-Object { $workspaceMemberIds -contains [string]$_.id } |
            ForEach-Object { [string]$_.name }
    )

    if ($workspacePackages.Count -eq 0) {
        throw "FORMAT_GATE_FAILED: workspace=$WorkspaceName step=package-discovery message=no_workspace_packages"
    }

    $emptyNames = @($workspacePackages | Where-Object { [string]::IsNullOrWhiteSpace($_) })
    if ($emptyNames.Count -ne 0) {
        throw "FORMAT_GATE_FAILED: workspace=$WorkspaceName step=package-discovery message=empty_package_name"
    }

    $uniqueNames = @($workspacePackages | Sort-Object -Unique)
    if ($uniqueNames.Count -ne $workspacePackages.Count) {
        throw "FORMAT_GATE_FAILED: workspace=$WorkspaceName step=package-discovery message=duplicate_package_name"
    }

    $packageRoots = @{}
    foreach ($package in @($metadata.packages | Where-Object { $workspaceMemberIds -contains [string]$_.id })) {
        $packageRoots[[string]$package.name] = [System.IO.Path]::GetFullPath(
            (Split-Path -Parent ([string]$package.manifest_path))
        )
    }
    $script:formatGatePackageRoots[$WorkspaceName] = $packageRoots
    return $uniqueNames
}

function Get-ChangedPackageNames {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [string[]]$ChangedPaths
    )

    $changedPackageNames = New-Object System.Collections.Generic.HashSet[string] ([System.StringComparer]::OrdinalIgnoreCase)
    $formatAllGameEditor = $false
    $formatAllCalyx = $false
    foreach ($relativePath in $ChangedPaths) {
        $normalized = ([string]$relativePath).Replace('/', '\')
        # Workspace manifests/lockfiles affect package discovery and dependency resolution, but
        # they do not change which Rust sources rustfmt formats. Staged Rust paths are mapped to
        # their owning packages below, so a manifest-only commit must not fan out to every
        # formatter (#1895). rustfmt.toml remains workspace-wide because it changes formatting
        # policy itself.
        if ($normalized -eq "rustfmt.toml") {
            $formatAllGameEditor = $true
            $formatAllCalyx = $true
        } elseif ($normalized -eq "Calyx\rustfmt.toml") {
            $formatAllCalyx = $true
        }
    }

    foreach ($workspaceName in @("gameeditor-workspace", "calyx-workspace")) {
        $packageRoots = $script:formatGatePackageRoots[$workspaceName]
        $workspacePrefix = if ($workspaceName -eq "calyx-workspace") {
            ([System.IO.Path]::GetFullPath($calyxRoot)).TrimEnd('\') + '\'
        } else {
            ([System.IO.Path]::GetFullPath($repoRoot)).TrimEnd('\') + '\'
        }
        foreach ($packageName in $packageRoots.Keys) {
            $packageRoot = ([System.IO.Path]::GetFullPath([string]$packageRoots[$packageName])).TrimEnd('\') + '\'
            foreach ($relativePath in $ChangedPaths) {
                $absolutePath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot (([string]$relativePath).Replace('/', '\'))))
                if ($workspaceName -eq "calyx-workspace" -and -not $absolutePath.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
                    continue
                }
                if ($absolutePath.StartsWith($packageRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
                    [void]$changedPackageNames.Add([string]$packageName)
                }
            }
        }
    }

    if ($formatAllGameEditor) {
        foreach ($packageName in $script:formatGatePackageRoots["gameeditor-workspace"].Keys) {
            [void]$changedPackageNames.Add([string]$packageName)
        }
    }
    if ($formatAllCalyx) {
        foreach ($packageName in $script:formatGatePackageRoots["calyx-workspace"].Keys) {
            [void]$changedPackageNames.Add([string]$packageName)
        }
    }
    return @($changedPackageNames)
}

function Invoke-CargoFormatBatch {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Batch
    )

    $running = New-Object System.Collections.ArrayList
    foreach ($step in $Batch) {
        $commandLine = "cargo " + ($step.Args -join " ")
        Write-Host "FORMAT_GATE_START step=$($step.Name) cwd=$($step.WorkingDirectory) command=$commandLine"
        try {
            $process = Start-Process `
                -FilePath "cargo.exe" `
                -ArgumentList @($step.Args) `
                -WorkingDirectory $step.WorkingDirectory `
                -NoNewWindow `
                -PassThru `
                -ErrorAction Stop
            [void]$running.Add([pscustomobject]@{
                    step = $step
                    process = $process
                    command = $commandLine
                })
        } catch {
            [void]$running.Add([pscustomobject]@{
                    step = $step
                    process = $null
                    command = $commandLine
                    launch_error = $_.Exception.Message
                })
        }
    }

    $failures = New-Object System.Collections.ArrayList
    foreach ($entry in $running) {
        if ($null -eq $entry.process) {
            [void]$failures.Add("step=$($entry.step.Name) launch_error=$($entry.launch_error) cwd=$($entry.step.WorkingDirectory) command=$($entry.command)")
            continue
        }
        $entry.process.WaitForExit()
        $exitCode = $entry.process.ExitCode
        if ($exitCode -ne 0) {
            [void]$failures.Add("step=$($entry.step.Name) exit_code=$exitCode cwd=$($entry.step.WorkingDirectory) command=$($entry.command)")
        } else {
            Write-Host "FORMAT_GATE_OK step=$($entry.step.Name)"
        }
        $entry.process.Dispose()
    }

    if ($failures.Count -ne 0) {
        throw "FORMAT_GATE_FAILED: parallel package batch failed`n$($failures -join "`n")"
    }
}

function Invoke-PowerShellGateStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$ScriptPath
    )

    if (-not (Test-Path -LiteralPath $ScriptPath -PathType Leaf)) {
        throw "FORMAT_GATE_FAILED: step=$Name script_not_found=$ScriptPath"
    }

    Write-Host "FORMAT_GATE_START step=$Name command=$ScriptPath"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $ScriptPath
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "FORMAT_GATE_FAILED: step=$Name exit_code=$exitCode command=$ScriptPath"
    }
    Write-Host "FORMAT_GATE_OK step=$Name"
}

function Invoke-PowerShellGateBatch {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Batch
    )

    $running = New-Object System.Collections.ArrayList
    foreach ($entry in $Batch) {
        Write-Host "FORMAT_GATE_START step=$($entry.Name) command=$($entry.ScriptPath)"
        try {
            $process = Start-Process `
                -FilePath "powershell.exe" `
                -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $entry.ScriptPath) `
                -WorkingDirectory $repoRoot `
                -NoNewWindow `
                -PassThru `
                -ErrorAction Stop
            [void]$running.Add([pscustomobject]@{ entry = $entry; process = $process })
        } catch {
            [void]$running.Add([pscustomobject]@{ entry = $entry; process = $null; launch_error = $_.Exception.Message })
        }
    }

    $failures = New-Object System.Collections.ArrayList
    foreach ($item in $running) {
        if ($null -eq $item.process) {
            [void]$failures.Add("step=$($item.entry.Name) launch_error=$($item.launch_error) command=$($item.entry.ScriptPath)")
            continue
        }
        $item.process.WaitForExit()
        $exitCode = $item.process.ExitCode
        if ($exitCode -ne 0) {
            [void]$failures.Add("step=$($item.entry.Name) exit_code=$exitCode command=$($item.entry.ScriptPath)")
        } else {
            Write-Host "FORMAT_GATE_OK step=$($item.entry.Name)"
        }
        $item.process.Dispose()
    }
    if ($failures.Count -ne 0) {
        throw "FORMAT_GATE_FAILED: parallel PowerShell gate batch failed`n$($failures -join "`n")"
    }
}

$rustfmtArgs = @()
if (-not $Write) {
    $rustfmtArgs = @("--", "--check")
}

$changedPaths = @()
$changedOnlyMetadataRequired = $true
if ($ChangedOnly) {
    $changedPaths = @(& git diff --cached --name-only --diff-filter=ACMR)
    $gitExitCode = $LASTEXITCODE
    if ($gitExitCode -ne 0) {
        throw "FORMAT_GATE_FAILED: changed-only staged path discovery failed exit_code=$gitExitCode"
    }
    # Script/docs/lockfile-only staged changes cannot own a Rust package formatter. Skip the two
    # cargo-metadata calls in that case; manifests and package-looking paths still take the full
    # metadata ownership path below. Empty staged input remains fail-closed for static ratchets.
    $changedOnlyMetadataRequired = @($changedPaths | Where-Object {
            $normalized = ([string]$_).Replace('/', '\\')
            $normalized -match '(^|\\)(Cargo\.toml|rustfmt\.toml)$' -or
            $normalized -match '(^|\\)crates\\' -or
            $normalized -match '^Calyx\\crates\\' -or
            $normalized -match '\\.rs$'
        }).Count -gt 0
    Write-Host "FORMAT_GATE_CHANGED_ONLY_METADATA required=$changedOnlyMetadataRequired"
}

if ($ChangedOnly -and -not $changedOnlyMetadataRequired) {
    $gameEditorPackages = @()
    $calyxPackages = @()
} else {
    $gameEditorPackages = @(Get-WorkspacePackageNames -WorkspaceRoot $repoRoot -WorkspaceName "gameeditor-workspace")
    $calyxPackages = @(Get-WorkspacePackageNames -WorkspaceRoot $calyxRoot -WorkspaceName "calyx-workspace")
}

# #1720: format the GameEditor workspace PACKAGE-SCOPED (one `cargo fmt -p <pkg>` per
# workspace member) instead of a single workspace-wide `cargo fmt`. A workspace-wide invocation
# concatenates the absolute path of every target root across ALL packages into ONE rustfmt
# command line; from a deep worktree base (e.g. .claude\worktrees\<id>\...) that aggregate
# command line overflows the Windows command-line limit and cargo aborts with os error 206
# ("The filename or extension is too long"). That failure is environmental (not a formatting
# defect) and forced every worktree agent onto --no-verify, hollowing out the gate. Partitioning
# by package keeps each rustfmt command line short while still failing LOUD on any real
# formatting drift - every per-package step throws on a non-zero exit. This mirrors the Calyx
# per-package loop below and Calyx\scripts\cargo-fmt-workspace.ps1, which adopted it for the
# same reason.
$steps = @()
foreach ($packageName in $gameEditorPackages) {
    $steps += [PSCustomObject]@{
        Name = "gameeditor-package:$packageName"
        PackageName = $packageName
        WorkingDirectory = $repoRoot
        Args = @("fmt", "-p", $packageName) + $rustfmtArgs
    }
}

foreach ($packageName in $calyxPackages) {
    $steps += [PSCustomObject]@{
        Name = "calyx-package:$packageName"
        PackageName = $packageName
        WorkingDirectory = $calyxRoot
        Args = @("fmt", "-p", $packageName) + $rustfmtArgs
    }
}

if (-not $FormatOnly) {
    $steps += [PSCustomObject]@{
        Name = "gameeditor-all-targets"
        WorkingDirectory = $repoRoot
        Args = @("check", "--workspace", "--all-targets")
        # #1563: route this step through the transient-vs-real retry helper (below).
        TransientRetry = $true
    }
}

Write-Host "FORMAT_GATE_SOURCE_OF_TRUTH manifests=$gameEditorManifest,$calyxManifest mode=$(if ($Write) { 'write' } else { 'check' }) format_only=$FormatOnly"
Write-Host "FORMAT_GATE_GAMEEDITOR_PACKAGES count=$($gameEditorPackages.Count) names=$($gameEditorPackages -join ',')"
Write-Host "FORMAT_GATE_CALYX_PACKAGES count=$($calyxPackages.Count) names=$($calyxPackages -join ',')"

$formatSteps = @($steps | Where-Object {
        -not (($_.PSObject.Properties.Name -contains 'TransientRetry') -and $_.TransientRetry)
    })
$changedPackageNames = @()
if ($ChangedOnly) {
    if ($changedOnlyMetadataRequired) {
        $changedPackageNames = @(Get-ChangedPackageNames -ChangedPaths $changedPaths)
    }
    $formatSteps = @($formatSteps | Where-Object {
            $changedPackageNames -contains [string]$_.PackageName
        })
    Write-Host "FORMAT_GATE_CHANGED_ONLY staged_paths=$($changedPaths.Count) packages=$($changedPackageNames -join ',')"
}
$nonFormatSteps = @($steps | Where-Object {
        ($_.PSObject.Properties.Name -contains 'TransientRetry') -and $_.TransientRetry
    })

# Format checks only read source and metadata, so package-scoped calls can be bounded-batched.
# The full compile/check step remains ordered and uses its existing transient-vs-real retry path.
for ($offset = 0; $offset -lt $formatSteps.Count; $offset += $FormatConcurrency) {
    $end = [Math]::Min($offset + $FormatConcurrency - 1, $formatSteps.Count - 1)
    $batch = @($formatSteps[$offset..$end])
    Invoke-CargoFormatBatch -Batch $batch
}

foreach ($step in $nonFormatSteps) {
    Invoke-CargoCheckStepWithRetry -Name $step.Name -WorkingDirectory $step.WorkingDirectory -CargoArgs $step.Args
}

$totalSteps = $formatSteps.Count + $nonFormatSteps.Count

# #1466: <500-line ratchet. Runs on EVERY gate invocation - including the fast pre-commit
# -FormatOnly path - so no commit can add a new oversized .rs file or grow a baselined one
# (#800 sec.6). Fast (line counts only, no build). Fix a failure by splitting the file, then run
# scripts\check-line-count.ps1 -Update to re-baseline.
$staticGates = @(
    [pscustomobject]@{ Name = "line-count-ratchet"; ScriptPath = (Join-Path $PSScriptRoot "check-line-count.ps1") },
    [pscustomobject]@{ Name = "temp-dir-hygiene-ratchet"; ScriptPath = (Join-Path $PSScriptRoot "check-temp-dir-hygiene.ps1") },
    [pscustomobject]@{ Name = "doctrine-refs-integrity"; ScriptPath = (Join-Path $PSScriptRoot "check-doctrine-refs.ps1") },
    [pscustomobject]@{ Name = "doc-refs"; ScriptPath = (Join-Path $PSScriptRoot "check-doc-refs.ps1") },
    [pscustomobject]@{ Name = "reference-artifact-doctrine"; ScriptPath = (Join-Path $PSScriptRoot "check-reference-artifacts.ps1") },
    [pscustomobject]@{ Name = "ascii-gate-scripts"; ScriptPath = (Join-Path $PSScriptRoot "check-ascii-gate-scripts.ps1") },
    [pscustomobject]@{ Name = "windows-platform-contract"; ScriptPath = (Join-Path $PSScriptRoot "check-windows-platform.ps1") },
    [pscustomobject]@{ Name = "fsv-idioms"; ScriptPath = (Join-Path $PSScriptRoot "check-fsv-idioms.ps1") }
)
if ($ChangedOnly -and $changedPaths.Count -gt 0) {
    $staticGateNames = New-Object System.Collections.Generic.HashSet[string]
    foreach ($relativePath in $changedPaths) {
        $normalized = ([string]$relativePath).Replace('/', '\').ToLowerInvariant()
        if ($normalized -match '\.rs$') {
            [void]$staticGateNames.Add("line-count-ratchet")
            [void]$staticGateNames.Add("temp-dir-hygiene-ratchet")
        }
        if ($normalized -match '\.md$|docs2\\|docs\\prompts\\') {
            [void]$staticGateNames.Add("doctrine-refs-integrity")
            [void]$staticGateNames.Add("doc-refs")
            [void]$staticGateNames.Add("reference-artifact-doctrine")
            [void]$staticGateNames.Add("fsv-idioms")
        }
        if ($normalized -match '(^|\\)scripts(\\|/).*\.ps1$' -or
            $normalized -match '(^|\\)\.githooks(\\|/).*') {
            [void]$staticGateNames.Add("ascii-gate-scripts")
            [void]$staticGateNames.Add("windows-platform-contract")
            [void]$staticGateNames.Add("fsv-idioms")
        }
        if ($normalized -match '(^|\\)(cargo\.toml|cargo\.lock|rustfmt\.toml)$|reference-artifact-doctrine\.tsv$|doc-refs-allowlist\.txt$|doctrine-refs-manifest\.tsv$') {
            [void]$staticGateNames.Add("doctrine-refs-integrity")
            [void]$staticGateNames.Add("doc-refs")
            [void]$staticGateNames.Add("reference-artifact-doctrine")
            [void]$staticGateNames.Add("windows-platform-contract")
        }
        if ($normalized -match 'docs\\readme\\.*\.(png|jpg|jpeg)$') {
            [void]$staticGateNames.Add("reference-artifact-doctrine")
        }
    }
    $staticGates = @($staticGates | Where-Object { $staticGateNames.Contains([string]$_.Name) })
    Write-Host "FORMAT_GATE_CHANGED_ONLY static_gates=$($staticGates.Name -join ',')"
}
for ($offset = 0; $offset -lt $staticGates.Count; $offset += $FormatConcurrency) {
    $end = [Math]::Min($offset + $FormatConcurrency - 1, $staticGates.Count - 1)
    Invoke-PowerShellGateBatch -Batch @($staticGates[$offset..$end])
}
$totalSteps += $staticGates.Count

# #1535: raw std::env::temp_dir() ratchet. Runs on EVERY gate invocation (fast, grep-only) so no
# commit can add a new uncleaned temp-dir helper (which leaks %TEMP%/litd-* dirs forever). Fix a
# failure by using litd_engine::test_temp::TestTempDir, then run
# scripts\check-temp-dir-hygiene.ps1 -Update to re-baseline.

# #1465: doctrine-reference integrity - TWO complementary layers, both fast and git-only.
# Layer 1 (manifest): files cited as binding by GITHUB ISSUE BODIES cannot be discovered by
# scanning the repo, so scripts/doctrine-refs-manifest.tsv curates them; each listed path must
# be git-tracked. Fix a failure by committing the file, or retire the reference in the manifest.

# Layer 2 (scanner): every tracked .md referencing a docs2/docs/prompts path requires that
# path to itself be GIT-TRACKED (existence on disk does not count - that was the #1465 loss
# mode). Fix a failure by committing the referenced file or correcting the reference;
# intentional external refs go in scripts\doc-refs-allowlist.txt.

# #1510: reference-artifact doctrine integrity. The #1465 gates above prove a doctrine-cited
# file EXISTS; they say nothing about whether its CONTENT still agrees with doctrine. ED12-
# play-topdown.png was a blessed #780/#800 quality floor whose pixels carried a minimap, quest
# trackers and a player-position marker -- every one forbidden in packaged player runtime by
# the acquired-map doctrine (#944). Nothing checked the two still agreed. This gate makes the
# agreement machine-checkable: every docs/readme/*.png must carry a doctrine verdict in
# scripts/reference-artifact-doctrine.tsv, and a FORBIDDEN_NAV artifact may not be presented
# in tracked markdown without its obsolescence annotation. Fast (git + text only).

# #1570: ASCII-only gate scripts. Gate scripts are stored without a BOM, so any non-ASCII
# byte (em-dash, smart quote, arrow) makes Windows PowerShell 5.1 die with a MISLEADING
# parse error instead of its real diagnostic when the pre-commit hook falls back from pwsh
# (#800 sec.6). This step is the enforcement that was missing. Fast (git + text only). Fix a
# failure by replacing the flagged non-ASCII character with its ASCII equivalent.


# #1556: FSV-idiom gate. An FSV instruction is itself code and must itself be verified. Bans
# two blind-measurement idioms in tracked docs/scripts: a blank-line-blind line counter that
# silently undercounts, and a native command whose exit code is read after a pipeline
# short-circuit (which corrupts the exit code every gate decides on). Fast (git + text only).
# Fix a failure per the message, or mark a genuinely-benign line with the allow comment it names.

if (-not $FormatOnly) {
    $sideStoreGate = Join-Path $PSScriptRoot "check-calyx-side-store-eradication.ps1"
    Invoke-PowerShellGateStep -Name "calyx-side-store-eradication" -ScriptPath $sideStoreGate
    $totalSteps += 1
}

# #1778: FSV-example compile coverage. The `gameeditor-all-targets` step above runs
# `cargo check --workspace --all-targets`, which SILENTLY SKIPS any target whose
# required-features are off. Every FSV example carries required-features=["fsv"]
# (crates/<pkg>/Cargo.toml), so that step reports GREEN while never compiling ~230 example
# binaries. That was the exact hole that let the #1725/#1749 AreaSpec schema change land five
# broken area examples on main behind a passing format gate: the only place that compiled the
# examples was the pre-push check-examples step, which a `git push --no-verify` bypasses. Close
# it by compiling the examples here (fsv feature on) so the FULL format gate fails LOUD on a
# rotted example instead of deferring all example coverage to a bypassable pre-push step. Full
# mode only -- the fast pre-commit -FormatOnly path stays compile-free.
if (-not $FormatOnly) {
    $checkExamplesGate = Join-Path $PSScriptRoot "check-examples.ps1"
    Invoke-PowerShellGateStep -Name "fsv-examples-compile" -ScriptPath $checkExamplesGate
    $totalSteps += 1
}

Write-Host "FORMAT_GATE_OK all_steps=$totalSteps"
