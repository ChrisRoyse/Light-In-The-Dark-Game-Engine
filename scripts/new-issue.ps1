# new-issue.ps1 - robust GitHub issue creation from a body FILE.
# Root cause (2026-07-12): inline multi-KB bodies passed through shell tooling truncate
# (~8KB) and die with cryptic quote errors; gh 504s can create-then-error. This helper
# makes both failure modes impossible to miss: body always via --body-file, hard errors,
# post-create existence verification. ASCII only.
param(
    [Parameter(Mandatory = $true)][string]$Title,
    [Parameter(Mandatory = $true)][string]$BodyPath
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $BodyPath)) {
    throw "new-issue: body file not found: $BodyPath"
}
$body = Get-Content -LiteralPath $BodyPath -Raw
if ([string]::IsNullOrWhiteSpace($body)) {
    throw "new-issue: body file is empty: $BodyPath"
}
if ($Title.Length -gt 256) {
    throw "new-issue: title exceeds GitHub 256-char limit ($($Title.Length))"
}
$url = gh issue create --title $Title --body-file $BodyPath 2>&1
if ($LASTEXITCODE -ne 0) {
    # gh may have created the issue before a gateway error; check before declaring failure.
    $probe = gh issue list --state open --search ("in:title " + '"' + $Title + '"') --json number,title --limit 5 | ConvertFrom-Json
    if ($probe -and ($probe | Where-Object { $_.title -eq $Title })) {
        $n = ($probe | Where-Object { $_.title -eq $Title } | Select-Object -First 1).number  # fsv-idioms:allow (in-memory JSON array; nearby $LASTEXITCODE is the gh-create exit, unrelated to this Select)
        Write-Error "new-issue: gh exited nonzero BUT issue #$n exists with this exact title. Verify #$n manually; do NOT blind-retry." -ErrorAction Stop
    }
    throw "new-issue: gh issue create failed (exit $LASTEXITCODE): $url"
}
# Full-state verification: confirm the issue actually exists and its body round-trips.
if ($url -notmatch '/issues/(\d+)$') {
    throw "new-issue: gh returned no issue URL: $url"
}
$num = $Matches[1]
$readback = gh issue view $num --json title,body | ConvertFrom-Json
if ($readback.title -ne $Title) {
    throw "new-issue: readback title mismatch on #${num}: got '$($readback.title)'"
}
if ($readback.body.Trim().Length -lt [Math]::Min(200, $body.Trim().Length)) {
    throw "new-issue: readback body suspiciously short on #${num} (possible truncation)"
}
Write-Output "created #$num $url (body $($body.Length) chars, readback verified)"
