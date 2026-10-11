[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path,
    [string]$CalyxHome = (Join-Path $PSScriptRoot "..\Calyx")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Set-CalyxWindowsEnv.ps1") -CalyxHome $CalyxHome | Out-Host
if ($env:NVCC_CCBIN) {
    $env:FORGE_CUDA_CCBIN = $env:NVCC_CCBIN
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$fsvRoot = Join-Path $RepoRoot "target\fsv\issue694_forge_sm120\$stamp"
$logs = Join-Path $fsvRoot "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null
$env:CALYX_FSV_ROOT = $fsvRoot

$calyxDir = $env:CALYX_HOME

function Invoke-CapturedCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Command,
        [string]$WorkingDirectory = $calyxDir
    )

    $logPath = Join-Path $logs "$Name.log"
    Push-Location $WorkingDirectory
    try {
        $output = & $Command[0] @($Command[1..($Command.Count - 1)]) 2>&1 |
            Tee-Object -FilePath $logPath
        $output | Out-Host
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    if ($null -eq $exitCode) {
        $exitCode = 0
    }
    if ($exitCode -ne 0) {
        throw "ISSUE694_FSV_COMMAND_FAILED: name=$Name exit=$exitCode log=$logPath"
    }
    [pscustomobject]@{
        name = $Name
        exit_code = $exitCode
        log_path = $logPath
        log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $logPath).Hash.ToLowerInvariant()
    }
}

function Find-ForgedKernelDir {
    $buildRoot = Join-Path $calyxDir "target\debug\build"
    $dirs = Get-ChildItem -LiteralPath $buildRoot -Directory -Filter "calyx-forge-*" -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "out\forge-cuda-kernels" } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Container } |
        Sort-Object { (Get-Item -LiteralPath $_).LastWriteTimeUtc } -Descending
    $selected = $dirs | Select-Object -First 1
    if (-not $selected) {
        throw "ISSUE694_FSV_KERNEL_DIR_MISSING: no forge-cuda-kernels directory under $buildRoot"
    }
    $selected
}

function Read-PtxTarget {
    param([Parameter(Mandatory = $true)][string]$Path)

    $line = Select-String -LiteralPath $Path -Pattern "^\.target\s+sm_120" -SimpleMatch:$false |
        Select-Object -First 1
    if ($null -eq $line) {
        throw "ISSUE694_FSV_PTX_NOT_SM120: $Path missing .target sm_120"
    }
    $line.Line.Trim()
}

function Get-KernelReadback {
    $kernelDir = Find-ForgedKernelDir
    $files = Get-ChildItem -LiteralPath $kernelDir -File |
        Where-Object { $_.Extension -in @(".ptx", ".cubin") } |
        Sort-Object Name
    if (($files | Where-Object Extension -eq ".ptx").Count -lt 3) {
        throw "ISSUE694_FSV_PTX_COUNT: expected at least 3 PTX files in $kernelDir"
    }
    if (($files | Where-Object Extension -eq ".cubin").Count -lt 3) {
        throw "ISSUE694_FSV_CUBIN_COUNT: expected at least 3 CUBIN files in $kernelDir"
    }

    [pscustomobject]@{
        kernel_dir = $kernelDir
        files = @($files | ForEach-Object {
            $entry = [ordered]@{
                name = $_.Name
                path = $_.FullName
                bytes = $_.Length
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            }
            if ($_.Extension -eq ".ptx") {
                $entry.ptx_target = Read-PtxTarget -Path $_.FullName
            }
            [pscustomobject]$entry
        })
    }
}

$gpuQuery = Invoke-CapturedCommand -Name "nvidia-smi-gpu-readback" -WorkingDirectory $RepoRoot -Command @(
    "nvidia-smi",
    "--query-gpu=name,compute_cap,memory.total,driver_version",
    "--format=csv,noheader"
)

$contextTests = Invoke-CapturedCommand -Name "cargo-test-cuda-context" -Command @(
    "cargo", "test", "-p", "calyx-forge", "--features", "cuda",
    "cuda::context::tests", "--", "--nocapture"
)

$parityTests = Invoke-CapturedCommand -Name "cargo-test-cuda-parity" -Command @(
    "cargo", "test", "-p", "calyx-forge", "--features", "cuda",
    "--test", "cuda_parity", "--", "--nocapture"
)

$kernelReadback = Get-KernelReadback
$cudaReadbacks = Get-ChildItem -LiteralPath $fsvRoot -File -Filter "cuda-*-parity.json" -ErrorAction SilentlyContinue |
    Sort-Object Name |
    ForEach-Object {
        [pscustomobject]@{
            name = $_.Name
            path = $_.FullName
            bytes = $_.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            json = Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
        }
    }

$contextLog = Get-Content -LiteralPath $contextTests.log_path -Raw
$parityLog = Get-Content -LiteralPath $parityTests.log_path -Raw
$requiredLogNeedles = @(
    "compute_capability=12.0",
    "CALYX_FORGE_DEVICE_UNAVAILABLE",
    "golden_gemm_parity PASSED",
    "golden_topk_parity PASSED",
    "cuda_normalize_fail_closed_edges PASSED",
    "perf_vs_cublas PASSED"
)
foreach ($needle in $requiredLogNeedles) {
    if (($contextLog + $parityLog) -notmatch [regex]::Escape($needle)) {
        throw "ISSUE694_FSV_LOG_NEEDLE_MISSING: $needle"
    }
}

$report = [ordered]@{
    issue = 694
    generated_utc = (Get-Date).ToUniversalTime().ToString("o")
    fsv_root = $fsvRoot
    env = [ordered]@{
        calyx_home = $env:CALYX_HOME
        cuda_path = $env:CUDA_PATH
        cuda_arch = $env:CUDA_ARCH
        cuda_compute_cap = $env:CUDA_COMPUTE_CAP
        nvcc_ccbin = $env:NVCC_CCBIN
        forge_cuda_ccbin = $env:FORGE_CUDA_CCBIN
        calyx_fsv_root = $env:CALYX_FSV_ROOT
    }
    commands = @($gpuQuery, $contextTests, $parityTests)
    kernel_readback = $kernelReadback
    cuda_json_readbacks = @($cudaReadbacks)
    manual_sufficiency = "Covers sm_120 build artifacts for distance/topk/mxfp4_gemm PTX+CUBIN, live RTX 5090 compute capability 12.0 context init, real CUDA GEMM/distance/normalize/topk execution against CPU golden outputs, cuBLAS performance sanity, and fail-loud CUDA/device/numerical/shape edges."
}

$reportPath = Join-Path $fsvRoot "issue694-forge-sm120-fsv.json"
$report | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $reportPath -Encoding UTF8
$reportHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant()
Write-Host "ISSUE694_FSV_OK report=$reportPath sha256=$reportHash"
