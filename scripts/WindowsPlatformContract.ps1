Set-StrictMode -Version Latest

function Assert-LitdWindowsPlatform {
    param([string]$Platform = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription)
    if ($Platform -notmatch '(?i)windows') {
        throw "LITD_WINDOWS_PLATFORM_REQUIRED: unsupported platform '$Platform'; GameEditor runtime and tooling require native Windows"
    }
}

function Assert-LitdWindowsPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Label,
        [switch]$RequireLeaf
    )
    if ($Path -match '^(?i)(/|\\\\wsl(?:\$|\.localhost)\\|[A-Z]:\\mnt\\)') {
        throw "LITD_WINDOWS_PATH_REQUIRED: $Label uses a Unix/WSL path: $Path"
    }
    if ($RequireLeaf -and -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "LITD_WINDOWS_RUNTIME_MISSING: $Label not found at $Path"
    }
}
