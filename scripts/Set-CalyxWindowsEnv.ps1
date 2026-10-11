[CmdletBinding()]
param(
    [string]$CalyxHome = (Join-Path $PSScriptRoot "..\Calyx"),

    [string]$CudaRoot = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.3",

    [string]$OrtVenv = "",

    [switch]$RequireCuVS,

    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "CalyxCanonicalPaths.ps1")
$isNativeWindows = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)

function Resolve-RequiredPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Label,

        [ValidateSet("Any", "Container", "Leaf")]
        [string]$Type = "Any"
    )

    $resolved = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
    $pathType = switch ($Type) {
        "Container" { "Container" }
        "Leaf" { "Leaf" }
        default { "Any" }
    }
    if (-not (Test-Path -LiteralPath $resolved -PathType $pathType)) {
        throw "CALYX_WINDOWS_ENV_PATH_MISSING: $Label not found at $resolved"
    }
    $resolved
}

function Find-MsvcCl {
    if ($env:NVCC_CCBIN) {
        $bindir = Resolve-RequiredPath -Path $env:NVCC_CCBIN -Label "NVCC_CCBIN" -Type Container
        $cl = Join-Path $bindir "cl.exe"
        Resolve-RequiredPath -Path $cl -Label "cl.exe from NVCC_CCBIN" -Type Leaf | Out-Null
        return $cl
    }

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $installRoots = @()
    if (Test-Path -LiteralPath $vswhere -PathType Leaf) {
        $found = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0) {
            throw "CALYX_WINDOWS_ENV_VSWHERE_FAILED: vswhere exited $LASTEXITCODE"
        }
        $installRoots += @($found | Where-Object { $_ })
    }
    $programFiles = $env:ProgramFiles
    if ($programFiles) {
        $installRoots += Get-ChildItem -LiteralPath (Join-Path $programFiles "Microsoft Visual Studio") -Directory -ErrorAction SilentlyContinue |
            Get-ChildItem -Directory -ErrorAction SilentlyContinue |
            Select-Object -ExpandProperty FullName
    }

    $candidates = foreach ($root in ($installRoots | Select-Object -Unique)) {
        Get-ChildItem -LiteralPath (Join-Path $root "VC\Tools\MSVC") -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            ForEach-Object { Join-Path $_.FullName "bin\HostX64\x64\cl.exe" } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }
    }
    $selected = $candidates | Select-Object -First 1
    if (-not $selected) {
        throw "CALYX_WINDOWS_ENV_MSVC_NOT_FOUND: install Visual Studio Build Tools with Desktop development with C++ or set NVCC_CCBIN to HostX64\x64"
    }
    $selected
}

function Add-PathPrefix {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Paths
    )

    $existing = @()
    if ($env:PATH) {
        $existing = $env:PATH -split ";"
    }
    $prefix = $Paths | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) } | Select-Object -Unique
    $env:PATH = (($prefix + $existing) | Where-Object { $_ } | Select-Object -Unique) -join ";"
}

function Add-MsvcClOption {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Option
    )

    $existing = @()
    if ($env:CL) {
        $existing = $env:CL -split "\s+" | Where-Object { $_ }
    }
    if ($existing -notcontains $Option) {
        $env:CL = (($existing + @($Option)) | Where-Object { $_ }) -join " "
    }
}

$runtimeArgs = @{
    CalyxHome = $CalyxHome
    PassThru = $true
}
if ($OrtVenv) {
    $runtimeArgs.OrtVenv = $OrtVenv
}
$runtime = & (Join-Path $PSScriptRoot "Set-GameEditorCalyxRuntimeEnv.ps1") @runtimeArgs
$calyxHomeResolved = $runtime.calyx_home
$ortVenvResolved = $runtime.ort_venv
$cudaRootResolved = Resolve-RequiredPath -Path $CudaRoot -Label "CudaRoot" -Type Container
$ortCapi = $runtime.calyx_ort_capi
$ortDll = $runtime.ort_dylib_path
$ortDllVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($ortDll)
$ortScripts = Resolve-RequiredPath -Path (Join-Path $ortVenvResolved "Scripts") -Label "ORT venv Scripts" -Type Container
$cmake = Resolve-RequiredPath -Path (Join-Path $ortScripts "cmake.exe") -Label "CMake executable" -Type Leaf
$nvcc = Resolve-RequiredPath -Path (Join-Path $cudaRootResolved "bin\nvcc.exe") -Label "nvcc" -Type Leaf
$cl = Find-MsvcCl
$nvccCcbin = Split-Path -Parent $cl
$nvidiaRoot = Join-Path $ortVenvResolved "Lib\site-packages\nvidia"

