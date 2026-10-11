<#
    Detection and LOUD, BOUNDED recovery for rustc incremental-compilation cache corruption
    (#1536).

    THE BUG (upstream, open, not ours)
    ----------------------------------
    rust-lang/rust#81280 -- "Incremental occasionally causes link failures (in particular when
    build process is interrupted)" -- labels A-incr-comp / A-linkage / T-compiler / C-bug, OPEN
    since 2021. rustc caches one object file per codegen unit as an incremental "work product"
    under target\<profile>\incremental\<crate>-<hash>\s-<session>\<id>.o. On a rebuild, a CGU
    whose fingerprint is unchanged is NOT recompiled: its cached .o is reused verbatim.

    Debug builds in this workspace run incremental ThinLTO (the .pre-lto.bc files sitting next
    to each .o in the session dir are the evidence). ThinLTO imports functions ACROSS the CGUs
    of a crate, and when an imported function references a module-local anonymous constant,
    LLVM promotes that constant to external linkage and renames it
    anon.<module-hash>.<n>.llvm.<discriminator>, exporting it from the defining CGU. The
    discriminator is derived from the import graph of THAT compilation.

    So if even ONE cached .o survives from a compilation with a different import graph while
    its sibling CGUs are freshly codegen'd, the fresh CGUs reference a promoted symbol that the
    stale CGU no longer defines under that name, and the link dies with:

        rust-lld: error: undefined symbol: anon.<32 hex>.<n>.llvm.<digits>
        >>> referenced by serde_core-1.0.228/src/de/mod.rs:495

    rustc DOES self-protect the incremental METADATA (a corrupt dep-graph.bin / query-cache.bin
    is detected, ignored and deleted -- see the `incremental_corrupt_file` diagnostic). It does
    NOT checksum the cached .o work products. That asymmetry is precisely why this class escapes
    every rustc-level guard and only ever surfaces at LINK time.

    WHY THIS MATTERS HERE: it is a FALSE-RED. The source is correct, the gate goes red anyway.
    A gate that cries wolf trains agents and humans to wave build failures through, which is how
    a real regression eventually ships. This is an FSV-integrity bug, not a papercut.

    WHAT WE DO NOT DO
    -----------------
    We do NOT disable incremental. Incremental is a 4.6x build-time lever on this workspace
    (20s vs 93s warm) and turning it off trades a rare correctness bug for a permanent tax.
    We do NOT silently retry, either: a swallowed recovery is how a real bug hides inside a
    "flaky build" story. Recovery is LOUD, STRUCTURED, COUNTED, and STRICTLY ONCE -- a second
    occurrence in the same run is not a flake, it is a real bug, and it FAILS HARD.
#>

Set-StrictMode -Version Latest

# The signature. `.llvm.<digits>` is the suffix LLVM appends when it promotes a module-LOCAL
# symbol to external linkage for ThinLTO import. It cannot appear on a symbol that came from
# source. So an "undefined symbol" line carrying a `.llvm.<digits>` discriminator is, by
# construction, a stale-codegen-unit artifact and NEVER a genuine missing-symbol bug (a real
# one -- a missing extern "C" fn, an unlinked native lib -- has no discriminator). That
# specificity is what lets us auto-recover without any risk of swallowing a true link error.
#
# Covers all three linker dialects so the detector survives a linker change:
#   rust-lld / ld.lld : undefined symbol: anon.<hex>.<n>.llvm.<digits>
#   MSVC link.exe     : unresolved external symbol anon.<hex>.<n>.llvm.<digits>
#   GNU ld            : undefined reference to `anon...' / undefined hidden symbol `anon...'
$script:LitdIncrementalCorruptionPattern =
    '(?i)(undefined symbol|unresolved external symbol|undefined reference to|undefined hidden symbol)[^\r\n]*\.llvm\.\d+'

# Crate name as it appears in an rcgu object path. Real names on this workspace carry a VARIABLE
# number of dot-separated segments between the crate hash and `.rcgu.o` -- verified against actual
# artifacts in target\debug\deps, e.g.
#   litd_editor_mcp-604aac33c8551a71.1a2l6xht3w7v3a3hf8g6b0t6p.1h4rvcj.rcgu.o   (three segments)
#   librustc_resolve-fac19ceb16326411.rlib(rustc_resolve-fac19ceb16326411.2u8t.rcgu.o)  (one)
# so the segment group MUST repeat (+). An earlier single-segment version of this pattern silently
# matched nothing on this repo's real artifacts and would have extracted no crate at all.
# Group 1 is the crate's snake_case name, which is also the incremental dir prefix.
$script:LitdRcguCratePattern = '(?i)(?:^|[\\/(])(?:lib)?([A-Za-z0-9_]+?)-[0-9a-f]{8,}(?:\.[0-9a-z]+)+\.rcgu\.o'

