param(
    [string]$Configuration = "release",
    [string]$PackageVersion = "0.0.1",
    [string]$OutputRoot = "dist/windows",
    [switch]$SkipBuild,
    [switch]$NoInstaller,
    [switch]$BuildInstaller,
    [string]$WixPath = "wix",
    [string]$SignToolPath = "",
    [string]$CertificateThumbprint = "",
    [string]$TimestampUrl = "http://timestamp.digicert.com",
    [string]$SourceDateEpoch = "1704067200",
    [string]$OrtDylibPath = $env:ORT_DYLIB_PATH,
    [string]$RuntimeFontNoticePath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "WindowsPlatformContract.ps1")
. (Join-Path $PSScriptRoot "CalyxCanonicalPaths.ps1")
. (Join-Path $PSScriptRoot "WindowsPackageContract.ps1")
. (Join-Path $PSScriptRoot "WindowsSigningContract.ps1")
Assert-LitdWindowsPlatform

$runtimeFontNoticeRelativePath = "licenses/Ubuntu-Font-License-1.0.txt"
$runtimeFontNoticeExpectedBytes = 4673
$runtimeFontNoticeExpectedSha256 = "2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002"

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Invoke-Checked {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )
    Push-Location -LiteralPath $WorkingDirectory
    try {
        & $FilePath @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$FilePath failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function New-CleanDirectory {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Get-TargetDir {
    param(
        [string]$RepoRoot,
        [string]$Configuration
    )
    if ($Configuration -eq "release") {
        return Join-Path $RepoRoot "target/release"
    }
    if ($Configuration -eq "debug" -or $Configuration -eq "dev") {
        return Join-Path $RepoRoot "target/debug"
    }
    return Join-Path $RepoRoot "target/$Configuration"
}

function Get-CargoBuildArguments {
    param([string]$Configuration)
    $args = @("build", "--locked")
    if ($Configuration -eq "release") {
        $args += "--release"
    } elseif ($Configuration -ne "debug" -and $Configuration -ne "dev") {
        $args += @("--profile", $Configuration)
    }
    return $args + @("-p", "litd-editor", "-p", "litd-game")
}

function Resolve-OrtDylibInput {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "LITD_WINDOWS_PACKAGE_ORT_REQUIRED: pass -OrtDylibPath or set ORT_DYLIB_PATH to the canonical Calyx onnxruntime.dll"
    }
    $resolved = Resolve-GameEditorCalyxPath -Path $Path -Label "package ORT_DYLIB_PATH" -Type Leaf
    if ([System.IO.Path]::GetFileName($resolved) -ne "onnxruntime.dll") {
        throw "LITD_WINDOWS_PACKAGE_ORT_INVALID: expected onnxruntime.dll, got $resolved"
    }
    return $resolved
}

function Resolve-RuntimeFontNoticeInput {
    param(
        [string]$RepoRoot,
        [string]$Path
    )
    $candidate = if ([string]::IsNullOrWhiteSpace($Path)) {
        Join-Path $RepoRoot "docs/licenses/Ubuntu-Font-License-1.0.txt"
    } else {
        $Path
    }
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "LITD_WINDOWS_PACKAGE_FONT_NOTICE_MISSING: runtime font notice not found at $candidate"
    }
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    $item = Get-Item -LiteralPath $resolved
    $sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $resolved).Hash.ToLowerInvariant()
    if ($item.Length -ne $runtimeFontNoticeExpectedBytes -or $sha256 -ne $runtimeFontNoticeExpectedSha256) {
        throw "LITD_WINDOWS_PACKAGE_FONT_NOTICE_TAMPERED: expected bytes=$runtimeFontNoticeExpectedBytes sha256=$runtimeFontNoticeExpectedSha256; actual bytes=$($item.Length) sha256=$sha256 path=$resolved"
    }
    return $resolved
}

function Copy-Payload {
    param(
        [string]$RepoRoot,
        [string]$TargetDir,
        [string]$StageRoot,
        [string]$OrtDylibPath,
        [string]$RuntimeFontNoticePath
    )
    $appDir = Join-Path $StageRoot "app"
    $licensesDir = Join-Path $StageRoot "licenses"
    New-Item -ItemType Directory -Force -Path $appDir | Out-Null
    New-Item -ItemType Directory -Force -Path $licensesDir | Out-Null
    foreach ($binary in @("litd-editor.exe", "litd-game.exe")) {
        $source = Join-Path $TargetDir $binary
        if (-not (Test-Path -LiteralPath $source)) {
            throw "missing packaged binary: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $appDir $binary)
    }
    Copy-Item -LiteralPath $OrtDylibPath -Destination (Join-Path $appDir "onnxruntime.dll")
    Copy-Item -LiteralPath $RuntimeFontNoticePath -Destination (Join-Path $StageRoot $runtimeFontNoticeRelativePath)
    Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination (Join-Path $StageRoot "README.md")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "Cargo.lock") -Destination (Join-Path $StageRoot "Cargo.lock")
}

