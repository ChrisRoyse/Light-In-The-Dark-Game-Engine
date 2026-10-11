param(
    [Parameter(Mandatory = $true)][string]$NewerMsiPath,
    [Parameter(Mandatory = $true)][string]$NewerStageRoot,
    [Parameter(Mandatory = $true)][string]$NewerVersion,
    [Parameter(Mandatory = $true)][string]$OlderMsiPath,
    [Parameter(Mandatory = $true)][string]$OlderStageRoot,
    [Parameter(Mandatory = $true)][string]$OlderVersion,
    [Parameter(Mandatory = $true)][string]$LogRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
. (Join-Path $PSScriptRoot "WindowsPackageContract.ps1")
Assert-LitdWindowsPlatform

$upgradeCode = "{2F3B1D41-B982-48D1-92C1-7F23B45574C9}"
$downgradeMessage = "A newer Light in the Dark build is already installed."
$environmentRegistryPath = "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"

function Assert-LifecycleAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_ADMIN_REQUIRED: run from an elevated 64-bit PowerShell session"
    }
    if (-not [Environment]::Is64BitProcess) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_X64_REQUIRED: run from 64-bit PowerShell"
    }
}

function Resolve-LifecycleLeaf {
    param([string]$Path, [string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_INPUT_MISSING: $Label not found at $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Resolve-LifecycleContainer {
    param([string]$Path, [string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_INPUT_MISSING: $Label not found at $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Get-MsiProperty {
    param([string]$MsiPath, [string]$Name)
    $installer = $null
    $database = $null
    $view = $null
    $record = $null
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.OpenDatabase($MsiPath, 0)
        $view = $database.OpenView(
            "SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$Name'"
        )
        [void]$view.Execute()
        $record = $view.Fetch()
        if ($null -eq $record) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_CONTRACT_INVALID: MSI property '$Name' is missing"
        }
        return [string]$record.StringData(1)
    } catch {
        if ($_.Exception.Message -match "^LITD_WINDOWS_MSI_") {
            throw
        }
        throw "LITD_WINDOWS_MSI_LIFECYCLE_DATABASE_INVALID: property=$Name error=$($_.Exception.Message)"
    } finally {
        if ($null -ne $record) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
        }
        if ($null -ne $view) {
            [void]$view.Close()
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
        }
        if ($null -ne $database) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)
        }
        if ($null -ne $installer) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
        }
    }
}

function Get-RelatedProductCodes {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    try {
        return @($installer.RelatedProducts($upgradeCode))
    } finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
    }
}

function Get-ProductState {
    param([string]$ProductCode)
    $installer = New-Object -ComObject WindowsInstaller.Installer
    try {
        try {
            return [int]$installer.ProductState($ProductCode)
        } catch {
            return -1
        }
    } finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
    }
}

function Get-InstalledProductVersion {
    param([string]$ProductCode)
    $installer = New-Object -ComObject WindowsInstaller.Installer
    try {
        return [string]$installer.ProductInfo($ProductCode, "VersionString")
    } finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
    }
}

function Get-SystemEnvironmentRow {
    param([string]$Name)
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($environmentRegistryPath)
    if ($null -eq $key) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_ENVIRONMENT_INVALID: system environment registry key is missing"
    }
    try {
        $present = $key.GetValueNames() -ccontains $Name
        $value = if ($present) {
            [string]$key.GetValue(
                $Name,
                $null,
                [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
            )
        } else {
            ""
        }
        return [pscustomobject]@{
            name = $Name
            present = $present
            value = $value
        }
    } finally {
        $key.Dispose()
    }
}

function Assert-SystemEnvironmentAbsent {
    foreach ($name in @("ORT_DYLIB_PATH", "CALYX_ORT_CAPI")) {
        $row = Get-SystemEnvironmentRow -Name $name
        if ($row.present) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_PREEXISTING_STATE: system environment '$name' already exists"
        }
    }
}

