[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$ManifestPath,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Package,

    [string]$TestFilter,

    # Select one explicit integration-test target. Unlike the default --tests selection, this
    # avoids compiling the package's cfg(test) unit-test target when a production golden path
    # exists. Full package tests remain available by omitting this parameter.
    [string]$TestTarget,

    [string[]]$Examples,

    [string[]]$Features,

    [string]$FsvRoot,

    [string]$TargetDir,

    [switch]$NoCapture,

    [switch]$Locked,

    [int]$LockTimeoutSeconds = 600,

    [int]$TestTimeoutSeconds = 900,

    # #1554: continuous incremental-cache cap, enforced under the gate mutex before each build so
    # target\<profile>\incremental cannot grow unbounded intraday (the once-daily GC let it hit
    # 50 GB / 476 dirs). Above EITHER cap the whole incremental dir is pruned (deps\ untouched ->
    # one non-incremental build). Matches prune-build-artifacts.ps1's defaults.
    [int]$IncrementalMaxSizeGB = 10,

    [int]$IncrementalMaxDirCount = 400
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "TestRustPackageJob.ps1")
. (Join-Path $PSScriptRoot "LitdOwnedTarget.ps1")
. (Join-Path $PSScriptRoot "TestRustPackageOutput.ps1")
. (Join-Path $PSScriptRoot "IncrementalCacheRecovery.ps1")
. (Join-Path $PSScriptRoot "IncrementalCacheCap.ps1")

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifest = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ManifestPath)
$targetDir = if ($TargetDir) {
    Resolve-LitdOwnedTargetPath -RepoRoot $repoRoot -TargetDir $TargetDir
} else {
    $null
}

