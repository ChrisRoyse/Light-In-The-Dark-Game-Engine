#requires -Version 5.1
<#
.SYNOPSIS
    5.1-safe capture/stream of a native command's output + real exit code for the gate
    scripts (#1809).

.DESCRIPTION
    ROOT CAUSE (#1809). On Windows PowerShell 5.1, merging a native command's stderr into
    the pipeline with `2>&1` wraps every stderr line in a
    [System.Management.Automation.ErrorRecord] (whose inner exception is a RemoteException,
    FullyQualifiedErrorId=NativeCommandError). With `$ErrorActionPreference = 'Stop'` in
    effect -- which check-format.ps1 and lint.ps1 both set -- the FIRST such record raises a
    SCRIPT-TERMINATING error BEFORE `$LASTEXITCODE` can be read. So a benign line cargo
    replays to stderr on every build (e.g. calyx-forge's "cuda feature not enabled, skipping
    kernel compilation") aborts the gate even though cargo exited 0, misreporting a clean
    build as a compile/lint failure. This is PowerShell issue #4002 / #10512; the intended
    suppressor `$PSNativeCommandUseErrorActionPreference = $false` only exists on PS 7.3+ and
    is a silent no-op on 5.1, so it does NOT protect the box this bug is about (a host with no
    pwsh 7 -- exactly the fallback the pre-commit/pre-push hooks use). Verified on this host:
    `& cmd /c "... 1>&2 ..." 2>&1 | Out-String` under EAP=Stop throws RemoteException on 5.1
    while exiting 0.

    FIX (best practice, per PowerShell/PowerShell#4002 and mklement0's canonical guidance):
      - relax `$ErrorActionPreference` to 'Continue' ONLY around the native call (a `throw`
        statement still terminates; every OTHER cmdlet keeps the caller's Stop preference),
      - render each stderr ErrorRecord back to its raw text via .ToString() so the captured
        output carries clean `error[...]`/`warning:` lines (no "cmd.exe :" / "At line:char"
        chrome that would defeat downstream diagnostic matching), and
      - decide success on `$LASTEXITCODE` ONLY -- never `$?`, which 5.1 flips to $false on any
        stderr line regardless of exit code.

    Dot-source this file to obtain Invoke-GateNativeCommand (capture) and
    Invoke-GateNativeStream (live stream). It defines functions only; it runs nothing at
    import.
#>

Set-StrictMode -Version Latest

function ConvertTo-GateNativeLine {
    # Flatten one pipeline item from a `native 2>&1` merge to raw text: an ErrorRecord
    # (a stderr line) renders via .ToString() to its ORIGINAL message with no host chrome;
    # anything else is a stdout line already.
    param([Parameter(Mandatory = $true)][AllowNull()]$Item)
    if ($null -eq $Item) { return "" }
    if ($Item -is [System.Management.Automation.ErrorRecord]) { return $Item.ToString() }
    return [string]$Item
}

function Invoke-GateNativeCommand {
    <#
      Run a native command capturing merged stdout+stderr as clean text and its real process
      exit code, without a stderr line terminating the caller under EAP=Stop on PS 5.1.
      Returns [pscustomobject]@{ Output = <string>; ExitCode = <int> }. Does not itself throw
      on a non-zero exit -- the caller inspects .ExitCode / .Output and decides.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter()]
        [string[]]$ArgumentList = @()
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    # No-op on 5.1 (variable does not exist there); on 7.3+ it likewise keeps a non-zero
    # native exit from auto-throwing so the caller always owns the exit-code decision.
    $PSNativeCommandUseErrorActionPreference = $false
    try {
        $lines = & $FilePath @ArgumentList 2>&1 | ForEach-Object { ConvertTo-GateNativeLine -Item $_ }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }

    $text = if ($null -eq $lines) { "" } else { ($lines -join [Environment]::NewLine) }
    return [pscustomobject]@{ Output = $text; ExitCode = $code }
}

function Invoke-GateNativeStream {
    <#
      Run a native command STREAMING its merged stdout+stderr to the host live (for long
      commands like clippy where live progress matters), 5.1-safe, returning ONLY the real
      process exit code. Capture nothing.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter()]
        [string[]]$ArgumentList = @()
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $PSNativeCommandUseErrorActionPreference = $false
    try {
        & $FilePath @ArgumentList 2>&1 | ForEach-Object { Write-Host (ConvertTo-GateNativeLine -Item $_) }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    return $code
}