$nvidiaBins = @(
    "cu13\bin",
    "cu13\bin\x86_64",
    "cu13\nvvm\bin",
    "cu13\lib\x64",
    "cublas\bin",
    "cuda_cupti\bin",
    "cuda_nvcc\bin",
    "cuda_nvcc\nvvm\bin",
    "cuda_nvrtc\bin",
    "cuda_runtime\bin",
    "cudnn\bin",
    "cufft\bin",
    "curand\bin",
    "cusolver\bin",
    "cusparse\bin",
    "nvjitlink\bin",
    "nvrtc\bin"
) | ForEach-Object { Join-Path $nvidiaRoot $_ }
$nvidiaDllDirs = @(
    $nvidiaBins |
        Where-Object { Test-Path -LiteralPath $_ -PathType Container } |
        Select-Object -Unique
)

$env:CALYX_NVIDIA_DLL_DIRS = ($nvidiaDllDirs -join [System.IO.Path]::PathSeparator)
$env:CUDA_PATH = $cudaRootResolved
$env:CUDA_ROOT = $cudaRootResolved
$env:CUDA_ARCH = "sm_120"
$env:CUDA_COMPUTE_CAP = "120"
$env:NVCC_CCBIN = $nvccCcbin
$env:VIRTUAL_ENV = $ortVenvResolved
Add-MsvcClOption -Option "/Zc:preprocessor"

Add-PathPrefix -Paths (@(
    $nvccCcbin,
    $ortScripts,
    $ortCapi,
    (Join-Path $cudaRootResolved "bin"),
    (Join-Path $cudaRootResolved "bin\x64")
) + $nvidiaBins)

if ($RequireCuVS) {
    if ($isNativeWindows) {
        throw "CALYX_WINDOWS_ENV_CUVS_UNSUPPORTED: upstream RAPIDS/cuVS does not provide native Windows C/C++ libcuvs/libcuvs_c CMake packages; do not request -RequireCuVS for the Windows product"
    }
    $python = Resolve-RequiredPath -Path (Join-Path $ortScripts "python.exe") -Label "Python executable" -Type Leaf
    $probe = @'
from importlib.util import find_spec
import sys
spec = find_spec(sys.argv[1])
if spec is None:
    sys.exit(2)
locations = spec.submodule_search_locations
print(next(iter(locations)) if locations else spec.origin)
'@
    $cuvsPath = & $python -c $probe "libcuvs"
    if ($LASTEXITCODE -ne 0 -or -not $cuvsPath) {
        throw "CALYX_WINDOWS_ENV_CUVS_MISSING: libcuvs is not installed in $ortVenvResolved; cuvs-sys 26.6.0 requires a matching cuVS 26.6 C/C++ CMake package"
    }
    $env:LIBCUVS_USE_PYTHON = "1"
}

$readback = [ordered]@{
    calyx_home = $env:CALYX_HOME
    calyx_hot_path = $env:CALYX_HOT_PATH
    calyx_cold_path = $env:CALYX_COLD_PATH
    hf_home = $env:HF_HOME
    ort_dylib_path = $env:ORT_DYLIB_PATH
    ort_dylib_sha256 = (Get-FileHash -LiteralPath $env:ORT_DYLIB_PATH -Algorithm SHA256).Hash.ToLowerInvariant()
    ort_dylib_file_version = $ortDllVersion.FileVersion
    ort_dylib_product_version = $ortDllVersion.ProductVersion
    calyx_ort_lib_dir = $env:CALYX_ORT_LIB_DIR
    calyx_ort_capi = $env:CALYX_ORT_CAPI
    calyx_nvidia_dll_dirs = $env:CALYX_NVIDIA_DLL_DIRS
    nvidia_dll_dir_count = $nvidiaDllDirs.Count
    cuda_path = $env:CUDA_PATH
    cuda_arch = $env:CUDA_ARCH
    cuda_compute_cap = $env:CUDA_COMPUTE_CAP
    nvcc = $nvcc
    nvcc_ccbin = $env:NVCC_CCBIN
    cl_env = $env:CL
    cl = $cl
    cmake = $cmake
    virtual_env = $env:VIRTUAL_ENV
    cuvs_required = [bool]$RequireCuVS
    cuvs_native_windows_policy = "cuda-build-enabled-cuvs-ffi-disabled"
    path_prefix_count = (($env:PATH -split ";") | Where-Object { $_ } | Select-Object -First 24).Count
}

Write-Host "CALYX_WINDOWS_ENV_OK calyx_home=$($readback.calyx_home) ort_sha256=$($readback.ort_dylib_sha256) nvcc=$($readback.nvcc) cl=$($readback.cl) cmake=$($readback.cmake)"
if ($PassThru) {
    [PSCustomObject]$readback
}
