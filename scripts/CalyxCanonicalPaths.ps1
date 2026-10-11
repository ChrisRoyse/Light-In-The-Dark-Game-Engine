function Get-GameEditorCanonicalCalyxHome {
    $expected = [System.IO.Path]::GetFullPath("C:\code\GameEditor\Calyx")
    if (-not (Test-Path -LiteralPath $expected -PathType Container)) {
        throw "LITD_CALYX_CANONICAL_ROOT_MISSING: canonical GameEditor Calyx root not found at $expected"
    }
    $item = Get-Item -LiteralPath $expected -Force
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "LITD_CALYX_CANONICAL_ROOT_ESCAPE: canonical GameEditor Calyx root cannot be a link or junction: $expected"
    }
    $resolved = (Resolve-Path -LiteralPath $expected).ProviderPath
    if (-not [System.StringComparer]::OrdinalIgnoreCase.Equals(
        (Normalize-GameEditorCalyxPathKey -Path $expected),
        (Normalize-GameEditorCalyxPathKey -Path $resolved)
    )) {
        throw "LITD_CALYX_CANONICAL_ROOT_ESCAPE: $expected resolves to $resolved"
    }
    $resolved
}

function Resolve-GameEditorCalyxHome {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $root = Get-GameEditorCanonicalCalyxHome
    Resolve-GameEditorCalyxPath -Path $Path -Label "CALYX_HOME" -Type Container
}

function Resolve-GameEditorCalyxPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Label,

        [ValidateSet("Any", "Container", "Leaf")]
        [string]$Type = "Any"
    )

    $root = Get-GameEditorCanonicalCalyxHome
    if (-not [System.IO.Path]::IsPathRooted($Path)) {
        throw "LITD_CALYX_PATH_NOT_ABSOLUTE: $Label must be an absolute path beneath $root, got $Path"
    }
    $absolute = [System.IO.Path]::GetFullPath($Path)
    $relative = [System.IO.Path]::GetRelativePath($root, $absolute)
    if (Test-GameEditorCalyxRelativePathOutside -RelativePath $relative) {
        throw "LITD_CALYX_PATH_OUTSIDE_CANONICAL_ROOT: $Label $absolute is outside $root"
    }
    $pathType = switch ($Type) {
        "Container" { "Container" }
        "Leaf" { "Leaf" }
        default { "Any" }
    }
    if (-not (Test-Path -LiteralPath $absolute -PathType $pathType)) {
        throw "LITD_CALYX_CANONICAL_PATH_MISSING: $Label not found at $absolute"
    }
    Assert-GameEditorCalyxPathHasNoEscape -Root $root -AbsolutePath $absolute -Label $Label
    $resolved = (Resolve-Path -LiteralPath $absolute).ProviderPath
    $resolvedRelative = [System.IO.Path]::GetRelativePath($root, $resolved)
    if (Test-GameEditorCalyxRelativePathOutside -RelativePath $resolvedRelative) {
        throw "LITD_CALYX_CANONICAL_PATH_ESCAPE: $Label $absolute resolves outside $root to $resolved"
    }
    $resolved
}

function Resolve-GameEditorEngineContentVault {
    param(
        [string]$CalyxHome = (Get-GameEditorCanonicalCalyxHome),
        [string]$VaultName = "gameeditor-engine-content-panel"
    )

    $resolvedHome = Resolve-GameEditorCalyxHome -Path $CalyxHome
    $indexPath = Resolve-GameEditorCalyxPath -Path (Join-Path $resolvedHome "vaults\index.json") -Label "Calyx vault index" -Type Leaf
    try {
        $index = Get-Content -LiteralPath $indexPath -Raw | ConvertFrom-Json
    } catch {
        throw "LITD_CALYX_VAULT_INDEX_INVALID: parse $indexPath failed: $($_.Exception.Message)"
    }
    $matches = @($index.vaults | Where-Object { $_.name -eq $VaultName })
    if ($matches.Count -ne 1) {
        throw "LITD_CALYX_ENGINE_PANEL_VAULT_MISSING: expected one vault named $VaultName in $indexPath, found $($matches.Count)"
    }
    $entry = $matches[0]
    $expectedRelative = "vaults/$($entry.vault_id)"
    $actualRelative = ([string]$entry.path).Replace("\", "/")
    if ($actualRelative -ne $expectedRelative) {
        throw "LITD_CALYX_VAULT_INDEX_INVALID: vault $VaultName path/id mismatch path=$actualRelative vault_id=$($entry.vault_id)"
    }
    Resolve-GameEditorCalyxPath -Path (Join-Path $resolvedHome $entry.path) -Label $VaultName -Type Container
}

function Assert-GameEditorCalyxPathHasNoEscape {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$AbsolutePath,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $relative = [System.IO.Path]::GetRelativePath($Root, $AbsolutePath)
    $cursor = $Root
    foreach ($segment in ($relative -split '[\\/]')) {
        if (-not $segment -or $segment -eq ".") {
            continue
        }
        $cursor = Join-Path $cursor $segment
        if (-not (Test-Path -LiteralPath $cursor)) {
            break
        }
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) {
            continue
        }
        if (-not $item.PSObject.Methods.Name.Contains("ResolveLinkTarget")) {
            throw "LITD_CALYX_CANONICAL_PATH_ESCAPE: cannot verify reparse target for $Label at $cursor"
        }
        $target = $item.ResolveLinkTarget($true)
        if ($null -eq $target) {
            throw "LITD_CALYX_CANONICAL_PATH_ESCAPE: unresolved reparse target for $Label at $cursor"
        }
        $targetRelative = [System.IO.Path]::GetRelativePath($Root, $target.FullName)
        if (Test-GameEditorCalyxRelativePathOutside -RelativePath $targetRelative) {
            throw "LITD_CALYX_CANONICAL_PATH_ESCAPE: $Label escapes $Root through $cursor to $($target.FullName)"
        }
    }
}

function Test-GameEditorCalyxRelativePathOutside {
    param([Parameter(Mandatory = $true)][string]$RelativePath)

    [System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -eq ".." -or
        $RelativePath.StartsWith("..\", [System.StringComparison]::Ordinal) -or
        $RelativePath.StartsWith("../", [System.StringComparison]::Ordinal)
}

function Normalize-GameEditorCalyxPathKey {
    param([Parameter(Mandatory = $true)][string]$Path)

    $value = [System.IO.Path]::GetFullPath($Path).TrimEnd("\", "/")
    if ($value.StartsWith("\\?\", [System.StringComparison]::Ordinal)) {
        $value = $value.Substring(4)
    }
    $value
}