$reportRoot = if ($FsvRoot) {
    $resolved = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($FsvRoot)
    New-Item -ItemType Directory -Force -Path $resolved | Out-Null
    $resolved
} else {
    $defaultRoot = Join-Path $repoRoot "target\fsv\test-rust-package"
    New-Item -ItemType Directory -Force -Path $defaultRoot | Out-Null
    $defaultRoot
}
if ($targetDir) {
    Assert-LitdTargetDoesNotContainPath `
        -TargetDir $targetDir `
        -ProtectedPath $reportRoot `
        -Label "test report root"
}

$reportPackage = $Package -replace "[^A-Za-z0-9_.-]", "_"
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$reportPath = Join-Path $reportRoot ("test-rust-package-{0}-{1}.json" -f $reportPackage, $timestamp)
$stdoutPath = Join-Path $reportRoot ("test-rust-package-{0}-{1}.stdout.log" -f $reportPackage, $timestamp)
$stderrPath = Join-Path $reportRoot ("test-rust-package-{0}-{1}.stderr.log" -f $reportPackage, $timestamp)
$processMatchNeedle = if ($targetDir) { $targetDir } else { $repoRoot }
$oldFsvRoot = $env:CALYX_FSV_ROOT
$env:CALYX_FSV_ROOT = $reportRoot

# The gate mutex exists to stop two cargo builds from corrupting a SHARED target tree.
# Scope it to the target dir, not the whole machine: each gate builds into its own STABLE,
# reused warm cache (test-calyx/test-gameeditor pass target\fsv\test-*\cargo-target), so two
# gates that build into DIFFERENT trees cannot conflict and are free to run concurrently --
# only gates sharing a tree serialize on that tree's mutex. A single machine-wide lock turned
# every agent's gate into one serial queue even when their trees were independent (#1499).
# Runs with no explicit target dir fall back to the repo root, preserving the old whole-repo
# serialization for that case.
$gateScope = if ($targetDir) { $targetDir } else { $repoRoot }
$scopeSha = [System.Security.Cryptography.SHA256]::Create()
try {
    $scopeHash = ([BitConverter]::ToString($scopeSha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($gateScope.ToLowerInvariant())))).Replace("-", "").Substring(0, 16)
} finally {
    $scopeSha.Dispose()
}
$mutexName = "Local\LitdGameEditorCargoTestGate-$scopeHash"
$mutex = New-Object System.Threading.Mutex($false, $mutexName)
$lockStopwatch = [System.Diagnostics.Stopwatch]::new()
$acquired = $false
$testStopwatch = [System.Diagnostics.Stopwatch]::new()
$exitCode = $null
$reportWritten = $false
$errorCode = $null
$timedOut = $false
$processId = $null
$processTreeBeforeStop = @()
$processTreeAfterStop = @()
$processStopErrors = @()
$knownProcessIds = New-Object "System.Collections.Generic.HashSet[int]"
$jobHandle = [IntPtr]::Zero
$startedAtUtc = [DateTimeOffset]::UtcNow
$targetCache = $null
$targetCacheError = $null
$testExecutionCount = $null
# #1536: at most ONE recovery retry. A stale incremental work product is recoverable; the SAME
# corruption surviving a cache clear is a real bug (or a sick host) and must go red, not spin.
$maxCargoAttempts = 2
$incrementalRecovery = $null
# #1554: records from the pre-build incremental-cache cap enforcement (one per profile evaluated).
$incrementalCap = $null
$attempts = New-Object System.Collections.ArrayList
$commandArgs = @("test")
if ($Locked) {
    $commandArgs += "--locked"
}
if ($TestTarget -and $Examples -and $Examples.Count -gt 0) {
    throw "TEST_GATE_INVALID: TestTarget cannot be combined with Examples"
}
if ($TestTarget -and $TestFilter) {
    throw "TEST_GATE_INVALID: TestTarget cannot be combined with TestFilter"
}
if ($Examples -and $Examples.Count -gt 0) {
    foreach ($example in $Examples) { $commandArgs += @("--example", $example) }
} else {
    if ($TestTarget) {
        # Cargo target selection avoids the implicit library/binary unit-test builds from --tests.
        $commandArgs += @("--test", $TestTarget)
    } else {
        # Normal gates exclude examples; the explicit example gate supplies harnessless mains.
        $commandArgs += "--tests"
    }
}
$commandArgs += @("--manifest-path", $manifest, "-p", $Package)
if ($Features -and $Features.Count -gt 0) {
    $commandArgs += @("--features", ($Features -join ","))
}
if ($targetDir) {
    $commandArgs += @("--target-dir", $targetDir)
}
if ($TestFilter) {
    $commandArgs += $TestFilter
}
if ($NoCapture) {
    $commandArgs += @("--", "--nocapture")
}

function ConvertTo-CommandLineArgument {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ($null -eq $Value) {
        return '""'
    }
    if ($Value -notmatch '[\s"]') {
        return $Value
    }
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Get-LogTail {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [int]$LineCount = 80
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    $content = Get-Content -LiteralPath $Path -Tail $LineCount -ErrorAction Stop
    if ($null -eq $content) {
        return ""
    }
    return (($content | Out-String).TrimEnd())
}

function Get-ProcessSnapshot {
    $items = @(Get-CimInstance Win32_Process | ForEach-Object {
            [pscustomobject]@{
                process_id = [int]$_.ProcessId
                parent_process_id = [int]$_.ParentProcessId
                name = [string]$_.Name
                command_line = if ($null -eq $_.CommandLine) { $null } else { [string]$_.CommandLine }
            }
        })
    return $items
}

function Test-ProcessMatchesTestGate {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Process
    )

    if ($null -eq $Process.command_line) {
        return $false
    }
    return $Process.command_line.Contains($processMatchNeedle)
}

function Get-TrackedProcessTree {
    param(
        [int[]]$RootIds,

        [Parameter(Mandatory = $true)]
        [System.Collections.Generic.HashSet[int]]$KnownIds
    )

    $all = @(Get-ProcessSnapshot)
    $byId = @{}
    $byParent = @{}
    foreach ($item in $all) {
        $byId[$item.process_id] = $item
        if (-not $byParent.ContainsKey($item.parent_process_id)) {
            $byParent[$item.parent_process_id] = New-Object System.Collections.ArrayList
        }
        [void]$byParent[$item.parent_process_id].Add($item)
    }

    $rootIdSet = New-Object "System.Collections.Generic.HashSet[int]"
    $seedIds = New-Object "System.Collections.Generic.HashSet[int]"
    foreach ($rootId in @($RootIds)) {
        if ($null -ne $rootId) {
            [void]$rootIdSet.Add([int]$rootId)
            [void]$seedIds.Add([int]$rootId)
        }
    }
    foreach ($knownId in $KnownIds) {
        [void]$seedIds.Add([int]$knownId)
    }

    $queue = New-Object "System.Collections.Generic.Queue[object]"
    $queuedParents = New-Object "System.Collections.Generic.HashSet[int]"
    foreach ($seedId in $seedIds) {
        if ($byId.ContainsKey($seedId)) {
            $seed = $byId[$seedId]
            if ($rootIdSet.Contains([int]$seedId) -or (Test-ProcessMatchesTestGate -Process $seed)) {
                $queue.Enqueue([pscustomobject]@{ item = $seed; depth = 0 })
            }
        } elseif ($byParent.ContainsKey($seedId)) {
            foreach ($child in @($byParent[$seedId])) {
                if (Test-ProcessMatchesTestGate -Process $child) {
                    $queue.Enqueue([pscustomobject]@{ item = $child; depth = 1 })
                }
            }
            [void]$queuedParents.Add([int]$seedId)
        }
    }

    $seen = New-Object "System.Collections.Generic.HashSet[int]"
    $result = New-Object System.Collections.ArrayList
    while ($queue.Count -gt 0) {
        $entry = $queue.Dequeue()
        $item = $entry.item
        if (-not $seen.Add([int]$item.process_id)) {
            continue
        }
        [void]$result.Add([pscustomobject]@{
                process_id = [int]$item.process_id
                parent_process_id = [int]$item.parent_process_id
                depth = [int]$entry.depth
                name = $item.name
                command_line = $item.command_line
            })

        if ($byParent.ContainsKey($item.process_id)) {
            foreach ($child in @($byParent[$item.process_id])) {
                if ((Test-ProcessMatchesTestGate -Process $child) -or $queuedParents.Contains([int]$item.process_id)) {
                    $queue.Enqueue([pscustomobject]@{ item = $child; depth = ([int]$entry.depth + 1) })
                }
            }
        }
    }

    return @($result | Sort-Object depth, process_id)
}

function Write-TestGateReport {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Status,

        [AllowNull()]
        [object]$ExitCode,

        [AllowNull()]
        [string]$ErrorCode,

        [AllowNull()]
        [string]$ErrorMessage
    )

    $report = [ordered]@{
        package = $Package
        test_filter = if ($TestFilter) { $TestFilter } else { $null }
        test_target = if ($TestTarget) { $TestTarget } else { $null }
        examples = @($Examples)
        test_execution_count = $testExecutionCount
        features = @($Features)
        command = "cargo " + ($commandArgs -join " ")
        cargo_args = $commandArgs
        status = $Status
        exit_code = $ExitCode
        error_code = $ErrorCode
        error_message = $ErrorMessage
        timed_out = $timedOut
        locked = [bool]$Locked
        process_id = $processId
        test_timeout_seconds = $TestTimeoutSeconds
        lock_name = $mutexName
        lock_wait_ms = $lockStopwatch.ElapsedMilliseconds
        elapsed_ms = $testStopwatch.ElapsedMilliseconds
        # #1536: null on a normal run. Non-null means a stale rustc incremental work product was
        # detected and its crate's cache was cleared -- a RECOVERED false-red, which must stay
        # visible in the evidence rather than vanishing into a silent green.
        incremental_recovery = $incrementalRecovery
        # #1554: per-profile records from the pre-build incremental-cache cap enforcement. null on a
        # run that never reached the enforcement point (early validation failure); [] when no
        # incremental dir existed yet. A record with pruned=true is a continuous-cap prune, the
        # intraday enforcement the once-daily GC could not provide.
        incremental_cap = $incrementalCap
        attempts = @($attempts)
        fsv_root = $reportRoot
        manifest = $manifest
        target_dir = $targetDir
        stdout_path = $stdoutPath
        stderr_path = $stderrPath
        stdout_tail = Get-LogTail -Path $stdoutPath
        stderr_tail = Get-LogTail -Path $stderrPath
        process_tree_before_stop = @($processTreeBeforeStop)
        process_tree_after_stop = @($processTreeAfterStop)
        process_stop_errors = @($processStopErrors)
        runtime_environment = [ordered]@{
            calyx_home = $env:CALYX_HOME
            ort_dylib_path = $env:ORT_DYLIB_PATH
            ort_dylib_sha256 = if ($env:ORT_DYLIB_PATH -and (Test-Path -LiteralPath $env:ORT_DYLIB_PATH -PathType Leaf)) {
                (Get-FileHash -LiteralPath $env:ORT_DYLIB_PATH -Algorithm SHA256).Hash.ToLowerInvariant()
            } else {
                $null
            }
        }
        target_cache = $targetCache
        target_cache_error = $targetCacheError
        started_at_utc = $startedAtUtc.ToString("O")
        ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
    }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    Write-Host "TEST_GATE_REPORT_READBACK path=$reportPath status=$($readback.status) exit_code=$($readback.exit_code) error_code=$($readback.error_code) timed_out=$($readback.timed_out) elapsed_ms=$($readback.elapsed_ms)"
    $script:reportWritten = $true
}

function Update-TestGateCacheReport {
    $report = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    $report.target_cache = $targetCache
    $report.target_cache_error = $targetCacheError
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    # target_cache is only populated when an explicit -TargetDir was given (the gate owns that
    # tree). Without one, cargo builds into CARGO_TARGET_DIR / <repo>\target, which the gate does
    # NOT own and must not report on -- target_cache is legitimately null there. Dereferencing
    # .path on that null threw TEST_GATE_CACHE_REPORT_ERROR from the `finally` block AFTER the
    # tests had already passed, turning a green run red: a false-red of exactly the kind #1536
    # exists to eliminate. Guard the null instead of assuming the property is always there.
    if ($null -eq $readback.target_cache) {
        Write-Host "TEST_GATE_TARGET_CACHE_READBACK target_cache=none reason=no-TargetDir-supplied (gate does not own the cargo target tree)"
        return
    }
    Write-Host "TEST_GATE_TARGET_CACHE_READBACK path=$($readback.target_cache.path) existed=$($readback.target_cache.existed) retained=$($readback.target_cache.retained) removed=$($readback.target_cache.removed) exists_after=$($readback.target_cache.exists_after)"
}

try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $errorCode = "TEST_GATE_CARGO_NOT_FOUND"
        throw "$($errorCode): cargo was not found on PATH"
    }
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        $errorCode = "TEST_GATE_MANIFEST_NOT_FOUND"
        throw "$($errorCode): manifest not found: $manifest"
    }
    if ($Examples -and $Examples.Count -gt 0 -and $TestFilter) {
        $errorCode = "TEST_GATE_INVALID_TARGET_SELECTION"
        throw "$($errorCode): Examples and TestFilter cannot be combined"
    }
    if ($LockTimeoutSeconds -lt 1) {
        $errorCode = "TEST_GATE_INVALID_LOCK_TIMEOUT"
        throw "$($errorCode): LockTimeoutSeconds must be >= 1"
    }
    if ($TestTimeoutSeconds -lt 1) {
        $errorCode = "TEST_GATE_INVALID_TEST_TIMEOUT"
        throw "$($errorCode): TestTimeoutSeconds must be >= 1"
    }

    $lockStopwatch.Start()
    try {
        $acquired = $mutex.WaitOne([TimeSpan]::FromSeconds($LockTimeoutSeconds))
    } catch [System.Threading.AbandonedMutexException] {
        $acquired = $true
        $errorCode = "TEST_GATE_ABANDONED_MUTEX"
        throw "$($errorCode): previous process abandoned $mutexName; refusing to run against potentially inconsistent Cargo artifacts"
    }
    $lockStopwatch.Stop()
    if (-not $acquired) {
        $errorCode = "TEST_GATE_LOCK_TIMEOUT"
        throw "$($errorCode): waited ${LockTimeoutSeconds}s for $mutexName"
    }

    $argumentLine = (($commandArgs | ForEach-Object { ConvertTo-CommandLineArgument -Value $_ }) -join " ")
    Write-Host "TEST_GATE_LOCK_ACQUIRED name=$mutexName wait_ms=$($lockStopwatch.ElapsedMilliseconds)"
    Write-Host "TEST_GATE_START cwd=$repoRoot command=cargo $($commandArgs -join ' ') timeout_seconds=$TestTimeoutSeconds"

    # The cache cargo will actually READ, which is what recovery must clear (#1536).
    $effectiveTargetDir = Resolve-LitdCargoTargetDir -RepoRoot $repoRoot -TargetDir $targetDir

    # #1554: enforce the incremental-cache cap HERE, on the hot path, BEFORE launching cargo. This is
    # the point of growth and the only place the cap can hold intraday -- the once-daily GC let
    # target\<profile>\incremental reach 50 GB / 476 dirs between runs. We hold the gate mutex for this
    # target tree, so pruning now cannot race the build we are about to start; above EITHER cap the
    # whole incremental dir is removed (deps\ untouched -> one non-incremental build) with a LOUD
    # INCREMENTAL_CACHE_PRUNED log so the prune is visible and counted, never silent. Composes with --
    # never replaces -- the #1536 surgical per-crate recovery that runs AFTER a failed build.
    #
    # -SkipIfInUse ONLY when no explicit -TargetDir was given. With -TargetDir the gate mutex is scoped
    # to THAT tree (see $gateScope above), so we already have exclusive ownership and the guard is
    # redundant -- worse, the explicit -TargetDir <path> sits on this process's and its launcher's
    # command lines, which the in-use check would match and then wrongly skip EVERY run. Without
    # -TargetDir the effective tree is the shared <repo>\target (or CARGO_TARGET_DIR) that raw,
    # non-gated cargo runs also write and the mutex does not fence, so there the guard is real
    # protection against wiping a tree mid-build (and the path is NOT on our command line, so it does
    # not false-match).
    $incCapBytes = [long]$IncrementalMaxSizeGB * 1GB
    $capParams = @{
        TargetDir      = $effectiveTargetDir
        MaxBytes       = $incCapBytes
        MaxDirCount    = $IncrementalMaxDirCount
        Trigger        = "test-gate:$Package"
    }
    if (-not $targetDir) { $capParams["SkipIfInUse"] = $true }
    try {
        $incrementalCap = @(Invoke-LitdIncrementalCacheCap @capParams)
    } catch {
        # The cap is a hygiene guard, not a correctness gate: a failure to measure/prune it must
        # never turn a passing test run red. Record and continue to the build.
        Write-Host "INCREMENTAL_CACHE_CAP_ERROR package=$Package target_dir=$effectiveTargetDir message=$($_.Exception.Message)"
        $incrementalCap = @()
    }

    # #1536: run cargo, and if -- and ONLY if -- it dies with the rustc stale-incremental-work-product
    # link signature (an undefined symbol carrying an LLVM `.llvm.<n>` ThinLTO discriminator), clear
    # the implicated crate's incremental cache and retry EXACTLY once. Any other failure -- a real
    # compile error, a real link error, a failing test -- falls through to the normal red on the
    # first attempt, untouched. See IncrementalCacheRecovery.ps1 for the full mechanism.
    for ($attempt = 1; $attempt -le $maxCargoAttempts; $attempt++) {
        if ($attempt -gt 1) {
            $stdoutPath = Join-Path $reportRoot ("test-rust-package-{0}-{1}.attempt{2}.stdout.log" -f $reportPackage, $timestamp, $attempt)
            $stderrPath = Join-Path $reportRoot ("test-rust-package-{0}-{1}.attempt{2}.stderr.log" -f $reportPackage, $timestamp, $attempt)
        }
        Write-Host "TEST_GATE_LOGS attempt=$attempt stdout=$stdoutPath stderr=$stderrPath"

        try {
            $jobHandle = New-KillOnCloseJob
            $testStopwatch.Restart()
            $process = Start-Process `
                -FilePath "cargo" `
                -ArgumentList $argumentLine `
                -WorkingDirectory $repoRoot `
                -RedirectStandardOutput $stdoutPath `
                -RedirectStandardError $stderrPath `
                -WindowStyle Hidden `
                -PassThru
            $processId = [int]$process.Id
            [void]$knownProcessIds.Add($processId)
            # Assign cargo to the job immediately. cargo parses args and reads Cargo.toml before it
            # ever spawns rustc, so the assignment lands before any child exists; every child
            # thereafter is born inside the job and cannot escape it.
            if (-not [LitdGate.JobNative]::AssignProcessToJobObject($jobHandle, $process.Handle)) {
                $errorCode = "TEST_GATE_JOB_ASSIGN_FAILED"
                throw "$($errorCode): AssignProcessToJobObject win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
            }
            Write-Host "TEST_GATE_PROCESS_STARTED pid=$processId job=assigned attempt=$attempt"
        } catch {
            if ($testStopwatch.IsRunning) {
                $testStopwatch.Stop()
            }
            if (-not $errorCode) { $errorCode = "TEST_GATE_CARGO_LAUNCH_FAILED" }
            throw "$($errorCode): $($_.Exception.Message)"
        }

        # Single blocking wait -- no polling. WaitForExit returns false on timeout.
        $completed = $process.WaitForExit([int]([Math]::Min([long]$TestTimeoutSeconds * 1000, [int]::MaxValue)))
        if (-not $completed) {
            $timedOut = $true
            $errorCode = "TEST_GATE_CARGO_TIMEOUT"
            # One-shot snapshot of the surviving tree for the report, THEN kill the whole job atomically.
            $processTreeBeforeStop = @(Get-TrackedProcessTree -RootIds @($processId) -KnownIds $knownProcessIds)
            if (-not [LitdGate.JobNative]::TerminateJobObject($jobHandle, 1)) {
                $processStopErrors = @([pscustomobject]@{ error = "TerminateJobObject win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())" })
            }
            $process.WaitForExit(5000) | Out-Null
            Start-Sleep -Milliseconds 500
            $processTreeAfterStop = @(Get-TrackedProcessTree -RootIds @($processId) -KnownIds $knownProcessIds)
            if ($testStopwatch.IsRunning) {
                $testStopwatch.Stop()
            }
            # This kill is an INTERRUPTED cargo -- the exact trigger of rust-lang/rust#81280. Say so,
            # loudly, so the causal chain is legible if the NEXT run of this tree links badly. We do
            # not pre-emptively wipe the cache here: the interruption only MIGHT have half-written a
            # work product, and the detector above repairs the actual damage surgically (one crate)
            # on the next run at zero cost when there is none. A blanket wipe would tax every crate
            # in this shared warm tree for a maybe.
            Write-Host "TEST_GATE_CARGO_INTERRUPTED pid=$processId target_dir=$effectiveTargetDir reason=timeout note=interrupted-cargo-can-leave-a-stale-incremental-work-product-rust-lang/rust#81280"
            if ($processTreeAfterStop.Count -gt 0) {
                $errorCode = "TEST_GATE_CARGO_TIMEOUT_CLEANUP_FAILED"
                throw "$($errorCode): timeout after ${TestTimeoutSeconds}s; remaining_processes=$($processTreeAfterStop.Count) report=$reportPath"
            }
            throw "$($errorCode): timeout after ${TestTimeoutSeconds}s; stopped_processes=$($processTreeBeforeStop.Count) report=$reportPath"
        }

        $process.Refresh()
        $exitCode = $process.ExitCode
        if ($testStopwatch.IsRunning) {
            $testStopwatch.Stop()
        }
        if ($jobHandle -ne [IntPtr]::Zero) {
            [LitdGate.JobNative]::CloseHandle($jobHandle) | Out-Null
            $jobHandle = [IntPtr]::Zero
        }
        [void]$attempts.Add([pscustomobject]@{
                attempt = $attempt
                exit_code = $exitCode
                elapsed_ms = $testStopwatch.ElapsedMilliseconds
                stdout_path = $stdoutPath
                stderr_path = $stderrPath
            })

        if ($exitCode -eq 0) {
            break
        }

        $corruption = Get-LitdIncrementalCorruption -Paths @($stderrPath, $stdoutPath) -Package $Package
        if (-not $corruption) {
            # A genuine failure. Unchanged behaviour: red on the first attempt, no retry, no clear.
            $errorCode = "TEST_GATE_CARGO_FAILED"
            Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "cargo exited with $exitCode"
            throw "TEST_GATE_FAILED: exit_code=$exitCode report=$reportPath"
        }

        $symbolList = ($corruption.symbols -join ",")
        $crateList = ($corruption.crates -join ",")
        Write-Host "INCREMENTAL_CACHE_CORRUPTION_DETECTED attempt=$attempt package=$Package crates=$crateList matches=$($corruption.match_count) symbols=$symbolList"
        foreach ($line in $corruption.matched_lines) {
            Write-Host "INCREMENTAL_CACHE_CORRUPTION_EVIDENCE $line"
        }

        if ($attempt -ge $maxCargoAttempts) {
            # RECURRENCE. The cache was already cleared once this run and the SAME corruption came
            # straight back. That is not a stale artifact -- the cache was empty. It is a real bug:
            # a genuinely broken source tree, a miscompiling toolchain, a failing disk, or another
            # process writing into this target tree underneath us. Retrying again would only spin
            # and normalize it, so go RED and hand over the whole diagnostic.
            $errorCode = "TEST_GATE_INCREMENTAL_CACHE_CORRUPTION_RECURRED"
            $recurrenceMessage = (
                "the rustc stale-incremental link signature (undefined symbol with an LLVM " +
                ".llvm.<n> ThinLTO discriminator) RECURRED after the incremental cache for " +
                "crate(s) $crateList was already cleared once this run. An empty cache cannot " +
                "produce a stale work product, so this is NOT the recoverable #1536 corruption: " +
                "treat it as a real defect. symbols=$symbolList"
            )
            Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage $recurrenceMessage
            Write-Host ""
            Write-Host "INCREMENTAL_CACHE_CORRUPTION_RECURRED package=$Package crates=$crateList cleared_already=1 target_dir=$effectiveTargetDir"
            Write-Host "  WHAT FAILED : cargo exited $exitCode with the stale-work-product link signature TWICE in one run."
            Write-Host "                Attempt 1 hit it, the gate cleared incremental\<crate>-* for $crateList, and attempt 2 hit it AGAIN"
            Write-Host "                against a cache that was empty. The cache is therefore NOT the cause."
            Write-Host "  HOW TO FIX  : this is a real bug, not a flake. In order:"
            Write-Host "                1. Confirm the tree is clean       : git status --short"
            Write-Host "                2. Confirm no other process is writing into the target tree:"
            Write-Host "                   Get-CimInstance Win32_Process | Where-Object { `$_.CommandLine -like '*$effectiveTargetDir*' }"
            Write-Host "                3. Full cold rebuild of THIS tree  : cargo clean --target-dir $effectiveTargetDir; then re-run this gate"
            Write-Host "                4. Still red after a cold build    => a genuine source/toolchain defect. Do NOT clear caches again."
            Write-Host "  EVIDENCE    : attempt1=$(($attempts | Where-Object { $_.attempt -eq 1 }).stderr_path)"
            Write-Host "                attempt2=$stderrPath"
            Write-Host "                report=$reportPath"
            Write-Host ""
            throw "$($errorCode): exit_code=$exitCode crates=$crateList report=$reportPath"
        }

        # First occurrence: surgical clear of ONLY the implicated crates, then exactly one retry.
        $cleared = @(Clear-LitdIncrementalCache -TargetDir $effectiveTargetDir -Crates $corruption.crates)
        $clearedBytes = [long](($cleared | Measure-Object -Property bytes -Sum).Sum)
        $failedClears = @($cleared | Where-Object { -not $_.removed })
        foreach ($entry in $cleared) {
            Write-Host ("INCREMENTAL_CACHE_CORRUPTION_RECOVERED crate={0} cleared={1} bytes={2} removed={3} exists_after={4}" -f `
                    $entry.crate, $entry.path, $entry.bytes, $entry.removed, $entry.exists_after)
        }
        if ($cleared.Count -eq 0) {
            # Nothing to clear means the cache is not what is broken -- do not burn a retry pretending
            # otherwise. Fail now with the real error rather than manufacturing a green on attempt 2.
            $errorCode = "TEST_GATE_INCREMENTAL_CACHE_NOT_FOUND"
            Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "the stale-incremental link signature was detected (crates=$crateList) but NO incremental cache directory exists under $effectiveTargetDir to clear, so the cache cannot be the cause; treat this as a real link error. symbols=$symbolList"
            throw "$($errorCode): crates=$crateList target_dir=$effectiveTargetDir report=$reportPath"
        }
        if ($failedClears.Count -gt 0) {
            $errorCode = "TEST_GATE_INCREMENTAL_CACHE_CLEAR_FAILED"
            $stuck = (($failedClears | ForEach-Object { $_.path }) -join ";")
            Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "could not remove incremental cache dir(s): $stuck -- a retry would just re-read the same stale work product. Close any process holding these files (cargo/rustc/antivirus) and re-run."
            throw "$($errorCode): stuck=$stuck report=$reportPath"
        }

        $incrementalRecovery = [pscustomobject]@{
            detected_on_attempt = $attempt
            package = $Package
            crates = @($corruption.crates)
            symbols = @($corruption.symbols)
            match_count = $corruption.match_count
            matched_lines = @($corruption.matched_lines)
            target_dir = $effectiveTargetDir
            cleared = @($cleared)
            cleared_bytes = $clearedBytes
            cleared_dir_count = $cleared.Count
            upstream_issue = "rust-lang/rust#81280"
            recovered_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
        }
        $ledgerPath = Write-LitdIncrementalCorruptionLedger -FsvRoot $reportRoot -Record $incrementalRecovery
        Write-Host "INCREMENTAL_CACHE_CORRUPTION_LEDGER path=$ledgerPath"
        Write-Host "INCREMENTAL_CACHE_CORRUPTION_RETRY package=$Package crates=$crateList cleared_dirs=$($cleared.Count) cleared_bytes=$clearedBytes next_attempt=$($attempt + 1) of=$maxCargoAttempts"
    }

    if ($exitCode -ne 0) {
        # Defensive: the loop only exits on success, a throw, or exhausting attempts. Never green here.
        $errorCode = "TEST_GATE_CARGO_FAILED"
        Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "cargo exited with $exitCode"
        throw "TEST_GATE_FAILED: exit_code=$exitCode report=$reportPath"
    }
    if ($incrementalRecovery) {
        # The build is green, but it was green only BECAUSE we repaired a corrupt cache. Never let
        # that pass as a routine green -- the recovery must be as visible as a failure would have been.
        Write-Host "TEST_GATE_INCREMENTAL_CACHE_RECOVERY_SUMMARY package=$Package crates=$(($incrementalRecovery.crates) -join ',') cleared_dirs=$($incrementalRecovery.cleared_dir_count) cleared_bytes=$($incrementalRecovery.cleared_bytes) upstream=rust-lang/rust#81280 outcome=recovered-and-green"
    }
    if ($Examples -and $Examples.Count -gt 0) {
        # FSV examples are declared `harness = false`, so `cargo test --example` runs each
        # example's `main()` directly and never emits a libtest "running N tests" line. If one
        # appears, `harness = false` was dropped (the #1497 regression, #1527): the example ran
        # under the libtest harness, executed zero assertions, and exited 0 hollow. A green exit
        # code alone cannot catch this, so fail loud on the harness signature instead of
        # green-lighting a no-op FSV run.
        $harnessInvocations = Get-RustHarnessInvocationCount -Path $stdoutPath
        if ($harnessInvocations -gt 0) {
            $errorCode = "TEST_GATE_HOLLOW_EXAMPLE"
            Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "example(s) ran under the libtest harness ($harnessInvocations 'running N tests' line(s)); harness=false is missing so main() did not execute"
            throw "$($errorCode): examples=$($Examples -join ',') harness_invocations=$harnessInvocations report=$reportPath"
        }
    } else {
        $testExecutionCount = Get-RustTestExecutionCount -Path $stdoutPath
    }
    if ($TestFilter -and $testExecutionCount -eq 0) {
        $errorCode = "TEST_GATE_ZERO_TESTS"
        Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage "test filter matched zero tests"
        throw "$($errorCode): filter=$TestFilter report=$reportPath"
    }
    Write-TestGateReport -Status "passed" -ExitCode $exitCode -ErrorCode $null -ErrorMessage $null
    Write-Host "TEST_GATE_OK report=$reportPath"
} catch {
    if (-not $reportWritten) {
        if (-not $errorCode) {
            $errorCode = "TEST_GATE_UNHANDLED_ERROR"
        }
        Write-TestGateReport -Status "failed" -ExitCode $exitCode -ErrorCode $errorCode -ErrorMessage $_.Exception.Message
    }
    throw
} finally {
    $cleanupFailure = $null
    if ($lockStopwatch.IsRunning) {
        $lockStopwatch.Stop()
    }
    if ($testStopwatch.IsRunning) {
        $testStopwatch.Stop()
    }
    if ($jobHandle -ne [IntPtr]::Zero) {
        [LitdGate.JobNative]::CloseHandle($jobHandle) | Out-Null
        $jobHandle = [IntPtr]::Zero
    }
    # RETAIN the target tree -- it is a stable, shared, warm incremental build cache, NOT
    # per-run scratch. Wiping it on exit (as the gate did through #1498) forced a full cold
    # rebuild of wgpu + naga + rapier + 40+ Calyx crates at O3 on EVERY run; keeping it lets
    # cargo fingerprint-skip the unchanged 95% of the graph so the next run is incremental.
    # Disk is bounded out-of-band by scripts/prune-build-artifacts.ps1 (cargo-sweep --maxsize),
    # which is the correct layer for build-cache hygiene -- not the hot path. Recording the
    # retained cache status is a cheap Test-Path; it never fails a gate whose tests passed.
    if ($acquired -and $targetDir) {
        try {
            $targetCache = Get-LitdOwnedTargetCacheStatus -RepoRoot $repoRoot -TargetDir $targetDir
        } catch {
            $targetCacheError = $_.Exception.Message
        }
    }
    if ($reportWritten) {
        try {
            Update-TestGateCacheReport
        } catch {
            if (-not $cleanupFailure) {
                $cleanupFailure = $_.Exception
            }
        }
    }
    if ($acquired) {
        $mutex.ReleaseMutex()
        Write-Host "TEST_GATE_LOCK_RELEASED name=$mutexName"
    }
    $mutex.Dispose()
    if ($null -eq $oldFsvRoot) {
        Remove-Item Env:\CALYX_FSV_ROOT -ErrorAction SilentlyContinue
    } else {
        $env:CALYX_FSV_ROOT = $oldFsvRoot
    }
    if ($cleanupFailure) {
        throw "TEST_GATE_CACHE_REPORT_ERROR: $($cleanupFailure.Message) report=$reportPath"
    }
}