function Get-RuntimeFontNoticeRow {
    param([string]$StageRoot)
    $path = Join-Path $StageRoot $runtimeFontNoticeRelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "LITD_WINDOWS_PACKAGE_FONT_NOTICE_MISSING: staged runtime font notice not found at $path"
    }
    $item = Get-Item -LiteralPath $path
    $sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if ($item.Length -ne $runtimeFontNoticeExpectedBytes -or $sha256 -ne $runtimeFontNoticeExpectedSha256) {
        throw "LITD_WINDOWS_PACKAGE_FONT_NOTICE_TAMPERED: staged notice expected bytes=$runtimeFontNoticeExpectedBytes sha256=$runtimeFontNoticeExpectedSha256; actual bytes=$($item.Length) sha256=$sha256 path=$path"
    }
    [ordered]@{
        dependency = "epaint_default_fonts"
        dependency_version = "0.33.3"
        license = "Ubuntu-font-1.0"
        path = $runtimeFontNoticeRelativePath
        bytes = $item.Length
        sha256 = $sha256
    }
}

function Get-OrtDependencyRow {
    param([string]$StageRoot)
    $path = Join-Path $StageRoot "app/onnxruntime.dll"
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "LITD_WINDOWS_PACKAGE_ORT_MISSING: staged ONNX Runtime DLL not found at $path"
    }
    $item = Get-Item -LiteralPath $path
    $version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($path)
    [ordered]@{
        name = "onnxruntime"
        path = "app/onnxruntime.dll"
        bytes = $item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        file_version = $version.FileVersion
        product_version = $version.ProductVersion
    }
}