function Assert-SystemEnvironmentInstalled {
    param([string]$InstallRoot)
    $expected = [ordered]@{
        ORT_DYLIB_PATH = Join-Path $InstallRoot "onnxruntime.dll"
        CALYX_ORT_CAPI = $InstallRoot.TrimEnd("\") + "\"
    }
    foreach ($item in $expected.GetEnumerator()) {
        $row = Get-SystemEnvironmentRow -Name $item.Key
        if (-not $row.present -or $row.value -cne $item.Value) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_ENVIRONMENT_INVALID: name=$($item.Key) expected='$($item.Value)' present=$($row.present) actual='$($row.value)'"
        }
    }
}

function Invoke-LifecycleMsiExec {
    param([string[]]$Arguments)
    $command = (Get-Command "msiexec.exe" -ErrorAction Stop).Source
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $command
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    foreach ($argument in $Arguments) {
        [void]$start.ArgumentList.Add($argument)
    }
    $process = [Diagnostics.Process]::Start($start)
    try {
        $process.WaitForExit()
        return $process.ExitCode
    } finally {
        $process.Dispose()
    }
}

function Invoke-LoggedMsiExec {
    param(
        [string[]]$OperationArguments,
        [string]$LogPath
    )
    if (Test-Path -LiteralPath $LogPath) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_LOG_EXISTS: refusing to overwrite $LogPath"
    }
    $arguments = @($OperationArguments) + @("/qn", "/norestart", "/l*v", $LogPath)
    return Invoke-LifecycleMsiExec -Arguments $arguments
}

function Get-PayloadContract {
    param([string]$StageRoot)
    $paths = [ordered]@{
        "Cargo.lock" = "Cargo.lock"
        "litd-editor.exe" = "app/litd-editor.exe"
        "litd-game.exe" = "app/litd-game.exe"
        "onnxruntime.dll" = "app/onnxruntime.dll"
        "package-manifest.json" = "package-manifest.json"
        "README.md" = "README.md"
        "licenses\Ubuntu-Font-License-1.0.txt" = "licenses/Ubuntu-Font-License-1.0.txt"
    }
    $rows = @()
    foreach ($item in $paths.GetEnumerator()) {
        $source = Join-Path $StageRoot $item.Value
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_INPUT_MISSING: staged payload not found at $source"
        }
        $rows += [pscustomobject]@{
            installed_path = $item.Key
            staged_path = $item.Value
            bytes = (Get-Item -LiteralPath $source).Length
            sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    return @($rows)
}

function Assert-InstalledPayload {
    param(
        [object[]]$Payload,
        [string]$InstallRoot
    )
    $readback = @()
    foreach ($item in $Payload) {
        $path = Join-Path $InstallRoot $item.installed_path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_PAYLOAD_INVALID: installed payload missing at $path"
        }
        $file = Get-Item -LiteralPath $path
        $sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($file.Length -ne $item.bytes -or $sha256 -ne $item.sha256) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_PAYLOAD_INVALID: path=$path expected_bytes=$($item.bytes) expected_sha256=$($item.sha256) actual_bytes=$($file.Length) actual_sha256=$sha256"
        }
        $readback += [ordered]@{
            path = $item.installed_path
            bytes = $file.Length
            sha256 = $sha256
        }
    }
    return @($readback)
}

function Assert-ProductVersion {
    param([string]$ProductCode, [string]$ExpectedVersion)
    $state = Get-ProductState -ProductCode $ProductCode
    if ($state -ne 5) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_PRODUCT_INVALID: product=$ProductCode expected_state=5 actual_state=$state"
    }
    $actualVersion = Get-InstalledProductVersion -ProductCode $ProductCode
    if ($actualVersion -cne $ExpectedVersion) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_PRODUCT_INVALID: product=$ProductCode expected_version=$ExpectedVersion actual_version=$actualVersion"
    }
}

function Get-LogReadback {
    param([string]$Path, [int]$ExitCode)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_LOG_MISSING: msiexec log not found at $Path"
    }
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $Path
        exit_code = $ExitCode
        bytes = $item.Length
        sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

Assert-LifecycleAdministrator

