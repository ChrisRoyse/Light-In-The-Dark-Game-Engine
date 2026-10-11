param(
    [string]$FsvRoot = "target\fsv\calyx-side-store-eradication",
    [string[]]$SourceRoot = @(),
    [string[]]$AdditionalSourceRoot = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$reportRoot = if ([System.IO.Path]::IsPathRooted($FsvRoot)) {
    $FsvRoot
} else {
    Join-Path $repoRoot $FsvRoot
}
New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$reportPath = Join-Path $reportRoot "side-store-eradication-$timestamp.json"

$sourceRoots = if ($SourceRoot.Count -gt 0) {
    @($SourceRoot)
} else {
    @(
        "crates\litd-editor\src",
        "crates\litd-engine\src",
        "crates\litd-game\src",
        "crates\litd-editor-mcp\src",
        "crates\litd-hash\src"
    )
}
if ($AdditionalSourceRoot.Count -gt 0) {
    $sourceRoots = @($sourceRoots + $AdditionalSourceRoot)
}

$excludedPathPatterns = @(
    "\\examples\\",
    "_tests\.rs$",
    "\\tests\.rs$",
    "issue\d+_.*tests\.rs$",
    "\\fsv_artifacts\.rs$",
    "\\main_build_identity\.rs$",
    "\\main_lifecycle\.rs$",
    "\\fail_loud\.rs$",
    "test_support\.rs$"
)

$allowedWritePathPatterns = @(
    "\\quick_share\.rs$",
    "\\mcp_project_delivery_store\.rs$",
    "\\render\\font_store\.rs$",
    "\\render\\.*_gpu_image\.rs$",
    "\\render\\reference_image\.rs$",
    "\\render\\ui_perf_overlay\.rs$",
    "\\render\\ui_text_preview\.rs$",
    "\\texture_import\\artifacts\.rs$",
    "\\terrain_heightmap_io\.rs$",
    "\\main_fsv\.rs$",
    "\\profile\.rs$",
    "\\audio\.rs$",
    "\\audio\\audio_spatial\.rs$",
    "\\lib\.rs$"
)

function Get-RepoRelativePath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $full = (Resolve-Path -LiteralPath $Path).Path
    if ($full.StartsWith($repoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $full.Substring($repoRoot.Length + 1)
    }
    return $full
}

function Test-MatchesAny {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value,

        [Parameter(Mandatory = $true)]
        [string[]]$Patterns
    )
    foreach ($pattern in $Patterns) {
        if ($Value -match $pattern) {
            return $true
        }
    }
    return $false
}

function Add-Violation {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [System.Collections.ArrayList]$Violations,

        [Parameter(Mandatory = $true)]
        [string]$Category,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [int]$Line,

        [Parameter(Mandatory = $true)]
        [string]$Evidence,

        [Parameter(Mandatory = $true)]
        [string]$Reason
    )
    [void]$Violations.Add([ordered]@{
        category = $Category
        path = $Path
        line = $Line
        evidence = $Evidence.Trim()
        reason = $Reason
    })
}

$rustFiles = New-Object System.Collections.ArrayList
$violations = New-Object System.Collections.ArrayList
foreach ($root in $sourceRoots) {
    $absolute = if ([System.IO.Path]::IsPathRooted($root)) {
        $root
    } else {
        Join-Path $repoRoot $root
    }
    if (-not (Test-Path -LiteralPath $absolute -PathType Container)) {
        continue
    }
    Get-ChildItem -LiteralPath $absolute -Recurse -File -Filter "*.rs" | ForEach-Object {
        $relative = Get-RepoRelativePath -Path $_.FullName
        if (-not (Test-MatchesAny -Value $relative -Patterns $excludedPathPatterns)) {
            [void]$rustFiles.Add([pscustomobject]@{
                FullName = $_.FullName
                Relative = $relative
            })
        }
    }
}

if ($rustFiles.Count -eq 0) {
    Add-Violation `
        -Violations $violations `
        -Category "empty_scan" `
        -Path "<scan-roots>" `
        -Line 0 `
        -Evidence ($sourceRoots -join ";") `
        -Reason "side-store eradication gate scanned zero Rust files"
}
$sourceTruthPattern = "source_of_truth.*(JSON|RON|SQLite|sqlite|\.json|\.ron|local store|side store)"
$writePattern = "(fs::write|std::fs::write|OpenOptions::new|File::create)"
$structuredStorePathPattern = "(_store\.rs$|main_editor_persistence\.rs$|editor_preferences_store\.rs$|scenario(_availability)?\.rs$|runtime_state\.rs$)"
$rpgContentFactoryPathPattern = "\\rpg\\.*_content\.rs$"
$rpgAuthoredContentGatePathPattern = "\\rpg\\authored_content\.rs$"
$rpgContentRecordFactoryPattern = "pub\s+fn\s+\w*content_records"