function Get-HashRows {
    param([string]$StageRoot)
    Get-ChildItem -LiteralPath $StageRoot -File -Recurse |
        Sort-Object { $_.FullName.Substring($StageRoot.Length).Replace('\', '/') } |
        ForEach-Object {
            $relative = $_.FullName.Substring($StageRoot.Length).TrimStart('\', '/').Replace('\', '/')
            [ordered]@{
                path = $relative
                bytes = $_.Length
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            }
        }
}

function Get-WindowsPeSubsystem {
    param([string]$Path)
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $bytes = [System.IO.File]::ReadAllBytes($resolved)
    if ($bytes.Length -lt 0x80) {
        throw "invalid PE file, too small: $resolved"
    }
    if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "invalid PE file, missing MZ header: $resolved"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
    $optionalHeaderOffset = $peOffset + 24
    if ($peOffset -lt 0 -or $bytes.Length -lt ($optionalHeaderOffset + 72)) {
        throw "invalid PE file, optional header is truncated: $resolved"
    }
    $signature = [System.Text.Encoding]::ASCII.GetString($bytes, $peOffset, 4)
    if ($signature -ne "PE`0`0") {
        throw "invalid PE file, missing PE signature: $resolved"
    }
    $magic = [BitConverter]::ToUInt16($bytes, $optionalHeaderOffset)
    if ($magic -ne 0x10b -and $magic -ne 0x20b) {
        throw "unsupported PE optional-header magic 0x$($magic.ToString('x')): $resolved"
    }
    $subsystemValue = [BitConverter]::ToUInt16($bytes, $optionalHeaderOffset + 68)
    $subsystemName = switch ($subsystemValue) {
        2 { "windows_gui" }
        3 { "windows_console" }
        default { "unknown_$subsystemValue" }
    }
    [ordered]@{
        path = $resolved
        pe_optional_magic = "0x$($magic.ToString('x'))"
        subsystem_value = $subsystemValue
        subsystem = $subsystemName
    }
}

function Get-BinarySubsystemRows {
    param([string]$StageRoot)
    foreach ($binary in @("litd-editor.exe", "litd-game.exe")) {
        $relative = "app/$binary"
        $row = Get-WindowsPeSubsystem -Path (Join-Path $StageRoot $relative)
        [ordered]@{
            path = $relative
            pe_optional_magic = $row.pe_optional_magic
            subsystem_value = $row.subsystem_value
            subsystem = $row.subsystem
        }
    }
}

function Assert-EditorGuiSubsystem {
    param([string]$StageRoot)
    $editorPath = Join-Path $StageRoot "app/litd-editor.exe"
    $row = Get-WindowsPeSubsystem -Path $editorPath
    if ($row.subsystem_value -ne 2) {
        throw "litd-editor.exe must use Windows GUI subsystem 2 to avoid a visible console window; got subsystem $($row.subsystem_value) ($($row.subsystem)) at $editorPath"
    }
}

function Write-PackageManifest {
    param(
        [string]$RepoRoot,
        [string]$StageRoot,
        [string]$ManifestPath,
        [string]$Configuration,
        [string]$PackageVersion,
        [string]$SourceDateEpoch
    )
    $gitCommit = (git -C $RepoRoot rev-parse HEAD).Trim()
    $manifest = [ordered]@{
        schema_version = 2
        package_name = "Light in the Dark"
        package_version = $PackageVersion
        target_os = "windows"
        target_arch = "x64"
        configuration = $Configuration
        source_date_epoch = [Int64]$SourceDateEpoch
        git_commit = $gitCommit
        binaries = @("app/litd-editor.exe", "app/litd-game.exe")
        binary_subsystems = @(Get-BinarySubsystemRows -StageRoot $StageRoot)
        runtime_dependencies = @(
            Get-OrtDependencyRow -StageRoot $StageRoot
        )
        licenses = @(
            Get-RuntimeFontNoticeRow -StageRoot $StageRoot
        )
        runtime_environment = [ordered]@{
            ORT_DYLIB_PATH = "app/onnxruntime.dll"
            CALYX_ORT_CAPI = "app"
        }
        macos_linux_status = "parked"
        installer = [ordered]@{
            wix_template = "packaging/windows/litd.wxs"
            package_version = $PackageVersion
            code_sign_ready = $true
        }
        files = @(Get-HashRows -StageRoot $StageRoot)
    }
    $json = $manifest | ConvertTo-Json -Depth 8
    Set-Content -LiteralPath $ManifestPath -Value $json -Encoding UTF8
}

function New-ReproducibleZip {
    param(
        [string]$StageRoot,
        [string]$ZipPath,
        [string]$SourceDateEpoch
    )
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    $timestamp = [DateTimeOffset]::FromUnixTimeSeconds([Int64]$SourceDateEpoch)
    $zip = [System.IO.Compression.ZipFile]::Open($ZipPath, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        Get-ChildItem -LiteralPath $StageRoot -File -Recurse |
            Sort-Object { $_.FullName.Substring($StageRoot.Length).Replace('\', '/') } |
            ForEach-Object {
                $relative = $_.FullName.Substring($StageRoot.Length).TrimStart('\', '/').Replace('\', '/')
                $entry = $zip.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = $timestamp
                $source = $_.OpenRead()
                try {
                    $dest = $entry.Open()
                    try {
                        $source.CopyTo($dest)
                    } finally {
                        $dest.Dispose()
                    }
                } finally {
                    $source.Dispose()
                }
            }
    } finally {
        $zip.Dispose()
    }
}

function Invoke-Wix {
    param(
        [string]$RepoRoot,
        [string]$StageRoot,
        [string]$MsiPath,
        [string]$WixPath,
        [string]$PackageVersion
    )
    $template = Join-Path $RepoRoot "packaging/windows/litd.wxs"
    Invoke-Checked -FilePath $WixPath -WorkingDirectory $RepoRoot -Arguments @(
        "build", $template,
        "-arch", "x64",
        "-d", "SourceDir=$StageRoot",
        "-d", "RepoRoot=$RepoRoot",
        "-d", "PackageVersion=$PackageVersion",
        "-out", $MsiPath
    )
}

$repoRoot = Get-RepoRoot
$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot } else { Join-Path $repoRoot $OutputRoot }
$stageRoot = Join-Path $outputRootPath "stage"
$manifestPath = Join-Path $stageRoot "package-manifest.json"
$zipPath = Join-Path $outputRootPath "litd-windows-x64.zip"
$releaseManifestPath = Join-Path $outputRootPath "release-manifest.json"
$msiPath = Join-Path $outputRootPath "litd-windows-x64.msi"
$msiReadbackPath = Join-Path $outputRootPath "msi-readback.json"
$targetDir = Get-TargetDir -RepoRoot $repoRoot -Configuration $Configuration
$packageVersionContract = ConvertTo-LitdWindowsPackageVersion -Value $PackageVersion
$packageVersion = $packageVersionContract.text
$signingConfiguration = Get-LitdWindowsSigningConfiguration `
    -SignToolPath $SignToolPath `
    -CertificateThumbprint $CertificateThumbprint `
    -TimestampUrl $TimestampUrl
$ortDylib = Resolve-OrtDylibInput -Path $OrtDylibPath
$runtimeFontNotice = Resolve-RuntimeFontNoticeInput -RepoRoot $repoRoot -Path $RuntimeFontNoticePath

if (-not $SkipBuild) {
    Invoke-Checked -FilePath "cargo" -WorkingDirectory $repoRoot -Arguments (Get-CargoBuildArguments -Configuration $Configuration)
}

New-CleanDirectory -Path $stageRoot
Copy-Payload -RepoRoot $repoRoot -TargetDir $targetDir -StageRoot $stageRoot -OrtDylibPath $ortDylib -RuntimeFontNoticePath $runtimeFontNotice
Assert-EditorGuiSubsystem -StageRoot $stageRoot
$binarySigning = @(
    Invoke-LitdWindowsAuthenticodeSigning -Configuration $signingConfiguration `
        -Artifact (Join-Path $stageRoot "app/litd-editor.exe") -Label "editor"
    Invoke-LitdWindowsAuthenticodeSigning -Configuration $signingConfiguration `
        -Artifact (Join-Path $stageRoot "app/litd-game.exe") -Label "game"
)
Write-PackageManifest -RepoRoot $repoRoot -StageRoot $stageRoot -ManifestPath $manifestPath -Configuration $Configuration -PackageVersion $packageVersion -SourceDateEpoch $SourceDateEpoch
New-ReproducibleZip -StageRoot $stageRoot -ZipPath $zipPath -SourceDateEpoch $SourceDateEpoch
$zipSigning = Test-LitdWindowsPortableZipSigningCoverage `
    -ZipPath $zipPath `
    -StageRoot $stageRoot `
    -BinarySigningRows $binarySigning `
    -SigningEnabled $signingConfiguration.enabled
$gitCommit = (git -C $repoRoot rev-parse HEAD).Trim()

$installerBuilt = $false
$installerSigning = $null
$msiReadback = $null
if ($BuildInstaller -and -not $NoInstaller) {
    Invoke-Wix -RepoRoot $repoRoot -StageRoot $stageRoot -MsiPath $msiPath -WixPath $WixPath -PackageVersion $packageVersion
    $installerBuilt = $true
    $installerSigning = Invoke-LitdWindowsAuthenticodeSigning `
        -Configuration $signingConfiguration `
        -Artifact $msiPath `
        -Label "installer"
    $msiReadback = & (Join-Path $PSScriptRoot "verify-windows-msi.ps1") `
        -MsiPath $msiPath `
        -StageRoot $stageRoot `
        -ExpectedPackageVersion $packageVersion `
        -OutputPath $msiReadbackPath
}

$releaseManifestArgs = @{
    Path = $releaseManifestPath
    PackageVersion = $packageVersion
    GitCommit = $gitCommit
    PortableZipPath = $zipPath
    PrimaryArtifact = if ($installerBuilt) { "windows_msi" } else { "portable_zip" }
}
if ($installerBuilt) {
    $releaseManifestArgs.WindowsMsiPath = $msiPath
}
Write-LitdWindowsReleaseManifest @releaseManifestArgs | Out-Null

$readback = [ordered]@{
    package_name = "Light in the Dark"
    target_os = "windows"
    target_arch = "x64"
    configuration = $Configuration
    package_version = $packageVersion
    stage_root = $stageRoot
    manifest_path = $manifestPath
    zip_path = $zipPath
    zip_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash.ToLowerInvariant()
    release_manifest = (Get-LitdWindowsReleaseManifestRow -Path $releaseManifestPath)
    file_count = (Get-HashRows -StageRoot $stageRoot).Count
    editor_present = Test-Path -LiteralPath (Join-Path $stageRoot "app/litd-editor.exe")
    game_present = Test-Path -LiteralPath (Join-Path $stageRoot "app/litd-game.exe")
    binary_subsystems = @(Get-BinarySubsystemRows -StageRoot $stageRoot)
    ort_dependency = (Get-OrtDependencyRow -StageRoot $stageRoot)
    runtime_font_notice = (Get-RuntimeFontNoticeRow -StageRoot $stageRoot)
    runtime_environment = [ordered]@{
        ORT_DYLIB_PATH = "app/onnxruntime.dll"
        CALYX_ORT_CAPI = "app"
    }
    installer_template = "packaging/windows/litd.wxs"
    installer_built = $installerBuilt
    installer_readback = $msiReadback
    signing = [ordered]@{
        schema = "litd.windows.signing.v1"
        enabled = $signingConfiguration.enabled
        mode = $signingConfiguration.mode
        sign_tool = if ($signingConfiguration.enabled) {
            [IO.Path]::GetFileName($signingConfiguration.sign_tool_path)
        } else {
            $null
        }
        certificate_thumbprint = if ($signingConfiguration.enabled) {
            $signingConfiguration.certificate_thumbprint
        } else {
            $null
        }
        timestamp_url = if ($signingConfiguration.enabled) {
            $signingConfiguration.timestamp_url
        } else {
            $null
        }
        binaries = $binarySigning
        portable_zip = $zipSigning
        installer = $installerSigning
    }
    code_sign_ready = $true
    macos_linux_status = "parked"
}
$readbackPath = Join-Path $outputRootPath "package-readback.json"
$readback | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $readbackPath -Encoding UTF8
$readback | ConvertTo-Json -Depth 8