$newerMsi = Resolve-LifecycleLeaf -Path $NewerMsiPath -Label "newer MSI"
$olderMsi = Resolve-LifecycleLeaf -Path $OlderMsiPath -Label "older MSI"
$newerStage = Resolve-LifecycleContainer -Path $NewerStageRoot -Label "newer stage"
$olderStage = Resolve-LifecycleContainer -Path $OlderStageRoot -Label "older stage"
$newerVersion = (ConvertTo-LitdWindowsPackageVersion -Value $NewerVersion).text
$olderVersion = (ConvertTo-LitdWindowsPackageVersion -Value $OlderVersion).text
if ((Compare-LitdWindowsPackageVersion -Left $olderVersion -Right $newerVersion) -ge 0) {
    throw "LITD_WINDOWS_MSI_LIFECYCLE_VERSION_INVALID: older=$olderVersion must be less than newer=$newerVersion"
}
if ($newerMsi -ceq $olderMsi) {
    throw "LITD_WINDOWS_MSI_LIFECYCLE_VERSION_INVALID: older and newer MSI paths must differ"
}

$logs = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($LogRoot)
if (Test-Path -LiteralPath $logs) {
    if (-not (Test-Path -LiteralPath $logs -PathType Container)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_LOG_ROOT_INVALID: path is not a directory: $logs"
    }
    if (@(Get-ChildItem -LiteralPath $logs -Force).Count -ne 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_LOG_ROOT_INVALID: directory must be empty: $logs"
    }
} else {
    [void][IO.Directory]::CreateDirectory($logs)
}

$newerMsiReadback = & (Join-Path $PSScriptRoot "verify-windows-msi.ps1") `
    -MsiPath $newerMsi -StageRoot $newerStage -ExpectedPackageVersion $newerVersion `
    -OutputPath (Join-Path $logs "newer-msi-readback.json")
$olderMsiReadback = & (Join-Path $PSScriptRoot "verify-windows-msi.ps1") `
    -MsiPath $olderMsi -StageRoot $olderStage -ExpectedPackageVersion $olderVersion `
    -OutputPath (Join-Path $logs "older-msi-readback.json")
$newerProductCode = Get-MsiProperty -MsiPath $newerMsi -Name "ProductCode"
$olderProductCode = Get-MsiProperty -MsiPath $olderMsi -Name "ProductCode"
if ($newerProductCode -ceq $olderProductCode) {
    throw "LITD_WINDOWS_MSI_LIFECYCLE_CONTRACT_INVALID: major-upgrade MSI ProductCodes must differ"
}