foreach ($file in $rustFiles) {
    $lines = Get-Content -LiteralPath $file.FullName
    $isRpgContentFactoryFile =
        ($file.Relative -match $rpgContentFactoryPathPattern) -and
        ($file.Relative -notmatch $rpgAuthoredContentGatePathPattern)
    $hasRpgContentRecordFactory = $false
    $rpgContentRecordFactoryLine = 0
    $hasRpgAuthoredValidator = $false
    for ($index = 0; $index -lt $lines.Count; $index++) {
        $line = [string]$lines[$index]
        $lineNumber = $index + 1

        if ($line -match $sourceTruthPattern -and $line -notmatch "Calyx Aster") {
            Add-Violation `
                -Violations $violations `
                -Category "source_of_truth_names_side_store" `
                -Path $file.Relative `
                -Line $lineNumber `
                -Evidence $line `
                -Reason "source_of_truth still names JSON/RON/SQLite/local storage instead of Calyx Aster"
        }

        if ($file.Relative -match $structuredStorePathPattern -and $line -match $writePattern) {
            if (-not (Test-MatchesAny -Value $file.Relative -Patterns $allowedWritePathPatterns)) {
                Add-Violation `
                    -Violations $violations `
                    -Category "direct_structured_file_write" `
                    -Path $file.Relative `
                    -Line $lineNumber `
                    -Evidence $line `
                    -Reason "production structured store writes directly to the filesystem"
            }
        }

        if ($isRpgContentFactoryFile) {
            if ($line -match $rpgContentRecordFactoryPattern) {
                $hasRpgContentRecordFactory = $true
                if ($rpgContentRecordFactoryLine -eq 0) {
                    $rpgContentRecordFactoryLine = $lineNumber
                }
            }
            if ($line -match "validate_rpg_authored_content_records") {
                $hasRpgAuthoredValidator = $true
            }
            if ($line -match "rpg://") {
                Add-Violation `
                    -Violations $violations `
                    -Category "rpg_content_compiled_source_literal" `
                    -Path $file.Relative `
                    -Line $lineNumber `
                    -Evidence $line `
                    -Reason "RPG content factory contains a compiled rpg:// source literal instead of editor-authored Calyx provenance"
            }
            if ($line -match "AnchorKind::TestPass") {
                Add-Violation `
                    -Violations $violations `
                    -Category "rpg_content_testpass_anchor" `
                    -Path $file.Relative `
                    -Line $lineNumber `
                    -Evidence $line `
                    -Reason "RPG content factory uses a self-attesting TestPass anchor"
            }
        }
    }

    if (
        $isRpgContentFactoryFile -and
        $hasRpgContentRecordFactory -and
        -not $hasRpgAuthoredValidator
    ) {
        Add-Violation `
            -Violations $violations `
            -Category "rpg_content_missing_authored_validator" `
            -Path $file.Relative `
            -Line $rpgContentRecordFactoryLine `
            -Evidence "pub fn *content_records without validate_rpg_authored_content_records" `
            -Reason "RPG content factories must fail closed through the shared authored-content validator"
    }
}

$status = if ($violations.Count -eq 0) { "passed" } else { "failed" }
$report = [ordered]@{
    schema_version = 1
    status = $status
    issue = 1363
    source_of_truth = "Calyx Aster vault rows are the only permitted authoritative structured-data source of truth"
    scanned_roots = $sourceRoots
    scanned_rust_file_count = $rustFiles.Count
    violation_count = $violations.Count
    violations = @($violations)
    allowed_non_authoritative_write_patterns = $allowedWritePathPatterns
    report_path = $reportPath
    created_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
}

$reportJson = $report | ConvertTo-Json -Depth 10
$reportJson | Set-Content -LiteralPath $reportPath -Encoding UTF8
$readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json

# Compute the report SHA-256 with the .NET crypto primitive directly instead of the
# Get-FileHash cmdlet. Get-FileHash lives in Microsoft.PowerShell.Utility, which does not
# reliably autoload under Windows PowerShell 5.1 on every host (issue #1574: the cmdlet was
# reported as "not recognized" under powershell 5.1 while resolving fine under pwsh 7). The
# .NET primitive is interpreter-independent and produces a byte-identical SHA-256 to
# Get-FileHash, so the gate hashes identically under PS 5.1 and pwsh 7. No fallback: any read
# or hash failure surfaces as a terminating error (ErrorActionPreference = Stop).
$reportBytes = [System.IO.File]::ReadAllBytes($reportPath)
$sha256Provider = [System.Security.Cryptography.SHA256]::Create()
try {
    $reportHashBytes = $sha256Provider.ComputeHash($reportBytes)
} finally {
    $sha256Provider.Dispose()
}
$reportSha256 = ([System.BitConverter]::ToString($reportHashBytes) -replace '-', '').ToLowerInvariant()

Write-Host "CALYX_SIDE_STORE_GATE_READBACK path=$reportPath status=$($readback.status) scanned_files=$($readback.scanned_rust_file_count) violations=$($readback.violation_count) sha256=$reportSha256"
if ($readback.violation_count -gt 0) {
    $readback.violations |
        Select-Object -First 40 |
        ForEach-Object {
            Write-Host "CALYX_SIDE_STORE_VIOLATION category=$($_.category) path=$($_.path) line=$($_.line) reason=$($_.reason)"
        }
    exit 1
}

exit 0