function Get-LitdIncrementalCorruption {
    <#
        Scan a linker/cargo log for the stale-work-product signature.

        Returns $null when the log is clean or absent (so a genuine compile error, a genuine
        link error, a test failure -- anything that is NOT this corruption class -- flows
        through untouched and fails the gate exactly as it does today).

        On a match, returns the evidence: the offending symbols, the crates whose rcgu objects
        referenced them, and the raw matched lines.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [string[]]$Paths,

        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$Package
    )

    $matchedLines = New-Object System.Collections.ArrayList
    $symbols = New-Object "System.Collections.Generic.HashSet[string]"
    $crates = New-Object "System.Collections.Generic.HashSet[string]"

    foreach ($path in @($Paths)) {
        if (-not $path -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        $lines = @(Get-Content -LiteralPath $path -ErrorAction SilentlyContinue)
        for ($i = 0; $i -lt $lines.Count; $i++) {
            $line = $lines[$i]
            if ($null -eq $line) { continue }
            if (-not [regex]::IsMatch($line, $script:LitdIncrementalCorruptionPattern)) { continue }

            [void]$matchedLines.Add($line.Trim())
            $symbolMatch = [regex]::Match($line, '(?i)([A-Za-z0-9_.$]*\.llvm\.\d+)')
            if ($symbolMatch.Success) { [void]$symbols.Add($symbolMatch.Groups[1].Value) }

            # rust-lld reports the referencing object on the FOLLOWING ">>>" continuation
            # lines, not on the "undefined symbol:" line itself. Look ahead a bounded window
            # to attribute the corruption to the crate that owns the stale work product.
            $lookaheadEnd = [Math]::Min($i + 6, $lines.Count - 1)
            for ($j = $i; $j -le $lookaheadEnd; $j++) {
                $cratehit = [regex]::Match([string]$lines[$j], $script:LitdRcguCratePattern)
                if ($cratehit.Success) { [void]$crates.Add($cratehit.Groups[1].Value) }
            }
        }
    }

    if ($matchedLines.Count -eq 0) { return $null }

    # The package under test is ALWAYS a clearing candidate: it is the crate being linked, so
    # its incremental cache is implicated even when rust-lld attributes the reference to a
    # dependency's rcgu (and even if the log format changes and we extract no crate at all).
    [void]$crates.Add(($Package -replace '-', '_'))

    return [pscustomobject]@{
        symbols = @($symbols)
        crates = @($crates | Sort-Object)
        matched_lines = @($matchedLines | Select-Object -First 20)
        match_count = $matchedLines.Count
    }
}

function Resolve-LitdCargoTargetDir {
    <#
        The target dir cargo will actually build into: an explicit --target-dir wins, then
        CARGO_TARGET_DIR, then <repo>\target. Recovery must clear the cache cargo will READ,
        not a directory we assumed it would read.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$RepoRoot,

        [AllowNull()]
        [string]$TargetDir
    )

    if ($TargetDir) { return $TargetDir }
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    return (Join-Path $RepoRoot "target")
}

function Clear-LitdIncrementalCache {
    <#
        Remove ONLY the incremental caches of the implicated crates -- never the whole
        incremental dir, never deps\, never the target tree. The target tree is a WARM SHARED
        cache (#1498) and other agents build into it; a blanket wipe would be a denial of
        service on every other agent on this host and would throw away the 4.6x incremental
        win for 40+ untouched crates to fix one crate's stale object.

        Clearing <profile>\incremental\<crate>-* costs exactly one non-incremental rebuild of
        that ONE crate. deps\ is untouched, so the rest of the graph still fingerprint-skips.

        Returns one record per directory considered, with the bytes reclaimed -- the source of
        truth for the gate's structured log and JSON report.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$TargetDir,

        [Parameter(Mandatory = $true)]
        [string[]]$Crates
    )

    $cleared = New-Object System.Collections.ArrayList

    foreach ($buildProfile in @("debug", "release")) {
        $incRoot = Join-Path (Join-Path $TargetDir $buildProfile) "incremental"
        if (-not (Test-Path -LiteralPath $incRoot -PathType Container)) { continue }

        foreach ($crate in @($Crates)) {
            if (-not $crate) { continue }
            # <crate>-<stable-crate-id>. Anchor the prefix so `litd_editor` never matches
            # `litd_editor_something_else`; the id segment is base-36, no dashes.
            $candidates = @(Get-ChildItem -LiteralPath $incRoot -Directory -Force -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match ('^' + [regex]::Escape($crate) + '-[0-9a-z]+$') })

            foreach ($dir in $candidates) {
                $bytes = 0
                try {
                    $bytes = [long]((Get-ChildItem -LiteralPath $dir.FullName -Recurse -File -Force -ErrorAction SilentlyContinue |
                        Measure-Object -Property Length -Sum).Sum)
                } catch { $bytes = 0 }

                $removed = $false
                $errorMessage = $null
                try {
                    Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction Stop
                    $removed = -not (Test-Path -LiteralPath $dir.FullName)
                } catch {
                    $errorMessage = $_.Exception.Message
                }

                [void]$cleared.Add([pscustomobject]@{
                        crate = $crate
                        build_profile = $buildProfile
                        path = $dir.FullName
                        bytes = $bytes
                        removed = $removed
                        exists_after = (Test-Path -LiteralPath $dir.FullName)
                        error_message = $errorMessage
                    })
            }
        }
    }

    return @($cleared)
}

function Write-LitdIncrementalCorruptionLedger {
    <#
        Append one JSONL row per recovery to <FsvRoot>\incremental-cache-corruption.jsonl.

        A recovered corruption that leaves no trace gets NORMALIZED -- it becomes "the build is
        just flaky sometimes" and stops being investigated. The ledger makes the rate visible
        and auditable across runs, so a rising frequency (a genuinely sick host, a bad
        toolchain, an agent that keeps hard-killing cargo) is DETECTABLE rather than absorbed.
        Never fails the caller: a ledger write error must not turn a recovered build red.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string]$FsvRoot,

        [Parameter(Mandatory = $true)]
        [object]$Record
    )

    try {
        $ledgerPath = Join-Path $FsvRoot "incremental-cache-corruption.jsonl"
        $line = ($Record | ConvertTo-Json -Depth 6 -Compress)
        Add-Content -LiteralPath $ledgerPath -Value $line -Encoding UTF8 -ErrorAction Stop
        return $ledgerPath
    } catch {
        Write-Warning "INCREMENTAL_CACHE_LEDGER_WRITE_FAILED: $($_.Exception.Message)"
        return $null
    }
}
