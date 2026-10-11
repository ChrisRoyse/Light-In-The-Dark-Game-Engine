#requires -Version 5.1
<#
.SYNOPSIS
    Reference-artifact doctrine gate (#1510). A reference image declared a quality/layout
    FLOOR must not silently contradict binding doctrine.

.DESCRIPTION
    Root cause of #1510: docs/readme/ED12-play-topdown.png visibly carries a circular
    minimap, quest-tracker panels and a player-position "Here" marker. The acquired-map
    doctrine (#944 / ADR-944, #800 SS17, #1447, #1448) forbids every one of those surfaces in
    the packaged player runtime. The image was nevertheless declared a quality/layout floor
    by #780/#800, so a future agent reading the reference set would have re-implemented
    exactly the omniscient navigation the operator prohibited. A doctrine and an artifact
    drifted apart and NOTHING mechanically checked that they still agreed.

    Prose warnings rot. This gate makes the agreement machine-checkable, in the same shape as
    the #1465 doctrine-refs gate (curated manifest + fast git-only scanner, run on every
    commit from scripts/check-format.ps1; this repo has no CI by permanent decision).

    Source of truth: scripts/reference-artifact-doctrine.tsv. One tab-separated row per
    reference artifact:

        <artifact-path>  <status>  <annotation>  <refs>

    status is one of:
      NOT_RUNTIME    - does not depict the packaged player runtime (concept art, lore plate,
                       marketing hero, menu screen, or editor-only tooling UI). Editor
                       authoring overviews - including an editor minimap panel - are creator
                       tooling and are explicitly allowed.
      RUNTIME_CLEAN  - depicts packaged player runtime and is doctrine-clean: no built-in
                       map/minimap, no GPS player dot, no quest pins/tracker, no route
                       ribbon, no automatic exploration-unlocked coverage.
      FORBIDDEN_NAV  - depicts packaged player runtime containing at least one surface the
                       acquired-map doctrine FORBIDS. The artifact is OBSOLETE on those
                       elements and MUST NOT be presented as a floor without saying so.

    <annotation> is the literal token that every tracked markdown presentation of a
    FORBIDDEN_NAV artifact must carry near the reference. For NOT_RUNTIME / RUNTIME_CLEAN
    rows it must be exactly '-'.

    Enforced invariants (each FAILS the commit, naming the file and the fix):
      1. MANIFEST_MALFORMED   - a row is not 4 non-empty tab-separated fields, has an unknown
                                status, a FORBIDDEN_NAV row with an empty/'-' annotation, or a
                                non-FORBIDDEN_NAV row carrying an annotation.
      2. MANIFEST_UNTRACKED   - a manifest artifact is not git-tracked (it would not exist on
                                a clean checkout, so its classification is vapor).
      3. UNCLASSIFIED         - a git-tracked docs/readme/*.png has NO manifest row. A new
                                reference artifact cannot enter the repo unclassified.
      4. DANGLING_REF         - a tracked .md references a docs/readme/*.png that is not
                                git-tracked.
      5. UNANNOTATED_FLOOR    - a tracked .md references a FORBIDDEN_NAV artifact without its
                                required annotation token within +/-8 lines. This is the #1510
                                defect itself: a doctrine-forbidden reference presented as a
                                floor with nothing marking it obsolete.

    Calyx/ markdown is exempt (it documents Calyx-Dev's own tree and tracker).

.EXAMPLE
    powershell -NoProfile -File scripts\check-reference-artifacts.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$manifestPath = Join-Path $PSScriptRoot "reference-artifact-doctrine.tsv"
$manifestRel = "scripts/reference-artifact-doctrine.tsv"

# How far from a reference line the annotation token may sit and still count as attached to
# it. A markdown image line plus its caption/bullet body is well inside this window.
$annotationWindow = 8

$validStatuses = @("NOT_RUNTIME", "RUNTIME_CLEAN", "FORBIDDEN_NAV")

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "REFERENCE_ARTIFACTS_MANIFEST_MISSING: $manifestPath not found. Every reference artifact must carry a doctrine classification (#1510)."
}

$trackedPaths = @(& git -C $repoRoot ls-files)
if ($LASTEXITCODE -ne 0) {
    throw "REFERENCE_ARTIFACTS_GATE_FAILED: step=git-ls-files exit_code=$LASTEXITCODE"
}
$trackedSet = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($path in $trackedPaths) { [void]$trackedSet.Add(($path -replace '\\', '/')) }

$failures = New-Object System.Collections.Generic.List[string]

# --- Parse + validate the manifest (invariant 1) ----------------------------------------
$status = @{}       # artifact -> status
$annotation = @{}   # artifact -> required token
$rowNo = 0
foreach ($line in (Get-Content -LiteralPath $manifestPath)) {
    $rowNo += 1
    $trimmed = $line.Trim()
    if ($trimmed -eq '' -or $trimmed.StartsWith('#')) { continue }

    $fields = $line -split "`t"
    if ($fields.Count -ne 4) {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} has $($fields.Count) tab-separated field(s), expected 4: <artifact> <status> <annotation> <refs>. Fix the row.")
        continue
    }

    $artifact = $fields[0].Trim()
    $rowStatus = $fields[1].Trim()
    $rowAnnotation = $fields[2].Trim()
    $rowRefs = $fields[3].Trim()

    if ($artifact -eq '' -or $rowStatus -eq '' -or $rowAnnotation -eq '' -or $rowRefs -eq '') {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} has an empty field (artifact='$artifact' status='$rowStatus' annotation='$rowAnnotation' refs='$rowRefs'). All 4 fields are required; use '-' for an annotation that does not apply.")
        continue
    }
    if ($validStatuses -notcontains $rowStatus) {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} artifact '$artifact' has unknown status '$rowStatus'. Valid: $($validStatuses -join ', ').")
        continue
    }
    if ($status.ContainsKey($artifact)) {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} artifact '$artifact' is listed more than once. Keep exactly one row per artifact.")
        continue
    }
    if ($rowStatus -eq 'FORBIDDEN_NAV' -and $rowAnnotation -eq '-') {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} artifact '$artifact' is FORBIDDEN_NAV but its annotation is '-'. A doctrine-forbidden artifact MUST declare the literal token that every markdown presentation of it has to carry (e.g. DOCTRINE-OBSOLETE-NAV(#944)). Set column 3 to that token.")
        continue
    }
    if ($rowStatus -ne 'FORBIDDEN_NAV' -and $rowAnnotation -ne '-') {
        $failures.Add("MANIFEST_MALFORMED ${manifestRel}:${rowNo} artifact '$artifact' is $rowStatus but declares annotation '$rowAnnotation'. Only FORBIDDEN_NAV rows require an annotation token; set column 3 to '-'.")
        continue
    }

    $status[$artifact] = $rowStatus
    $annotation[$artifact] = $rowAnnotation
}

# --- Manifest artifacts must be git-tracked (invariant 2) --------------------------------
foreach ($artifact in $status.Keys) {
    if (-not $trackedSet.Contains($artifact)) {
        $failures.Add("MANIFEST_UNTRACKED '$artifact' is classified in $manifestRel but is NOT git-tracked. A clean checkout would not contain it. Commit the artifact (git add $artifact) or remove its row.")
    }
}

# --- Every tracked reference artifact must be classified (invariant 3) --------------------
$artifactPattern = '^docs/readme/.+\.png$'
$trackedArtifacts = @($trackedSet | Where-Object { $_ -match $artifactPattern } | Sort-Object)
foreach ($artifact in $trackedArtifacts) {
    if (-not $status.ContainsKey($artifact)) {
        $failures.Add("UNCLASSIFIED '$artifact' is a tracked reference artifact with NO row in $manifestRel. Every reference image is a declared quality/layout floor (#780/#800), so it must state whether it agrees with the acquired-map doctrine (#944). Look at the image and add a row: '$artifact<TAB>NOT_RUNTIME|RUNTIME_CLEAN|FORBIDDEN_NAV<TAB><annotation-or-dash><TAB><refs>'.")
    }
}

# --- Scan tracked markdown (invariants 4 + 5) --------------------------------------------
# Files exempt from the ANNOTATION requirement only (invariant 5). These mention artifact
# filenames as DATA (e.g. a repo-size table listing the largest PNGs), not as visual floors,
# so demanding an obsolescence caption there would be noise. They are still fully subject to
# invariant 4 (dangling references). Keep this list tiny; an exemption is not a licence to
# present a forbidden artifact as a target.
$annotationExemptPath = Join-Path $PSScriptRoot "reference-artifact-allowlist.txt"
$annotationExempt = New-Object 'System.Collections.Generic.HashSet[string]'
if (Test-Path -LiteralPath $annotationExemptPath -PathType Leaf) {
    foreach ($line in (Get-Content -LiteralPath $annotationExemptPath)) {
        $t = $line.Trim()
        if ($t -eq '' -or $t.StartsWith('#')) { continue }
        [void]$annotationExempt.Add($t)
    }
}

$mdFiles = @($trackedSet | Where-Object { $_ -like '*.md' -and $_ -notlike 'Calyx/*' } | Sort-Object)

# Two reference forms both count as "presenting" an artifact:
#   1. a repo-root path:  docs/readme/ED12-play-topdown.png   (root README, other docs)
#   2. a bare filename:   ED12-play-topdown.png               (relative links inside
#      docs/readme/README.md - the reference INDEX itself - and continuation lists such as
#      docs/assets/asset-foundation.md). A bare name only counts when it matches a known
#      manifest artifact, so unrelated .png names elsewhere are never misread as references.
$pathRefPattern = '(?<![\w.\\/-])docs/readme/[\w.-]+\.png'
$bareRefPattern = '(?<![\w.\\/-])[\w-]+\.png'
$byBaseName = @{}
foreach ($artifact in $status.Keys) { $byBaseName[($artifact -replace '^docs/readme/', '')] = $artifact }

$refsChecked = 0

foreach ($rel in $mdFiles) {
    $full = Join-Path $repoRoot ($rel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { continue }

    $lines = @(Get-Content -LiteralPath $full)
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $refsOnLine = New-Object 'System.Collections.Generic.HashSet[string]'
        foreach ($m in [regex]::Matches($lines[$i], $pathRefPattern)) {
            [void]$refsOnLine.Add($m.Value)
        }
        foreach ($m in [regex]::Matches($lines[$i], $bareRefPattern)) {
            if ($byBaseName.ContainsKey($m.Value)) { [void]$refsOnLine.Add($byBaseName[$m.Value]) }
        }

        foreach ($ref in $refsOnLine) {
            $refsChecked += 1
            $lineNo = $i + 1

            if (-not $trackedSet.Contains($ref)) {
                $failures.Add("DANGLING_REF ${rel}:${lineNo} references '$ref', which is NOT git-tracked. A reference artifact that does not exist on a clean checkout cannot be a floor. Commit the image or fix the reference.")
                continue
            }
            if (-not $status.ContainsKey($ref)) {
                # Already reported as UNCLASSIFIED above; do not double-report per reference.
                continue
            }
            if ($status[$ref] -ne 'FORBIDDEN_NAV') { continue }
            if ($annotationExempt.Contains($rel)) { continue }

            $token = $annotation[$ref]
            $from = [Math]::Max(0, $i - $annotationWindow)
            $to = [Math]::Min($lines.Count - 1, $i + $annotationWindow)
            $found = $false
            for ($j = $from; $j -le $to; $j++) {
                if ($lines[$j].Contains($token)) { $found = $true; break }
            }
            if (-not $found) {
                $failures.Add("UNANNOTATED_FLOOR ${rel}:${lineNo} presents '$ref' as a reference/floor, but that artifact is classified FORBIDDEN_NAV in $manifestRel (it depicts omniscient navigation the acquired-map doctrine forbids in packaged player runtime: built-in map/minimap, GPS player dot, quest pins/tracker, route ribbon, automatic exploration-unlocked coverage) and the required annotation token '$token' does not appear within $annotationWindow lines. FIX: add '$token' to the caption/bullet for this reference, stating that those surfaces are OBSOLETE and MUST NOT be implemented (see #944 / ADR-944, #1510, #1537) - or, if the artifact has been replaced with a doctrine-clean capture, reclassify its row in $manifestRel.")
            }
        }
    }
}

$forbiddenCount = @($status.Values | Where-Object { $_ -eq 'FORBIDDEN_NAV' }).Count
Write-Host "REFERENCE_ARTIFACTS_GATE classified=$($status.Count) tracked_artifacts=$($trackedArtifacts.Count) forbidden_nav=$forbiddenCount md_files_scanned=$($mdFiles.Count) refs_checked=$refsChecked"

if ($failures.Count -gt 0) {
    Write-Host ""
    foreach ($f in $failures) { Write-Host "  $f" }
    Write-Host ""
    throw "REFERENCE_ARTIFACTS_GATE_FAILED count=$($failures.Count): a reference artifact and the binding acquired-map doctrine have drifted apart (#1510). Fix as listed above."
}
Write-Host "REFERENCE_ARTIFACTS_GATE_OK every reference artifact is doctrine-classified and every FORBIDDEN_NAV presentation carries its obsolescence annotation."
