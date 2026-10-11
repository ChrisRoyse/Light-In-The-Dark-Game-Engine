param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$StageRoot,
    [Parameter(Mandatory = $true)][string]$ExpectedPackageVersion,
    [string]$OutputPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
. (Join-Path $PSScriptRoot "WindowsPackageContract.ps1")
. (Join-Path $PSScriptRoot "WindowsMsiDiscoverabilityContract.ps1")
Assert-LitdWindowsPlatform

function Assert-MsiValue {
    param(
        [string]$Actual,
        [string]$Expected,
        [string]$Label
    )
    if ($Actual -cne $Expected) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: $Label expected='$Expected' actual='$Actual'"
    }
}

function Assert-MsiRowCount {
    param(
        [object[]]$Rows,
        [int]$Expected,
        [string]$Label
    )
    $actual = if ($null -eq $Rows) { 0 } else { $Rows.Count }
    if ($actual -ne $Expected) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: $Label expected_rows=$Expected actual_rows=$actual"
    }
}

function Get-MsiRows {
    param(
        [object]$Database,
        [string]$Query,
        [int]$ColumnCount
    )
    $view = $Database.OpenView($Query)
    try {
        [void]$view.Execute()
        $rows = @()
        while ($record = $view.Fetch()) {
            try {
                $values = @()
                foreach ($column in 1..$ColumnCount) {
                    $values += [string]$record.StringData($column)
                }
                $rows += [pscustomobject]@{ values = $values }
            } finally {
                [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
            }
        }
        return @($rows)
    } finally {
        [void]$view.Close()
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
    }
}

function Get-MsiRowMap {
    param(
        [object[]]$Rows,
        [string]$Label
    )
    $map = @{}
    foreach ($row in $Rows) {
        $key = [string]$row.values[0]
        if ($map.ContainsKey($key)) {
            throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: duplicate $Label key '$key'"
        }
        $map[$key] = $row
    }
    return $map
}

function Get-MsiRequiredRow {
    param(
        [hashtable]$Map,
        [string]$Key,
        [string]$Label
    )
    if (-not $Map.ContainsKey($Key)) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: missing $Label row '$Key'"
    }
    return $Map[$Key]
}

function Get-MsiLongFileName {
    param([string]$Value)
    $parts = $Value.Split("|", 2)
    if ($parts.Count -eq 2) {
        return $parts[1]
    }
    return $parts[0]
}

function Export-MsiCabinet {
    param(
        [object]$Database,
        [string]$CabinetName,
        [string]$Destination
    )
    $view = $Database.OpenView(
        "SELECT ``Name``, ``Data`` FROM ``_Streams`` WHERE ``Name`` = '$CabinetName'"
    )
    try {
        [void]$view.Execute()
        $record = $view.Fetch()
        if ($null -eq $record) {
            throw "LITD_WINDOWS_MSI_CABINET_INVALID: embedded stream '$CabinetName' is missing"
        }
        try {
            $expectedBytes = [int64]$record.DataSize(2)
            if ($expectedBytes -le 0) {
                throw "LITD_WINDOWS_MSI_CABINET_INVALID: embedded stream '$CabinetName' is empty"
            }
            $output = [IO.File]::Open(
                $Destination,
                [IO.FileMode]::CreateNew,
                [IO.FileAccess]::Write,
                [IO.FileShare]::None
            )
            try {
                while ($true) {
                    $chunk = [string]$record.ReadStream(2, 1MB, 1)
                    if ($chunk.Length -eq 0) {
                        break
                    }
                    $bytes = [Text.Encoding]::GetEncoding(28591).GetBytes($chunk)
                    $output.Write($bytes, 0, $bytes.Length)
                }
            } finally {
                $output.Dispose()
            }
            $actualBytes = (Get-Item -LiteralPath $Destination).Length
            if ($actualBytes -ne $expectedBytes) {
                throw "LITD_WINDOWS_MSI_CABINET_INVALID: stream '$CabinetName' expected_bytes=$expectedBytes actual_bytes=$actualBytes"
            }
            return [pscustomobject]@{
                name = $CabinetName
                bytes = $actualBytes
                sha256 = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        } finally {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
        }
    } finally {
        [void]$view.Close()
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
    }
}

if (-not (Test-Path -LiteralPath $MsiPath -PathType Leaf)) {
    throw "LITD_WINDOWS_MSI_MISSING: MSI not found at $MsiPath"
}
if (-not (Test-Path -LiteralPath $StageRoot -PathType Container)) {
    throw "LITD_WINDOWS_MSI_STAGE_MISSING: staged payload not found at $StageRoot"
}
$msi = (Resolve-Path -LiteralPath $MsiPath).Path
$stage = (Resolve-Path -LiteralPath $StageRoot).Path
$version = (ConvertTo-LitdWindowsPackageVersion -Value $ExpectedPackageVersion).text
$output = if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    Join-Path (Split-Path -Parent $msi) "msi-readback.json"
} else {
    $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
}
$outputParent = Split-Path -Parent $output
if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
    throw "LITD_WINDOWS_MSI_OUTPUT_PARENT_MISSING: output directory not found at $outputParent"
}

