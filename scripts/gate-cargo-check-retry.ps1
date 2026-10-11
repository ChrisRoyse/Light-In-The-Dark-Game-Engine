#requires -Version 5.1
<#
.SYNOPSIS
    Transient-vs-real discrimination + bounded retry for the format gate's
    `cargo check --workspace --all-targets` step (#1563).

.DESCRIPTION
    The full (non -FormatOnly) format gate runs `cargo check --workspace --all-targets`
    against the SHARED warm target cache at C:\code\GameEditor\target. Several agents /
    worktrees build against that one directory, which cargo guards with an EXCLUSIVE
    target\.cargo-lock.

    PROVEN mechanism (rust-lang/cargo, rust-analyzer, HULKs/hulk #721):
      - Cargo does NOT return an error on lock contention. It BLOCKS, printing
        "Blocking waiting for file lock on build directory", and proceeds once it
        acquires the lock. So a plain lock wait never reddens the gate.

    INFERRED mechanism of the bare exit=-1 seen in #1563 (timing-dependent, hard to
    reproduce deterministically, but consistent with the sources above):
      - When a concurrent cargo/rustc against the shared target is aborted/cancelled
        mid-build (agent killed, timeout elsewhere, machine pressure) it can corrupt
        incremental artifacts, and on Windows a transient file handle (AV scan, another
        process) can kill or fail-to-start the rustc/cargo child. PowerShell surfaces such
        an abnormally terminated / not-started native child as a NEGATIVE or non-101 exit
        code with NO rustc diagnostics emitted.

    A GENUINE compile failure is categorically different: rustc/cargo ALWAYS emits
    diagnostics (`error[E0433]:`, `error:`, `could not compile`) and exits 101. That is
    the discriminator this helper keys on -- presence of diagnostics, NOT the exit code
    alone. So:
      - exit 0                                   -> success.
      - non-zero exit WITH diagnostics present   -> REAL failure, fail IMMEDIATELY,
                                                    never retried (diagnostics surfaced).
      - non-zero exit WITHOUT diagnostics        -> TRANSIENT signature: emit a loud,
                                                    greppable GATE_STEP_TRANSIENT_RETRY log
                                                    and retry a bounded number of times
                                                    (cargo blocks on the lock, so a retry
                                                    normally acquires it and succeeds).
                                                    If it STILL fails with no diagnostics
                                                    after the retries, fail HARD and loud.

    This does NOT weaken coverage (still `--workspace --all-targets`) and does NOT swallow
    real errors -- a real `error[...]` fails on the first attempt.

    Dot-source this file to obtain Test-CargoOutputHasCompileError and
    Invoke-CargoCheckStepWithRetry. It defines functions only; it runs nothing at import.
#>

Set-StrictMode -Version Latest

# #1809: 5.1-safe native-command capture. On Windows PowerShell 5.1 `& cargo ... 2>&1 | Out-String`
# under $ErrorActionPreference='Stop' throws a script-terminating NativeCommandError on the FIRST
# stderr line (e.g. calyx-forge's benign "cuda feature not enabled" build-script warning) before
# $LASTEXITCODE can be read. Invoke-GateNativeCommand relaxes EAP only around the native call and
# flattens stderr ErrorRecords to raw text. Dot-sourced (functions only).
$gateNativeHelper = Join-Path $PSScriptRoot "gate-native-command.ps1"
if (-not (Test-Path -LiteralPath $gateNativeHelper -PathType Leaf)) {
    throw "FORMAT_GATE_FAILED: helper not found: $gateNativeHelper"
}
. $gateNativeHelper

function Test-CargoOutputHasCompileError {
    <#
      True IFF the captured cargo/rustc output carries a genuine compiler/cargo error
      diagnostic. A transient process abort / lock-kill / process-start failure emits none
      of these. Warnings (`warning:`) are intentionally NOT treated as errors.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [AllowNull()]
        [string]$Output
    )

    if ([string]::IsNullOrEmpty($Output)) { return $false }

    # rustc coded errors, e.g. "error[E0433]: failed to resolve".
    if ($Output -match '(?m)^\s*error\[E\d+\]') { return $true }
    # rustc/cargo plain errors, e.g. "error: expected one of ...", "error: could not compile".
    if ($Output -match '(?m)^\s*error(\[|:)') { return $true }
    # Cargo's compile-failure summary line.
    if ($Output -match 'could not compile') { return $true }

    return $false
}

