Set-StrictMode -Version Latest

function Test-LitdRelativePathOutside {
    param([Parameter(Mandatory = $true)][string]$RelativePath)

    [System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -eq ".." -or
        $RelativePath.StartsWith("..\", [System.StringComparison]::Ordinal) -or
        $RelativePath.StartsWith("../", [System.StringComparison]::Ordinal)
}

function Resolve-LitdOwnedTargetPath {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$TargetDir
    )

    $repo = [System.IO.Path]::GetFullPath($RepoRoot)
    $targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repo "target"))
    $candidate = [System.IO.Path]::GetFullPath(
        $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($TargetDir)
    )
    $relative = [System.IO.Path]::GetRelativePath($targetRoot, $candidate)
    if ($relative -eq ".") {
        throw "TEST_GATE_TARGET_ROOT_FORBIDDEN: target dir must be a child of $targetRoot"
    }
    if (Test-LitdRelativePathOutside -RelativePath $relative) {
        throw "TEST_GATE_TARGET_OUTSIDE_REPO: $candidate is outside $targetRoot"
    }
    foreach ($rootToCheck in @($repo, $targetRoot)) {
        if (-not (Test-Path -LiteralPath $rootToCheck)) {
            continue
        }
        $item = Get-Item -LiteralPath $rootToCheck -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "TEST_GATE_TARGET_REPARSE_ESCAPE: cleanup root cannot be a link or junction: $rootToCheck"
        }
    }
    $cursor = $targetRoot
    foreach ($segment in ($relative -split '[\\/]')) {
        if (-not $segment -or $segment -eq ".") {
            continue
        }
        $cursor = Join-Path $cursor $segment
        if (-not (Test-Path -LiteralPath $cursor)) {
            break
        }
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "TEST_GATE_TARGET_REPARSE_ESCAPE: owned target contains a link or junction: $cursor"
        }
    }
    $candidate
}

function Assert-LitdTargetDoesNotContainPath {
    param(
        [Parameter(Mandatory = $true)][string]$TargetDir,
        [Parameter(Mandatory = $true)][string]$ProtectedPath,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $target = [System.IO.Path]::GetFullPath($TargetDir)
    $protected = [System.IO.Path]::GetFullPath($ProtectedPath)
    $relative = [System.IO.Path]::GetRelativePath($target, $protected)
    if (-not (Test-LitdRelativePathOutside -RelativePath $relative)) {
        throw "TEST_GATE_TARGET_CONTAINS_PROTECTED_PATH: $target contains $Label $protected"
    }
}

function Get-LitdTargetProcessUsers {
    param([Parameter(Mandatory = $true)][string]$TargetDir)

    $target = [System.IO.Path]::GetFullPath($TargetDir)
    @(
        Get-CimInstance Win32_Process | Where-Object {
            $_.ProcessId -ne $PID -and (
                ($_.ExecutablePath -and $_.ExecutablePath.StartsWith(
                    $target + "\",
                    [System.StringComparison]::OrdinalIgnoreCase
                )) -or
                ($_.CommandLine -and $_.CommandLine.IndexOf(
                    $target,
                    [System.StringComparison]::OrdinalIgnoreCase
                ) -ge 0)
            )
        } | Select-Object ProcessId, Name, ExecutablePath, CommandLine
    )
}

function Get-LitdOwnedTargetCacheStatus {
    <#
        Non-destructive counterpart to Remove-LitdOwnedTargetTree. The gate target dir is a
        STABLE, shared, warm incremental build cache (target\fsv\test-*\cargo-target), not
        per-run scratch: its entire value is being REUSED across runs so cargo can fingerprint-
        skip the 95% of the graph (wgpu, naga, rapier, 40+ Calyx crates at O3) that did not
        change. Wiping it on exit - as the gate did through #1498 - turned every run into a full
        cold rebuild. This records that the cache was retained (cheap Test-Path only; it does NOT
        recursively size the multi-GB tree on the hot path) so the report proves reuse, not
        removal. Disk is bounded out-of-band by scripts/prune-build-artifacts.ps1 (cargo-sweep
        --maxsize), which is the correct layer for build-cache hygiene.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$TargetDir
    )

    $target = Resolve-LitdOwnedTargetPath -RepoRoot $RepoRoot -TargetDir $TargetDir
    $exists = Test-Path -LiteralPath $target
    [pscustomobject]@{
        path = $target
        existed = $exists
        retained = $true
        removed = $false
        exists_after = $exists
    }
}

function Remove-LitdOwnedTargetTree {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$TargetDir
    )

    $target = Resolve-LitdOwnedTargetPath -RepoRoot $RepoRoot -TargetDir $TargetDir
    if (-not (Test-Path -LiteralPath $target)) {
        return [pscustomobject]@{
            path = $target
            existed = $false
            file_count = 0
            directory_count = 0
            bytes = 0
            process_user_count = 0
            removed = $false
            exists_after = $false
        }
    }
    $entries = @(Get-ChildItem -LiteralPath $target -Recurse -Force -ErrorAction Stop)
    $files = @($entries | Where-Object { -not $_.PSIsContainer })
    $directories = @($entries | Where-Object { $_.PSIsContainer })
    $bytes = [int64]0
    if ($files.Count -ne 0) {
        $bytes = [int64](($files | Measure-Object -Property Length -Sum).Sum)
    }
    $processUsers = @(Get-LitdTargetProcessUsers -TargetDir $target)
    if ($processUsers.Count -ne 0) {
        throw "TEST_GATE_TARGET_IN_USE: refusing to remove $target; process_users=$($processUsers | ConvertTo-Json -Compress)"
    }
    Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction Stop
    $existsAfter = Test-Path -LiteralPath $target
    if ($existsAfter) {
        throw "TEST_GATE_TARGET_CLEANUP_FAILED: $target still exists after Remove-Item"
    }
    [pscustomobject]@{
        path = $target
        existed = $true
        file_count = $files.Count
        directory_count = $directories.Count
        bytes = [int64]$bytes
        process_user_count = $processUsers.Count
        removed = $true
        exists_after = $existsAfter
    }
}
