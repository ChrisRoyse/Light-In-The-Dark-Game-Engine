#requires -Version 5.1
<#
.SYNOPSIS
    Honest RED-vs-INFRA classification for the main-health backstop (#1567).

.DESCRIPTION
    run-main-health-gate.ps1 runs the FULL correctness gate against origin/main and must announce
    the result on always-on channels. The gate's inner exit code alone is NOT enough to tell WHY a
    run was non-zero: the test gate exits 1 both for a genuine correctness failure AND for a
    setup/guard throw (e.g. LitdOwnedTarget rejecting the build dir #1520, or the Calyx/ORT runtime
    env failing to materialize #1493). Keying RED purely on "gate exit != 0" therefore announces an
    INFRA misconfiguration as a broken main -- a false-RED alarm.

    This module is the SINGLE source of that decision, kept as a pure function of (gate exit code,
    captured gate output lines) with NO side effects so it is unit-testable without cargo. It is
    dot-sourced by run-main-health-gate.ps1 and by the #1567 static-verification harness.

        GREEN : gate exit 0.
        INFRA : gate exit != 0 AND the captured output carries a setup/guard marker
                (see $MainHealthInfraMarkerPattern) -- the gate could NOT be run to an honest
                correctness verdict. Announced as INFRA (exit 4), never RED.
        RED   : gate exit != 0 with NONE of those markers -- a real PRE_PUSH_GATE_FAILED at a
                compile/test/lint step. A genuine correctness regression reached main.

    git fetch / worktree / checkout failures happen BEFORE the gate produces output; those are
    caught by run-main-health-gate.ps1's try/catch and already classified INFRA there. This module
    covers the case where the gate DID run and emitted a setup marker into its output.
#>

Set-StrictMode -Version Latest

# Setup / infrastructure markers. Presence of ANY of these in the captured gate output means the
# run hit a setup/guard failure, not a correctness failure. Grounded in the actual markers the
# gate subtree emits:
#   * TEST_GATE_TARGET_*        -- LitdOwnedTarget.ps1 target-ownership guard throws (#1520).
#   * LITD_CALYX_*              -- CalyxCanonicalPaths.ps1 / Set-GameEditorCalyxRuntimeEnv.ps1
#                                  runtime-env + ORT-dll resolution throws (#1543/#1493).
#   * CALYX_LENS_UNREACHABLE    -- the ONNX lens could not be reached at runtime (#1493).
$script:MainHealthInfraMarkerPattern =
    'TEST_GATE_TARGET_(OUTSIDE_REPO|ROOT_FORBIDDEN|REPARSE_ESCAPE|CONTAINS_PROTECTED_PATH|IN_USE|CLEANUP_FAILED)' +
    '|LITD_CALYX_[A-Z_]+' +
    '|CALYX_LENS_UNREACHABLE'

function Get-MainHealthInfraMarker {
    # Return the FIRST setup/guard marker found in $GateLines, or $null when none is present.
    param([string[]]$GateLines)
    foreach ($line in @($GateLines)) {
        if ($null -eq $line) { continue }
        if ($line -match $script:MainHealthInfraMarkerPattern) {
            return $Matches[0]
        }
    }
    return $null
}

function Get-MainHealthClassification {
    # Pure map from a finished gate run to GREEN / RED / INFRA. See the module .DESCRIPTION.
    param(
        [Parameter(Mandatory = $true)][int]$GateExit,
        [string[]]$GateLines
    )
    if ($GateExit -eq 0) {
        return [PSCustomObject]@{ Kind = "GREEN"; InfraMarker = $null }
    }
    $marker = Get-MainHealthInfraMarker -GateLines $GateLines
    if ($marker) {
        return [PSCustomObject]@{ Kind = "INFRA"; InfraMarker = $marker }
    }
    return [PSCustomObject]@{ Kind = "RED"; InfraMarker = $null }
}