$relatedBefore = @(Get-RelatedProductCodes)
if ($relatedBefore.Count -ne 0) {
    throw "LITD_WINDOWS_MSI_LIFECYCLE_PREEXISTING_STATE: related_products=$($relatedBefore -join ',')"
}
Assert-SystemEnvironmentAbsent
$installRoot = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)) `
    "Light in the Dark"
if (Test-Path -LiteralPath $installRoot) {
    throw "LITD_WINDOWS_MSI_LIFECYCLE_PREEXISTING_STATE: install directory already exists at $installRoot"
}

$newerPayload = @(Get-PayloadContract -StageRoot $newerStage)
$mutationStarted = $false
$installExit = -1
$repairExit = -1
$downgradeExit = -1
$uninstallExit = -1
$installedReadback = @()
$repairReadback = @()

try {
    $mutationStarted = $true
    $installLog = Join-Path $logs "install-newer.log"
    $installExit = Invoke-LoggedMsiExec -OperationArguments @("/i", $newerMsi) -LogPath $installLog
    if ($installExit -ne 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_INSTALL_FAILED: exit=$installExit log=$installLog"
    }
    Assert-ProductVersion -ProductCode $newerProductCode -ExpectedVersion $newerVersion
    Assert-SystemEnvironmentInstalled -InstallRoot $installRoot
    $installedReadback = @(Assert-InstalledPayload -Payload $newerPayload -InstallRoot $installRoot)

    $repairProbe = Join-Path $installRoot "README.md"
    [IO.File]::AppendAllText($repairProbe, "`r`nLITD_MSI_REPAIR_PROBE`r`n", [Text.Encoding]::UTF8)
    $probeHash = (Get-FileHash -LiteralPath $repairProbe -Algorithm SHA256).Hash.ToLowerInvariant()
    $expectedReadmeHash = ($newerPayload | Where-Object installed_path -ceq "README.md").sha256
    if ($probeHash -ceq $expectedReadmeHash) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_REPAIR_PROBE_FAILED: README hash did not change"
    }

    $repairLog = Join-Path $logs "repair-newer.log"
    $repairExit = Invoke-LoggedMsiExec -OperationArguments @("/fa", $newerProductCode) `
        -LogPath $repairLog
    if ($repairExit -ne 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_REPAIR_FAILED: exit=$repairExit log=$repairLog"
    }
    $repairReadback = @(Assert-InstalledPayload -Payload $newerPayload -InstallRoot $installRoot)

    $downgradeLog = Join-Path $logs "downgrade-older.log"
    $downgradeExit = Invoke-LoggedMsiExec -OperationArguments @("/i", $olderMsi) `
        -LogPath $downgradeLog
    if ($downgradeExit -eq 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_DOWNGRADE_ALLOWED: older=$olderVersion newer=$newerVersion"
    }
    if (-not (Test-Path -LiteralPath $downgradeLog -PathType Leaf)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_DOWNGRADE_UNPROVEN: log missing at $downgradeLog"
    }
    $downgradeText = Get-Content -LiteralPath $downgradeLog -Raw
    if (-not $downgradeText.Contains($downgradeMessage)) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_DOWNGRADE_UNPROVEN: exit=$downgradeExit authored message absent from $downgradeLog"
    }
    Assert-ProductVersion -ProductCode $newerProductCode -ExpectedVersion $newerVersion
    [void](Assert-InstalledPayload -Payload $newerPayload -InstallRoot $installRoot)

    $uninstallLog = Join-Path $logs "uninstall-newer.log"
    $uninstallExit = Invoke-LoggedMsiExec -OperationArguments @("/x", $newerProductCode) `
        -LogPath $uninstallLog
    if ($uninstallExit -ne 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_UNINSTALL_FAILED: exit=$uninstallExit log=$uninstallLog"
    }
    if ((Get-ProductState -ProductCode $newerProductCode) -eq 5) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_UNINSTALL_FAILED: product remains installed"
    }
    if (@(Get-RelatedProductCodes).Count -ne 0) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_UNINSTALL_FAILED: related product remains registered"
    }
    if (Test-Path -LiteralPath $installRoot) {
        throw "LITD_WINDOWS_MSI_LIFECYCLE_UNINSTALL_FAILED: install directory remains at $installRoot"
    }
    Assert-SystemEnvironmentAbsent
    $mutationStarted = $false

    $readback = [ordered]@{
        schema = "litd.windows.msi-lifecycle.v1"
        newer_version = $newerVersion
        older_version = $olderVersion
        newer_product_code = $newerProductCode
        older_product_code = $olderProductCode
        newer_msi_sha256 = $newerMsiReadback.msi_sha256
        older_msi_sha256 = $olderMsiReadback.msi_sha256
        install_root = $installRoot
        installed_payload = $installedReadback
        repaired_payload = $repairReadback
        downgrade_message = $downgradeMessage
        operations = [ordered]@{
            install = Get-LogReadback -Path $installLog -ExitCode $installExit
            repair = Get-LogReadback -Path $repairLog -ExitCode $repairExit
            downgrade = Get-LogReadback -Path $downgradeLog -ExitCode $downgradeExit
            uninstall = Get-LogReadback -Path $uninstallLog -ExitCode $uninstallExit
        }
        final_related_product_count = 0
        final_install_root_present = $false
        final_system_environment_present = $false
    }
    $readbackPath = Join-Path $logs "lifecycle-readback.json"
    $readback | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $readbackPath -Encoding UTF8
    [pscustomobject]$readback
} finally {
    if ($mutationStarted) {
        $cleanupFailures = @()
        foreach ($productCode in @(Get-RelatedProductCodes)) {
            $cleanupLog = Join-Path $logs "cleanup-$($productCode.Trim('{}')).log"
            $cleanupExit = Invoke-LoggedMsiExec -OperationArguments @("/x", $productCode) `
                -LogPath $cleanupLog
            if ($cleanupExit -ne 0 -and $cleanupExit -ne 1605) {
                $cleanupFailures += "$productCode exit=$cleanupExit log=$cleanupLog"
            }
        }
        if ($cleanupFailures.Count -ne 0) {
            throw "LITD_WINDOWS_MSI_LIFECYCLE_CLEANUP_FAILED: $($cleanupFailures -join '; ')"
        }
    }
}
