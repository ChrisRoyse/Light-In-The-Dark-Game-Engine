param(
    [Parameter(Mandatory = $true)][string]$StageRoot,
    [Parameter(Mandatory = $true)][string]$GamePackagePath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [ValidateRange(1, 120)][int]$WindowTimeoutSeconds = 30,
    [ValidateRange(100, 10000)][int]$ObservationMilliseconds = 1000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
Assert-LitdWindowsPlatform

function Get-RequiredJsonProperty {
    param(
        [object]$Object,
        [string]$Name,
        [string]$Context
    )
    if ($null -eq $Object -or $Object.PSObject.Properties.Name -notcontains $Name) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: $Context is missing '$Name'"
    }
    return $Object.$Name
}

function Get-ValidatedStagePath {
    param(
        [string]$Root,
        [string]$RelativePath
    )
    if (
        [IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -notmatch "^[A-Za-z0-9._/-]+$" -or
        $RelativePath.Split("/") -contains ".."
    ) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: unsafe staged path '$RelativePath'"
    }
    $candidate = Join-Path $Root $RelativePath
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "LITD_WINDOWS_PORTABLE_PAYLOAD_MISSING: staged file not found at $candidate"
    }
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    $prefix = $Root.TrimEnd("\") + "\"
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: staged path escaped root: $RelativePath"
    }
    return $resolved
}

function Assert-StageManifest {
    param(
        [string]$Root,
        [string]$ManifestPath
    )
    try {
        $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
    } catch {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: parse failed at $ManifestPath error=$($_.Exception.Message)"
    }
    if (
        [int](Get-RequiredJsonProperty $manifest "schema_version" "package manifest") -ne 2 -or
        [string](Get-RequiredJsonProperty $manifest "target_os" "package manifest") -cne "windows" -or
        [string](Get-RequiredJsonProperty $manifest "target_arch" "package manifest") -cne "x64"
    ) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: expected schema=2 target=windows/x64"
    }

    $expectedPaths = @(
        "Cargo.lock",
        "README.md",
        "app/litd-editor.exe",
        "app/litd-game.exe",
        "app/onnxruntime.dll",
        "licenses/Ubuntu-Font-License-1.0.txt"
    )
    $fileRows = @(Get-RequiredJsonProperty $manifest "files" "package manifest")
    if ($fileRows.Count -ne $expectedPaths.Count) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: expected file_rows=$($expectedPaths.Count) actual=$($fileRows.Count)"
    }
    $rowsByPath = @{}
    $readback = @()
    foreach ($row in $fileRows) {
        $relative = [string](Get-RequiredJsonProperty $row "path" "file row")
        if ($rowsByPath.ContainsKey($relative)) {
            throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: duplicate file row '$relative'"
        }
        $rowsByPath[$relative] = $row
    }
    foreach ($relative in $expectedPaths) {
        if (-not $rowsByPath.ContainsKey($relative)) {
            throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: missing file row '$relative'"
        }
        $row = $rowsByPath[$relative]
        $path = Get-ValidatedStagePath -Root $Root -RelativePath $relative
        $item = Get-Item -LiteralPath $path
        $sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $expectedBytes = [int64](Get-RequiredJsonProperty $row "bytes" "file row '$relative'")
        $expectedHash = [string](Get-RequiredJsonProperty $row "sha256" "file row '$relative'")
        if ($item.Length -ne $expectedBytes -or $sha256 -cne $expectedHash) {
            throw "LITD_WINDOWS_PORTABLE_PAYLOAD_MISMATCH: path=$relative expected_bytes=$expectedBytes expected_sha256=$expectedHash actual_bytes=$($item.Length) actual_sha256=$sha256"
        }
        $readback += [ordered]@{
            path = $relative
            bytes = $item.Length
            sha256 = $sha256
        }
    }

    $actualRelativePaths = @(
        Get-ChildItem -LiteralPath $Root -File -Recurse |
            ForEach-Object {
                $_.FullName.Substring($Root.Length).TrimStart("\", "/").Replace("\", "/")
            } |
            Sort-Object
    )
    $expectedActualPaths = @($expectedPaths + "package-manifest.json" | Sort-Object)
    if (($actualRelativePaths -join "`n") -cne ($expectedActualPaths -join "`n")) {
        throw "LITD_WINDOWS_PORTABLE_PAYLOAD_MISMATCH: staged file set differs expected='$($expectedActualPaths -join ',')' actual='$($actualRelativePaths -join ',')'"
    }

    $runtimeEnvironment = Get-RequiredJsonProperty $manifest "runtime_environment" "package manifest"
    if (
        [string](Get-RequiredJsonProperty $runtimeEnvironment "ORT_DYLIB_PATH" "runtime environment") -cne
            "app/onnxruntime.dll" -or
        [string](Get-RequiredJsonProperty $runtimeEnvironment "CALYX_ORT_CAPI" "runtime environment") -cne
            "app"
    ) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: bundled ORT environment contract changed"
    }

    $subsystems = @(
        Get-RequiredJsonProperty $manifest "binary_subsystems" "package manifest"
    )
    $subsystemsByPath = @{}
    foreach ($row in $subsystems) {
        $subsystemsByPath[[string]$row.path] = $row
    }
    foreach ($expected in @(
            [pscustomobject]@{ path = "app/litd-editor.exe"; value = 2; name = "windows_gui" }
            [pscustomobject]@{ path = "app/litd-game.exe"; value = 3; name = "windows_console" }
        )) {
        if (-not $subsystemsByPath.ContainsKey($expected.path)) {
            throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: subsystem row missing '$($expected.path)'"
        }
        $row = $subsystemsByPath[$expected.path]
        if ([int]$row.subsystem_value -ne $expected.value -or [string]$row.subsystem -cne $expected.name) {
            throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: subsystem mismatch path=$($expected.path)"
        }
    }

    $ortRows = @(Get-RequiredJsonProperty $manifest "runtime_dependencies" "package manifest")
    if ($ortRows.Count -ne 1 -or [string]$ortRows[0].path -cne "app/onnxruntime.dll") {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: expected one bundled onnxruntime dependency"
    }
    $ortFile = $readback | Where-Object path -CEQ "app/onnxruntime.dll"
    if (
        [int64]$ortRows[0].bytes -ne $ortFile.bytes -or
        [string]$ortRows[0].sha256 -cne $ortFile.sha256
    ) {
        throw "LITD_WINDOWS_PORTABLE_MANIFEST_INVALID: ONNX dependency identity differs from file row"
    }

    return [pscustomobject]@{
        manifest = $manifest
        files = $readback
    }
}

