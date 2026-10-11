function Get-RustTestExecutionCount {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Rust test stdout log is missing: $Path"
    }
    [long]$count = 0
    foreach ($line in Get-Content -LiteralPath $Path) {
        $match = [regex]::Match($line, '^\s*running\s+(\d+)\s+tests?\s*$')
        if ($match.Success) {
            $count += [long]::Parse($match.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
        }
    }
    return $count
}

function Get-RustHarnessInvocationCount {
    <#
        Counts libtest harness invocations ("running N tests" lines) in a stdout log.

        FSV example targets are declared `harness = false`, so `cargo test --example` runs the
        example's `main()` directly and NEVER emits a "running N tests" line. If one appears in
        an --example run, `harness = false` was dropped (the #1497 regression fixed by ad1ee0af /
        tracked in #1527): the example ran under libtest, executed zero `#[test]` functions, and
        "passed" with exit 0 having run none of its FSV assertions - a false-green. The gate uses
        this count to fail loud instead of green-lighting a no-op FSV run. Unlike
        Get-RustTestExecutionCount (which sums N and so cannot tell a harnessless main() apart
        from a hollow "running 0 tests"), this counts the presence of the harness line itself.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Rust test stdout log is missing: $Path"
    }
    [long]$count = 0
    foreach ($line in Get-Content -LiteralPath $Path) {
        if ([regex]::IsMatch($line, '^\s*running\s+\d+\s+tests?\s*$')) {
            $count += 1
        }
    }
    return $count
}