$payload = @(
    [pscustomobject]@{ id = "CargoLock"; component = "CargoLock"; directory = "InstallFolder"; path = "Cargo.lock" }
    [pscustomobject]@{ id = "LitdEditorExe"; component = "LitdEditorExe"; directory = "InstallFolder"; path = "app/litd-editor.exe" }
    [pscustomobject]@{ id = "LitdGameExe"; component = "LitdGameExe"; directory = "InstallFolder"; path = "app/litd-game.exe" }
    [pscustomobject]@{ id = "OnnxRuntimeDll"; component = "OnnxRuntimeDll"; directory = "InstallFolder"; path = "app/onnxruntime.dll" }
    [pscustomobject]@{ id = "PackageManifest"; component = "PackageManifest"; directory = "InstallFolder"; path = "package-manifest.json" }
    [pscustomobject]@{ id = "Readme"; component = "Readme"; directory = "InstallFolder"; path = "README.md" }
    [pscustomobject]@{ id = "UbuntuFontLicense"; component = "UbuntuFontLicense"; directory = "LicensesFolder"; path = "licenses/Ubuntu-Font-License-1.0.txt" }
)
foreach ($item in $payload) {
    $source = Join-Path $stage $item.path
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "LITD_WINDOWS_MSI_STAGE_MISSING: required payload not found at $source"
    }
    $item | Add-Member -NotePropertyName source -NotePropertyValue $source
    $item | Add-Member -NotePropertyName bytes -NotePropertyValue (Get-Item -LiteralPath $source).Length
    $item | Add-Member -NotePropertyName sha256 -NotePropertyValue (
        (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    )
}

$scratch = Join-Path ([IO.Path]::GetTempPath()) ("litd-msi-verify-" + [guid]::NewGuid().ToString("N"))
[void][IO.Directory]::CreateDirectory($scratch)
$cabPath = Join-Path $scratch "cab1.cab"
$extractRoot = Join-Path $scratch "payload"
[void][IO.Directory]::CreateDirectory($extractRoot)

