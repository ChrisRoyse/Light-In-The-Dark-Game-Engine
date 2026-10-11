Set-StrictMode -Version Latest

function Export-LitdMsiIcon {
    param(
        [Parameter(Mandatory = $true)]$Database,
        [Parameter(Mandatory = $true)][string]$IconName,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    $view = $Database.OpenView(
        "SELECT ``Data`` FROM ``Icon`` WHERE ``Name`` = '$IconName'"
    )
    try {
        [void]$view.Execute()
        $record = $view.Fetch()
        if ($null -eq $record) {
            throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: missing Icon row '$IconName'"
        }
        try {
            $expectedBytes = [int]$record.DataSize(1)
            if ($expectedBytes -le 0) {
                throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: Icon.$IconName data is empty"
            }
            $output = [IO.File]::Open(
                $Destination,
                [IO.FileMode]::CreateNew,
                [IO.FileAccess]::Write,
                [IO.FileShare]::None
            )
            try {
                $read = 0
                do {
                    $chunk = [string]$record.ReadStream(1, 65536, 1)
                    if ($chunk.Length -eq 0) {
                        break
                    }
                    $bytes = [Text.Encoding]::GetEncoding(28591).GetBytes($chunk)
                    $output.Write($bytes, 0, $bytes.Length)
                    $read += $bytes.Length
                } while ($read -lt $expectedBytes)
            } finally {
                $output.Dispose()
            }
            if ($read -ne $expectedBytes) {
                throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: Icon.$IconName expected_bytes=$expectedBytes actual_bytes=$read"
            }
        } finally {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
        }
    } finally {
        [void]$view.Close()
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
    }
}

function Test-LitdWindowsMsiDiscoverability {
    param(
        [Parameter(Mandatory = $true)]$Database,
        [Parameter(Mandatory = $true)][hashtable]$Properties,
        [Parameter(Mandatory = $true)][hashtable]$Directories,
        [Parameter(Mandatory = $true)][string]$IconDestination
    )

    $arp = [ordered]@{
        ARPCOMMENTS = "Windows game runtime and editor."
        ARPHELPLINK = "https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues"
        ARPURLINFOABOUT = "https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine"
        ARPPRODUCTICON = "LitdProductIcon.ico"
        ARPINSTALLLOCATION = "[INSTALLFOLDER]"
    }
    foreach ($expected in $arp.GetEnumerator()) {
        if ($expected.Key -ceq "ARPINSTALLLOCATION") {
            continue
        }
        $row = Get-MsiRequiredRow -Map $Properties -Key $expected.Key -Label "Property"
        Assert-MsiValue -Actual $row.values[1] -Expected $expected.Value `
            -Label "Property.$($expected.Key)"
    }
    if ($Properties.ContainsKey("ARPINSTALLLOCATION")) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: ARPINSTALLLOCATION must be set after CostFinalize, not stored as a bracketed Property value"
    }

    $installLocationActionRows = Get-MsiRows -Database $Database `
        -Query "SELECT ``Action``, ``Type``, ``Source``, ``Target`` FROM ``CustomAction`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'" `
        -ColumnCount 4
    Assert-MsiRowCount -Rows $installLocationActionRows -Expected 1 `
        -Label "CustomAction.SetARPINSTALLLOCATION"
    $installLocationAction = $installLocationActionRows[0]
    Assert-MsiValue -Actual $installLocationAction.values[1] -Expected "51" `
        -Label "CustomAction.SetARPINSTALLLOCATION.type"
    Assert-MsiValue -Actual $installLocationAction.values[2] -Expected "ARPINSTALLLOCATION" `
        -Label "CustomAction.SetARPINSTALLLOCATION.source"
    Assert-MsiValue -Actual $installLocationAction.values[3] -Expected "[INSTALLFOLDER]" `
        -Label "CustomAction.SetARPINSTALLLOCATION.target"

    $installLocationSequenceRows = Get-MsiRows -Database $Database `
        -Query "SELECT ``Action``, ``Condition``, ``Sequence`` FROM ``InstallExecuteSequence`` WHERE ``Action`` = 'SetARPINSTALLLOCATION'" `
        -ColumnCount 3
    Assert-MsiRowCount -Rows $installLocationSequenceRows -Expected 1 `
        -Label "InstallExecuteSequence.SetARPINSTALLLOCATION"
    $installLocationSequence = $installLocationSequenceRows[0]
    Assert-MsiValue -Actual $installLocationSequence.values[1] -Expected "" `
        -Label "InstallExecuteSequence.SetARPINSTALLLOCATION.condition"
    Assert-MsiValue -Actual $installLocationSequence.values[2] -Expected "1001" `
        -Label "InstallExecuteSequence.SetARPINSTALLLOCATION.sequence"
    $costFinalizeRows = Get-MsiRows -Database $Database `
        -Query "SELECT ``Action``, ``Sequence`` FROM ``InstallExecuteSequence`` WHERE ``Action`` = 'CostFinalize'" `
        -ColumnCount 2
    Assert-MsiRowCount -Rows $costFinalizeRows -Expected 1 `
        -Label "InstallExecuteSequence.CostFinalize"
    Assert-MsiValue -Actual $costFinalizeRows[0].values[1] -Expected "1000" `
        -Label "InstallExecuteSequence.CostFinalize.sequence"

    $programs = Get-MsiRequiredRow -Map $Directories -Key "ApplicationProgramsFolder" `
        -Label "Directory"
    Assert-MsiValue -Actual $programs.values[1] -Expected "ProgramMenuFolder" `
        -Label "Directory.ApplicationProgramsFolder.parent"
    Assert-MsiValue -Actual (Get-MsiLongFileName $programs.values[2]) `
        -Expected "Light in the Dark" -Label "Directory.ApplicationProgramsFolder.name"

    $expectedShortcuts = @(
        [pscustomobject]@{
            id = "EditorStartMenuShortcut"
            name = "Light in the Dark Editor"
            component = "LitdEditorExe"
            target = "[#LitdEditorExe]"
            description = "Create and edit Light in the Dark worlds."
        },
        [pscustomobject]@{
            id = "GameStartMenuShortcut"
            name = "Light in the Dark Game"
            component = "LitdGameExe"
            target = "[#LitdGameExe]"
            description = "Launch the Light in the Dark game runtime."
        }
    )
    $shortcutRows = Get-MsiRows -Database $Database `
        -Query "SELECT ``Shortcut``, ``Directory_``, ``Name``, ``Component_``, ``Target``, ``Description``, ``Icon_``, ``WkDir`` FROM ``Shortcut``" `
        -ColumnCount 8
    Assert-MsiRowCount -Rows $shortcutRows -Expected $expectedShortcuts.Count -Label "Shortcut"
    $shortcuts = Get-MsiRowMap -Rows $shortcutRows -Label "Shortcut"
    $shortcutReadback = @()
    foreach ($expected in $expectedShortcuts) {
        $row = Get-MsiRequiredRow -Map $shortcuts -Key $expected.id -Label "Shortcut"
        Assert-MsiValue -Actual $row.values[1] -Expected "ApplicationProgramsFolder" `
            -Label "Shortcut.$($expected.id).directory"
        Assert-MsiValue -Actual (Get-MsiLongFileName $row.values[2]) -Expected $expected.name `
            -Label "Shortcut.$($expected.id).name"
        Assert-MsiValue -Actual $row.values[3] -Expected $expected.component `
            -Label "Shortcut.$($expected.id).component"
        Assert-MsiValue -Actual $row.values[4] -Expected $expected.target `
            -Label "Shortcut.$($expected.id).target"
        Assert-MsiValue -Actual $row.values[5] -Expected $expected.description `
            -Label "Shortcut.$($expected.id).description"
        Assert-MsiValue -Actual $row.values[6] -Expected "LitdProductIcon.ico" `
            -Label "Shortcut.$($expected.id).icon"
        Assert-MsiValue -Actual $row.values[7] -Expected "InstallFolder" `
            -Label "Shortcut.$($expected.id).working_directory"
        $shortcutReadback += [ordered]@{
            id = $expected.id
            name = $expected.name
            target = $expected.target
            component = $expected.component
        }
    }

    $iconRows = Get-MsiRows -Database $Database -Query "SELECT ``Name`` FROM ``Icon``" `
        -ColumnCount 1
    Assert-MsiRowCount -Rows $iconRows -Expected 1 -Label "Icon"
    Assert-MsiValue -Actual $iconRows[0].values[0] -Expected "LitdProductIcon.ico" `
        -Label "Icon.name"
    Export-LitdMsiIcon -Database $Database -IconName "LitdProductIcon.ico" `
        -Destination $IconDestination
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $sourceIcon = Join-Path $repoRoot "packaging/windows/litd.ico"
    if (-not (Test-Path -LiteralPath $sourceIcon -PathType Leaf)) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: product icon source is missing at $sourceIcon"
    }
    $sourceItem = Get-Item -LiteralPath $sourceIcon
    $embeddedItem = Get-Item -LiteralPath $IconDestination
    $sourceHash = (Get-FileHash -LiteralPath $sourceIcon -Algorithm SHA256).Hash.ToLowerInvariant()
    $embeddedHash = (Get-FileHash -LiteralPath $IconDestination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sourceItem.Length -ne $embeddedItem.Length -or $sourceHash -ne $embeddedHash) {
        throw "LITD_WINDOWS_MSI_CONTRACT_INVALID: Icon.LitdProductIcon.ico source/embedded bytes differ"
    }

    $removeRows = Get-MsiRows -Database $Database `
        -Query "SELECT ``FileKey``, ``Component_``, ``DirProperty``, ``InstallMode`` FROM ``RemoveFile``" `
        -ColumnCount 4
    Assert-MsiRowCount -Rows $removeRows -Expected 1 -Label "RemoveFile"
    Assert-MsiValue -Actual $removeRows[0].values[0] -Expected "RemoveApplicationProgramsFolder" `
        -Label "RemoveFile.key"
    Assert-MsiValue -Actual $removeRows[0].values[1] -Expected "LitdEditorExe" `
        -Label "RemoveFile.component"
    Assert-MsiValue -Actual $removeRows[0].values[2] -Expected "ApplicationProgramsFolder" `
        -Label "RemoveFile.directory"
    Assert-MsiValue -Actual $removeRows[0].values[3] -Expected "2" `
        -Label "RemoveFile.install_mode"

    [ordered]@{
        start_menu_folder = "Light in the Dark"
        shortcuts = $shortcutReadback
        icon = [ordered]@{
            id = "LitdProductIcon.ico"
            bytes = $embeddedItem.Length
            sha256 = $embeddedHash
        }
        arp = $arp
        arp_install_location_action = [ordered]@{
            action = "SetARPINSTALLLOCATION"
            type = 51
            source = "ARPINSTALLLOCATION"
            target = "[INSTALLFOLDER]"
            sequence = 1001
            after = "CostFinalize"
        }
        remove_start_menu_folder_on_uninstall = $true
    }
}