function Invoke-CargoCheckStepWithRetry {
    <#
      Run `cargo <CargoArgs>` in $WorkingDirectory, discriminating a REAL compile failure
      (diagnostics present -> fail immediately) from a TRANSIENT abort (no diagnostics ->
      bounded retry). Throws FORMAT_GATE_FAILED on a real error or on a transient signature
      that persists after all attempts. Returns quietly on success.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$WorkingDirectory,

        [Parameter(Mandatory = $true)]
        [string[]]$CargoArgs,

        # Total attempts (1 initial + retries). 3 => up to 2 retries.
        [int]$MaxAttempts = 3,

        # Backoff before each retry; last value reused if fewer entries than retries.
        [int[]]$BackoffSeconds = @(5, 15)
    )

    $commandLine = "cargo " + ($CargoArgs -join " ")
    Write-Host "FORMAT_GATE_START step=$Name cwd=$WorkingDirectory command=$commandLine"

    $attempt = 0
    while ($true) {
        $attempt += 1

        Push-Location -LiteralPath $WorkingDirectory
        try {
            # #1809: capture via the 5.1-safe helper so a stderr line (build-script warning,
            # lock-wait notice) never terminates the gate before we read the real exit code.
            $result = Invoke-GateNativeCommand -FilePath "cargo" -ArgumentList $CargoArgs
            $out = $result.Output
            $code = $result.ExitCode
        } finally {
            Pop-Location
        }

        # Never hide what cargo/rustc printed -- diagnostics (and lock-wait notices) must be
        # visible in the gate log regardless of outcome.
        if (-not [string]::IsNullOrWhiteSpace($out)) {
            Write-Host $out
        }

        if ($code -eq 0) {
            Write-Host "FORMAT_GATE_OK step=$Name attempts=$attempt"
            return
        }

        $hasCompileError = Test-CargoOutputHasCompileError -Output $out

        if ($hasCompileError) {
            # Genuine compile/config failure: rustc/cargo emitted diagnostics (surfaced
            # above). This is the change's fault -- fail LOUD and IMMEDIATELY, never retried.
            throw "FORMAT_GATE_FAILED: step=$Name exit_code=$code reason=compile-error command=$commandLine (rustc/cargo diagnostics present above; genuine failure, NOT retried)"
        }

        # Non-zero exit but NO diagnostics: the transient signature (target-dir lock
        # contention / aborted concurrent build / process could not start).
        if ($attempt -lt $MaxAttempts) {
            $backoffIndex = [Math]::Min($attempt - 1, $BackoffSeconds.Count - 1)
            $sleepSeconds = $BackoffSeconds[$backoffIndex]
            Write-Host "GATE_STEP_TRANSIENT_RETRY step=$Name reason=target-lock-or-process-start exit=$code attempt=$attempt/$MaxAttempts backoff_seconds=$sleepSeconds (no rustc diagnostics emitted; NOT a compile error)"
            Start-Sleep -Seconds $sleepSeconds
            continue
        }

        # Retries exhausted, still the transient signature: fail HARD and loud so a truly
        # stuck condition is never silently green.
        throw "FORMAT_GATE_FAILED: step=$Name exit_code=$code reason=transient-persisted-after-$MaxAttempts-attempts command=$commandLine (no rustc/cargo diagnostics across attempts; likely a stuck concurrent cargo holding target\.cargo-lock or repeated process-start failure -- inspect for a hung cargo/rustc, then re-run)"
    }
}