try {
    $installer = $null
    $database = $null
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.OpenDatabase($msi, 0)

        $summary = $database.SummaryInformation(0)
        try {
            Assert-MsiValue -Actual ([string]$summary.Property(7)) -Expected "x64;0" -Label "summary template"
        } finally {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($summary)
        }

        $propertyRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Property``, ``Value`` FROM ``Property``" -ColumnCount 2
        $properties = Get-MsiRowMap -Rows $propertyRows -Label "Property"
        foreach ($expected in ([ordered]@{
            ProductName = "Light in the Dark"
            Manufacturer = "Light in the Dark"
            ProductVersion = $version
            UpgradeCode = "{2F3B1D41-B982-48D1-92C1-7F23B45574C9}"
            ALLUSERS = "1"
        }).GetEnumerator()) {
            $row = Get-MsiRequiredRow -Map $properties -Key $expected.Key -Label "Property"
            Assert-MsiValue -Actual $row.values[1] -Expected $expected.Value -Label "Property.$($expected.Key)"
        }

        $directoryRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Directory``, ``Directory_Parent``, ``DefaultDir`` FROM ``Directory``" `
            -ColumnCount 3
        Assert-MsiRowCount -Rows $directoryRows -Expected 6 -Label "Directory"
        $directories = Get-MsiRowMap -Rows $directoryRows -Label "Directory"
        $installDirectory = Get-MsiRequiredRow -Map $directories -Key "InstallFolder" -Label "Directory"
        Assert-MsiValue -Actual $installDirectory.values[1] -Expected "ProgramFiles64Folder" `
            -Label "Directory.InstallFolder.parent"
        Assert-MsiValue -Actual (Get-MsiLongFileName $installDirectory.values[2]) `
            -Expected "Light in the Dark" -Label "Directory.InstallFolder.name"
        $licenseDirectory = Get-MsiRequiredRow -Map $directories -Key "LicensesFolder" -Label "Directory"
        Assert-MsiValue -Actual $licenseDirectory.values[1] -Expected "InstallFolder" `
            -Label "Directory.LicensesFolder.parent"
        Assert-MsiValue -Actual $licenseDirectory.values[2] -Expected "licenses" `
            -Label "Directory.LicensesFolder.name"

        $componentRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Component``, ``Directory_``, ``Attributes``, ``KeyPath`` FROM ``Component``" `
            -ColumnCount 4
        Assert-MsiRowCount -Rows $componentRows -Expected $payload.Count -Label "Component"
        $components = Get-MsiRowMap -Rows $componentRows -Label "Component"
        foreach ($item in $payload) {
            $row = Get-MsiRequiredRow -Map $components -Key $item.component -Label "Component"
            Assert-MsiValue -Actual $row.values[1] -Expected $item.directory `
                -Label "Component.$($item.component).directory"
            Assert-MsiValue -Actual $row.values[2] -Expected "256" `
                -Label "Component.$($item.component).attributes"
            Assert-MsiValue -Actual $row.values[3] -Expected $item.id `
                -Label "Component.$($item.component).key_path"
        }

        $fileRows = Get-MsiRows -Database $database `
            -Query "SELECT ``File``, ``Component_``, ``FileName``, ``FileSize``, ``Sequence`` FROM ``File``" `
            -ColumnCount 5
        Assert-MsiRowCount -Rows $fileRows -Expected $payload.Count -Label "File"
        $files = Get-MsiRowMap -Rows $fileRows -Label "File"
        foreach ($item in $payload) {
            $row = Get-MsiRequiredRow -Map $files -Key $item.id -Label "File"
            Assert-MsiValue -Actual $row.values[1] -Expected $item.component `
                -Label "File.$($item.id).component"
            Assert-MsiValue -Actual (Get-MsiLongFileName $row.values[2]) `
                -Expected ([IO.Path]::GetFileName($item.path)) -Label "File.$($item.id).name"
            Assert-MsiValue -Actual $row.values[3] -Expected ([string]$item.bytes) `
                -Label "File.$($item.id).bytes"
        }

        $featureRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Feature_``, ``Component_`` FROM ``FeatureComponents``" -ColumnCount 2
        Assert-MsiRowCount -Rows $featureRows -Expected $payload.Count -Label "FeatureComponents"
        $featureMap = @{}
        foreach ($row in $featureRows) {
            $featureMap[$row.values[1]] = $row.values[0]
        }
        foreach ($item in $payload) {
            Assert-MsiValue -Actual ([string]$featureMap[$item.component]) -Expected "Main" `
                -Label "FeatureComponents.$($item.component)"
        }

        $environmentRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Environment``, ``Name``, ``Value``, ``Component_`` FROM ``Environment``" `
            -ColumnCount 4
        Assert-MsiRowCount -Rows $environmentRows -Expected 2 -Label "Environment"
        $environment = Get-MsiRowMap -Rows $environmentRows -Label "Environment"
        $ortEnvironment = Get-MsiRequiredRow -Map $environment -Key "OrtDylibPath" -Label "Environment"
        Assert-MsiValue -Actual $ortEnvironment.values[1] -Expected "=-*ORT_DYLIB_PATH" `
            -Label "Environment.OrtDylibPath.name"
        Assert-MsiValue -Actual $ortEnvironment.values[2] -Expected "[INSTALLFOLDER]onnxruntime.dll" `
            -Label "Environment.OrtDylibPath.value"
        Assert-MsiValue -Actual $ortEnvironment.values[3] -Expected "OnnxRuntimeDll" `
            -Label "Environment.OrtDylibPath.component"
        $capiEnvironment = Get-MsiRequiredRow -Map $environment -Key "CalyxOrtCapi" -Label "Environment"
        Assert-MsiValue -Actual $capiEnvironment.values[1] -Expected "=-*CALYX_ORT_CAPI" `
            -Label "Environment.CalyxOrtCapi.name"
        Assert-MsiValue -Actual $capiEnvironment.values[2] -Expected "[INSTALLFOLDER]" `
            -Label "Environment.CalyxOrtCapi.value"
        Assert-MsiValue -Actual $capiEnvironment.values[3] -Expected "OnnxRuntimeDll" `
            -Label "Environment.CalyxOrtCapi.component"

        $upgradeRows = Get-MsiRows -Database $database `
            -Query "SELECT ``UpgradeCode``, ``VersionMin``, ``VersionMax``, ``Attributes``, ``ActionProperty`` FROM ``Upgrade``" `
            -ColumnCount 5
        Assert-MsiRowCount -Rows $upgradeRows -Expected 2 -Label "Upgrade"
        $upgradeMap = @{}
        foreach ($row in $upgradeRows) {
            $upgradeMap[$row.values[4]] = $row
            Assert-MsiValue -Actual $row.values[0] `
                -Expected "{2F3B1D41-B982-48D1-92C1-7F23B45574C9}" -Label "Upgrade.code"
        }
        $upgrade = $upgradeMap["WIX_UPGRADE_DETECTED"]
        Assert-MsiValue -Actual $upgrade.values[2] -Expected $version -Label "Upgrade.maximum"
        Assert-MsiValue -Actual $upgrade.values[3] -Expected "1" -Label "Upgrade.attributes"
        $downgrade = $upgradeMap["WIX_DOWNGRADE_DETECTED"]
        Assert-MsiValue -Actual $downgrade.values[1] -Expected $version -Label "Downgrade.minimum"
        Assert-MsiValue -Actual $downgrade.values[3] -Expected "2" -Label "Downgrade.attributes"

        $launchRows = Get-MsiRows -Database $database `
            -Query "SELECT ``Condition``, ``Description`` FROM ``LaunchCondition``" -ColumnCount 2
        Assert-MsiRowCount -Rows $launchRows -Expected 1 -Label "LaunchCondition"
        Assert-MsiValue -Actual $launchRows[0].values[0] -Expected "NOT WIX_DOWNGRADE_DETECTED" `
            -Label "LaunchCondition.condition"
        Assert-MsiValue -Actual $launchRows[0].values[1] `
            -Expected "A newer Light in the Dark build is already installed." `
            -Label "LaunchCondition.description"

        $mediaRows = Get-MsiRows -Database $database `
            -Query "SELECT ``DiskId``, ``LastSequence``, ``Cabinet`` FROM ``Media``" -ColumnCount 3
        Assert-MsiRowCount -Rows $mediaRows -Expected 1 -Label "Media"
        Assert-MsiValue -Actual $mediaRows[0].values[0] -Expected "1" -Label "Media.disk"
        Assert-MsiValue -Actual $mediaRows[0].values[1] -Expected ([string]$payload.Count) `
            -Label "Media.last_sequence"
        Assert-MsiValue -Actual $mediaRows[0].values[2] -Expected "#cab1.cab" -Label "Media.cabinet"
        $discoverability = Test-LitdWindowsMsiDiscoverability `
            -Database $database `
            -Properties $properties `
            -Directories $directories `
            -IconDestination (Join-Path $scratch "LitdProductIcon.ico")
        $cabinet = Export-MsiCabinet -Database $database -CabinetName "cab1.cab" `
            -Destination $cabPath
    } catch {
        if ($_.Exception.Message -match "^LITD_WINDOWS_MSI_") {
            throw
        }
        $source = ($_.ScriptStackTrace -replace "(`r`n|`n|`r)+", " <- ")
        throw "LITD_WINDOWS_MSI_DATABASE_INVALID: $($_.Exception.Message); source=$source"
    } finally {
        if ($null -ne $database) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)
        }
        if ($null -ne $installer) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($installer)
        }
    }

    $expand = Get-Command "expand.exe" -ErrorAction SilentlyContinue
    if ($null -eq $expand) {
        throw "LITD_WINDOWS_MSI_CABINET_INVALID: Windows expand.exe was not found"
    }
    $expandLog = @(& $expand.Source "-F:*" $cabPath $extractRoot 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "LITD_WINDOWS_MSI_CABINET_INVALID: expand.exe failed exit=$LASTEXITCODE output=$($expandLog -join ' ')"
    }
    $extracted = @(Get-ChildItem -LiteralPath $extractRoot -File)
    Assert-MsiRowCount -Rows $extracted -Expected $payload.Count -Label "embedded cabinet payload"

    $payloadReadback = @()
    foreach ($item in $payload) {
        $embedded = Join-Path $extractRoot $item.id
        if (-not (Test-Path -LiteralPath $embedded -PathType Leaf)) {
            throw "LITD_WINDOWS_MSI_PAYLOAD_MISMATCH: embedded file '$($item.id)' is missing"
        }
        $embeddedItem = Get-Item -LiteralPath $embedded
        $embeddedHash = (Get-FileHash -LiteralPath $embedded -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($embeddedItem.Length -ne $item.bytes -or $embeddedHash -ne $item.sha256) {
            throw "LITD_WINDOWS_MSI_PAYLOAD_MISMATCH: path=$($item.path) expected_bytes=$($item.bytes) expected_sha256=$($item.sha256) actual_bytes=$($embeddedItem.Length) actual_sha256=$embeddedHash"
        }
        $payloadReadback += [ordered]@{
            id = $item.id
            path = $item.path
            bytes = $embeddedItem.Length
            sha256 = $embeddedHash
        }
    }

    $msiItem = Get-Item -LiteralPath $msi
    $readback = [ordered]@{
        schema = "litd.windows.msi-readback.v2"
        msi_path = $msi
        msi_bytes = $msiItem.Length
        msi_sha256 = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToLowerInvariant()
        package_name = "Light in the Dark"
        manufacturer = "Light in the Dark"
        package_version = $version
        architecture = "x64"
        install_scope = "perMachine"
        upgrade_code = "{2F3B1D41-B982-48D1-92C1-7F23B45574C9}"
        downgrade_blocked = $true
        discoverability = $discoverability
        cabinet = $cabinet
        payload_file_count = $payload.Count
        payload_files = $payloadReadback
        runtime_environment = [ordered]@{
            ORT_DYLIB_PATH = "[INSTALLFOLDER]onnxruntime.dll"
            CALYX_ORT_CAPI = "[INSTALLFOLDER]"
        }
    }
    $readback | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $output -Encoding UTF8
    [pscustomobject]$readback
} finally {
    if (Test-Path -LiteralPath $scratch) {
        [IO.Directory]::Delete($scratch, $true)
    }
}