function Stop-OwnedPortableProcess {
    param([Diagnostics.Process]$Process)
    if ($null -eq $Process -or $Process.HasExited) {
        return "already_exited"
    }
    $closed = $Process.CloseMainWindow()
    if ($closed -and $Process.WaitForExit(5000)) {
        return "close_main_window"
    }
    $Process.Kill($true)
    [void]$Process.WaitForExit(15000)
    if (-not $Process.HasExited) {
        throw "LITD_WINDOWS_PORTABLE_PROCESS_CLEANUP_FAILED: pid=$($Process.Id)"
    }
    return "kill_owned_tree"
}

function Invoke-PortableWindowProbe {
    param(
        [string]$Application,
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [string]$OrtDylibPath,
        [string]$CalyxOrtCapi,
        [string]$ExpectedTitlePrefix,
        [int]$TimeoutSeconds,
        [int]$ObservationMilliseconds
    )
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.WorkingDirectory = $WorkingDirectory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.Environment["ORT_DYLIB_PATH"] = $OrtDylibPath
    $start.Environment["CALYX_ORT_CAPI"] = $CalyxOrtCapi
    foreach ($argument in $Arguments) {
        [void]$start.ArgumentList.Add($argument)
    }

    $process = $null
    $shutdown = ""
    try {
        try {
            $process = [Diagnostics.Process]::Start($start)
        } catch {
            throw "LITD_WINDOWS_PORTABLE_LAUNCH_FAILED: application=$Application start_error=$($_.Exception.Message)"
        }
        $started = [DateTimeOffset]::UtcNow
        $deadline = $started.AddSeconds($TimeoutSeconds)
        $handle = [int64]0
        $title = ""
        $startupMilliseconds = -1
        while ([DateTimeOffset]::UtcNow -lt $deadline) {
            if ($process.HasExited) {
                throw "LITD_WINDOWS_PORTABLE_LAUNCH_FAILED: application=$Application pid=$($process.Id) exit=$($process.ExitCode) before_window=true"
            }
            $process.Refresh()
            $handle = [int64]$process.MainWindowHandle
            $title = [string]$process.MainWindowTitle
            if ($handle -ne 0 -and $title.StartsWith($ExpectedTitlePrefix, [StringComparison]::Ordinal)) {
                $startupMilliseconds = [int]([DateTimeOffset]::UtcNow - $started).TotalMilliseconds
                break
            }
            Start-Sleep -Milliseconds 100
        }
        if ($handle -eq 0 -or -not $title.StartsWith($ExpectedTitlePrefix, [StringComparison]::Ordinal)) {
            throw "LITD_WINDOWS_PORTABLE_WINDOW_TIMEOUT: application=$Application pid=$($process.Id) timeout_seconds=$TimeoutSeconds hwnd=$handle title='$title'"
        }
        Start-Sleep -Milliseconds $ObservationMilliseconds
        if ($process.HasExited) {
            throw "LITD_WINDOWS_PORTABLE_LAUNCH_FAILED: application=$Application pid=$($process.Id) exit=$($process.ExitCode) during_observation=true"
        }
        $process.Refresh()
        $observedHandle = [int64]$process.MainWindowHandle
        $observedTitle = [string]$process.MainWindowTitle
        if ($observedHandle -ne $handle -or $observedTitle -cne $title) {
            throw "LITD_WINDOWS_PORTABLE_WINDOW_CHANGED: application=$Application initial_hwnd=$handle observed_hwnd=$observedHandle initial_title='$title' observed_title='$observedTitle'"
        }
        $shutdown = Stop-OwnedPortableProcess -Process $process
        return [pscustomobject]@{
            application = $Application
            executable = $Executable
            executable_sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
            pid = $process.Id
            hwnd = $handle
            title = $title
            startup_milliseconds = $startupMilliseconds
            observation_milliseconds = $ObservationMilliseconds
            shutdown = $shutdown
            exit_code = $process.ExitCode
        }
    } finally {
        if ($null -ne $process) {
            if (-not $process.HasExited) {
                [void](Stop-OwnedPortableProcess -Process $process)
            }
            $process.Dispose()
        }
    }
}

