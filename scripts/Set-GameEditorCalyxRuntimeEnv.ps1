[CmdletBinding()]
param(
    [string]$CalyxHome = "",

    [string]$OrtVenv = "",

    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "CalyxCanonicalPaths.ps1")

# #1543: default to the CANONICAL Calyx root, not "$PSScriptRoot\..\Calyx". A linked git worktree
# has its own tracked Calyx/ copy, but the canonical-root policy in CalyxCanonicalPaths.ps1
# (rightly) forbids alternate roots -- so the old relative default made this script throw
# LITD_CALYX_PATH_OUTSIDE_CANONICAL_ROOT for every agent working in a worktree, which blocked the
# test gate there. The runtime env must point at the one canonical Calyx regardless of checkout.
if (-not $CalyxHome) {
    $CalyxHome = Get-GameEditorCanonicalCalyxHome
}

$calyxHomeResolved = Resolve-GameEditorCalyxHome -Path $CalyxHome
if (-not $OrtVenv) {
    $OrtVenv = Join-Path $calyxHomeResolved "ort-gpu-venv"
}
$ortVenvResolved = Resolve-GameEditorCalyxPath -Path $OrtVenv -Label "OrtVenv" -Type Container
$ortCapi = Resolve-GameEditorCalyxPath `
    -Path (Join-Path $ortVenvResolved "Lib\site-packages\onnxruntime\capi") `
    -Label "ONNX Runtime capi" `
    -Type Container
$ortDll = Resolve-GameEditorCalyxPath `
    -Path (Join-Path $ortCapi "onnxruntime.dll") `
    -Label "ORT_DYLIB_PATH" `
    -Type Leaf
$ortDllVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($ortDll)

$env:CALYX_HOME = $calyxHomeResolved
$env:CALYX_HOT_PATH = Join-Path $calyxHomeResolved "data"
$env:CALYX_COLD_PATH = Join-Path $calyxHomeResolved "archive"
$env:HF_HOME = Join-Path $calyxHomeResolved ".hf-cache"
$env:ORT_DYLIB_PATH = $ortDll
$env:CALYX_ORT_LIB_DIR = $ortCapi
$env:CALYX_ORT_CAPI = $ortCapi

$readback = [ordered]@{
    calyx_home = $env:CALYX_HOME
    calyx_hot_path = $env:CALYX_HOT_PATH
    calyx_cold_path = $env:CALYX_COLD_PATH
    hf_home = $env:HF_HOME
    ort_venv = $ortVenvResolved
    ort_dylib_path = $env:ORT_DYLIB_PATH
    ort_dylib_sha256 = (Get-FileHash -LiteralPath $env:ORT_DYLIB_PATH -Algorithm SHA256).Hash.ToLowerInvariant()
    ort_dylib_file_version = $ortDllVersion.FileVersion
    ort_dylib_product_version = $ortDllVersion.ProductVersion
    calyx_ort_lib_dir = $env:CALYX_ORT_LIB_DIR
    calyx_ort_capi = $env:CALYX_ORT_CAPI
}

Write-Host "CALYX_RUNTIME_ENV_OK calyx_home=$($readback.calyx_home) ort_sha256=$($readback.ort_dylib_sha256)"
if ($PassThru) {
    [PSCustomObject]$readback
}