if (-not (Test-Path -LiteralPath $StageRoot -PathType Container)) {
    throw "LITD_WINDOWS_PORTABLE_STAGE_MISSING: stage not found at $StageRoot"
}
if (-not (Test-Path -LiteralPath $GamePackagePath -PathType Leaf)) {
    throw "LITD_WINDOWS_PORTABLE_GAME_PACKAGE_MISSING: game package not found at $GamePackagePath"
}
$stage = (Resolve-Path -LiteralPath $StageRoot).Path.TrimEnd("\")
$gamePackage = (Resolve-Path -LiteralPath $GamePackagePath).Path
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
$outputParent = Split-Path -Parent $output
if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
    throw "LITD_WINDOWS_PORTABLE_OUTPUT_PARENT_MISSING: output directory not found at $outputParent"
}
$manifestPath = Join-Path $stage "package-manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "LITD_WINDOWS_PORTABLE_MANIFEST_MISSING: manifest not found at $manifestPath"
}
$stageReadback = Assert-StageManifest -Root $stage -ManifestPath $manifestPath
$appRoot = Join-Path $stage "app"
$ortDylib = Join-Path $appRoot "onnxruntime.dll"

$editor = Invoke-PortableWindowProbe `
    -Application "editor" `
    -Executable (Join-Path $appRoot "litd-editor.exe") `
    -Arguments @() `
    -WorkingDirectory $appRoot `
    -OrtDylibPath $ortDylib `
    -CalyxOrtCapi $appRoot `
    -ExpectedTitlePrefix "Light in the Dark Editor - world view" `
    -TimeoutSeconds $WindowTimeoutSeconds `
    -ObservationMilliseconds $ObservationMilliseconds
$game = Invoke-PortableWindowProbe `
    -Application "game" `
    -Executable (Join-Path $appRoot "litd-game.exe") `
    -Arguments @("--package", $gamePackage) `
    -WorkingDirectory $appRoot `
    -OrtDylibPath $ortDylib `
    -CalyxOrtCapi $appRoot `
    -ExpectedTitlePrefix "Light in the Dark Runtime - " `
    -TimeoutSeconds $WindowTimeoutSeconds `
    -ObservationMilliseconds $ObservationMilliseconds

$gpuRows = @(
    Get-CimInstance Win32_VideoController |
        Sort-Object PNPDeviceID |
        ForEach-Object {
            [ordered]@{
                name = [string]$_.Name
                vendor = [string]$_.AdapterCompatibility
                driver_version = [string]$_.DriverVersion
                pnp_device_id = [string]$_.PNPDeviceID
                status = [string]$_.Status
            }
        }
)
$gamePackageItem = Get-Item -LiteralPath $gamePackage
$readback = [ordered]@{
    schema = "litd.windows.portable-launch.v1"
    package_version = [string]$stageReadback.manifest.package_version
    git_commit = [string]$stageReadback.manifest.git_commit
    stage_root = $stage
    manifest_sha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    staged_files = $stageReadback.files
    ort_dylib_path = $ortDylib
    ort_dylib_sha256 = (
        Get-FileHash -LiteralPath $ortDylib -Algorithm SHA256
    ).Hash.ToLowerInvariant()
    calyx_ort_capi = $appRoot
    game_package = [ordered]@{
        path = $gamePackage
        bytes = $gamePackageItem.Length
        sha256 = (Get-FileHash -LiteralPath $gamePackage -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    launches = @($editor, $game)
    gpu_adapters = $gpuRows
    vulkan_loader_present = Test-Path -LiteralPath (Join-Path $env:WINDIR "System32\vulkan-1.dll")
}
$readback | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $output -Encoding UTF8
[pscustomobject]$readback
