param(
    [string]$ProjectId = "c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5",

    [string]$Environment = "prod",

    [string]$SecretPath = "/gameeditor",

    [string]$SecretName = "GAMEEDITOR_DEEPSEEK_API_KEY",

    [string]$FsvRoot,

    [string]$InfisicalExecutable = "infisical"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "infisical-path-guard.ps1")

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$reportRoot = if ($FsvRoot) {
    if (-not [System.IO.Path]::IsPathRooted($FsvRoot)) {
        Join-Path $repoRoot $FsvRoot
    } else {
        $FsvRoot
    }
} else {
    Join-Path $repoRoot "target\fsv\infisical-secret-verification"
}
New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$reportPath = Join-Path $reportRoot ("infisical-secret-verify-{0}-{1}.json" -f ($SecretName -replace "[^A-Za-z0-9_.-]", "_"), $timestamp)
$startedAtUtc = [DateTimeOffset]::UtcNow
$status = "failed"
$errorCode = $null
$errorMessage = $null
$exitCode = 1
$secretValue = $null
$directRead = $null
$injectionRead = $null
$authRead = $null
$cliVersion = $null

$env:INFISICAL_DISABLE_UPDATE_CHECK = "true"

function Stop-WithCode {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Code,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    $script:errorCode = $Code
    throw "$($Code): $Message"
}

function Protect-Text {
    param(
        [AllowNull()]
        [string]$Text
    )

    if ($null -eq $Text) {
        return $null
    }
    $safe = [string]$Text
    if (-not [string]::IsNullOrEmpty($script:secretValue)) {
        $safe = $safe.Replace($script:secretValue, "<redacted-secret-value>")
    }
    return ($safe -replace "sk-[A-Za-z0-9]{20,}", "<redacted-secret-like-token>")
}

function Get-Sha256Hex {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function ConvertTo-CommandLineArgument {
    param(
        [AllowNull()]
        [string]$Value
    )

    if ($null -eq $Value) {
        return '""'
    }
    if ($Value -notmatch '[\s"]') {
        return $Value
    }
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Command,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Command
    $startInfo.Arguments = (($Arguments | ForEach-Object { ConvertTo-CommandLineArgument -Value $_ }) -join " ")
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        Stop-WithCode -Code "INFISICAL_VERIFY_PROCESS_START_FAILED" -Message "Failed to start $Command."
    }
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $nativeExitCode = $process.ExitCode
    $process.Dispose()

    $lines = New-Object System.Collections.ArrayList
    foreach ($line in @($stdout -split "\r?\n")) {
        if ($line.Length -gt 0) {
            [void]$lines.Add($line)
        }
    }
    foreach ($line in @($stderr -split "\r?\n")) {
        if ($line.Length -gt 0) {
            [void]$lines.Add($line)
        }
    }
    return [pscustomobject]@{
        exit_code = $nativeExitCode
        lines = @($lines)
    }
}

function Get-OutputTail {
    param(
        [string[]]$Lines,

        [int]$Count = 20
    )

    return @($Lines | Select-Object -Last $Count | ForEach-Object { Protect-Text -Text $_ })
}

function Get-ObjectPropertyValue {
    param(
        [AllowNull()]
        [object]$Object,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [AllowNull()]
        [object]$Default
    )

    if ($null -eq $Object) {
        return $Default
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $Default
    }
    return $property.Value
}

function Assert-Inputs {
    if ([string]::IsNullOrWhiteSpace($ProjectId)) {
        Stop-WithCode -Code "INFISICAL_VERIFY_INVALID_PROJECT_ID" -Message "ProjectId must be non-empty."
    }
    if ([string]::IsNullOrWhiteSpace($Environment)) {
        Stop-WithCode -Code "INFISICAL_VERIFY_INVALID_ENVIRONMENT" -Message "Environment must be non-empty."
    }
    if (Test-InfisicalPathMangled -SecretPath $SecretPath) {
        Stop-WithCode -Code "INFISICAL_PATH_MANGLED" -Message (Get-InfisicalPathMangledMessage -SecretPath $SecretPath)
    }
    if ([string]::IsNullOrWhiteSpace($SecretPath) -or -not $SecretPath.StartsWith("/")) {
        Stop-WithCode -Code "INFISICAL_VERIFY_INVALID_PATH" -Message "SecretPath must be an absolute Infisical path."
    }
    if ($SecretName -notmatch "^[A-Za-z_][A-Za-z0-9_]*$") {
        Stop-WithCode -Code "INFISICAL_VERIFY_INVALID_SECRET_NAME" -Message "SecretName must be a valid environment variable name."
    }
}

function Test-InfisicalAuthentication {
    if (-not [string]::IsNullOrWhiteSpace($env:INFISICAL_TOKEN)) {
        return [pscustomobject]@{
            status = "passed"
            auth_source = "INFISICAL_TOKEN"
            exit_code = 0
            output_tail = @()
        }
    }

    $result = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments @("login", "status", "--silent")
    if ($result.exit_code -ne 0) {
        return [pscustomobject]@{
            status = "failed"
            error_code = "INFISICAL_VERIFY_AUTH_REQUIRED"
            exit_code = $result.exit_code
            auth_source = "none"
            output_tail = Get-OutputTail -Lines $result.lines
        }
    }

    return [pscustomobject]@{
        status = "passed"
        auth_source = "cli_keyring"
        exit_code = $result.exit_code
        output_tail = Get-OutputTail -Lines $result.lines
    }
}

function Read-InfisicalSecret {
    $args = @(
        "secrets",
        "get",
        $SecretName,
        "--projectId=$ProjectId",
        "--env=$Environment",
        "--path=$SecretPath",
        "--output=json",
        "--silent"
    )
    $result = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments $args
    if ($result.exit_code -ne 0) {
        return [pscustomobject]@{
            status = "failed"
            error_code = "INFISICAL_VERIFY_GET_FAILED"
            exit_code = $result.exit_code
            present = $false
            output_tail = Get-OutputTail -Lines $result.lines
        }
    }

    $jsonText = ($result.lines -join "`n").Trim()
    if ([string]::IsNullOrWhiteSpace($jsonText)) {
        Stop-WithCode -Code "INFISICAL_VERIFY_EMPTY_GET_OUTPUT" -Message "Infisical returned no JSON for the direct secret read."
    }

    try {
        $json = $jsonText | ConvertFrom-Json
    } catch {
        Stop-WithCode -Code "INFISICAL_VERIFY_GET_JSON_PARSE_FAILED" -Message "Infisical direct read returned non-parseable JSON."
    }

    $items = @($json)
    $item = @($items | Where-Object { $_.secretKey -eq $SecretName } | Select-Object -First 1)
    if ($item.Count -eq 0) {
        return [pscustomobject]@{
            status = "failed"
            error_code = "INFISICAL_VERIFY_SECRET_MISSING"
            exit_code = $result.exit_code
            secret_name = $SecretName
            secret_path = $SecretPath
            present = $false
            value_length = 0
            sha256 = $null
            sha256_12 = $null
            secret_value_printed = $false
        }
    }

    $value = [string]$item[0].secretValue
    if ([string]::IsNullOrEmpty($value)) {
        return [pscustomobject]@{
            status = "failed"
            error_code = "INFISICAL_VERIFY_SECRET_EMPTY"
            exit_code = $result.exit_code
            secret_name = [string]$item[0].secretKey
            secret_path = $SecretPath
            present = $true
            value_length = 0
            sha256 = $null
            sha256_12 = $null
            secret_value_printed = $false
        }
    }

    $script:secretValue = $value
    $hash = Get-Sha256Hex -Value $value
    return [pscustomobject]@{
        status = "passed"
        exit_code = $result.exit_code
        secret_name = [string]$item[0].secretKey
        secret_path = $SecretPath
        present = $true
        value_length = $value.Length
        sha256 = $hash
        sha256_12 = $hash.Substring(0, 12)
        secret_value_printed = $false
    }
}

function Read-InfisicalInjection {
    $childScript = @'
$ErrorActionPreference = "Stop"
$name = $env:INFISICAL_VERIFY_SECRET_NAME
if ([string]::IsNullOrWhiteSpace($name)) {
    [pscustomobject]@{ secret_name = $null; present = $false; length = 0; sha256 = $null; value_printed = $false; error_code = "INFISICAL_VERIFY_CHILD_SECRET_NAME_MISSING" } | ConvertTo-Json -Compress
    exit 12
}
$value = [Environment]::GetEnvironmentVariable($name)
if ([string]::IsNullOrEmpty($value)) {
    [pscustomobject]@{ secret_name = $name; present = $false; length = 0; sha256 = $null; value_printed = $false; error_code = "INFISICAL_VERIFY_INJECTION_SECRET_MISSING" } | ConvertTo-Json -Compress
    exit 11
}
$sha = [System.Security.Cryptography.SHA256]::Create()
try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($value)
    $hash = ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
} finally {
    $sha.Dispose()
}
[pscustomobject]@{ secret_name = $name; present = $true; length = $value.Length; sha256 = $hash; value_printed = $false; error_code = $null } | ConvertTo-Json -Compress
'@

    $previousName = $env:INFISICAL_VERIFY_SECRET_NAME
    $env:INFISICAL_VERIFY_SECRET_NAME = $SecretName
    try {
        $encoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($childScript))
        $args = @(
            "run",
            "--projectId=$ProjectId",
            "--env=$Environment",
            "--path=$SecretPath",
            "--silent",
            "--",
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            $encoded
        )
        $result = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments $args
    } finally {
        if ($null -eq $previousName) {
            Remove-Item Env:\INFISICAL_VERIFY_SECRET_NAME -ErrorAction SilentlyContinue
        } else {
            $env:INFISICAL_VERIFY_SECRET_NAME = $previousName
        }
    }

    $combinedOutput = ($result.lines -join "`n")
    if (-not [string]::IsNullOrEmpty($script:secretValue) -and $combinedOutput.Contains($script:secretValue)) {
        Stop-WithCode -Code "INFISICAL_VERIFY_INJECTION_LEAK_GUARD" -Message "Injection check output contained the raw secret value."
    }

    $jsonLine = @($result.lines | Where-Object { $_.TrimStart().StartsWith("{") } | Select-Object -Last 1)
    if ($jsonLine.Count -eq 0) {
        return [pscustomobject]@{
            status = "failed"
            exit_code = $result.exit_code
            present = $false
            output_tail = Get-OutputTail -Lines $result.lines
        }
    }

    try {
        $json = ([string]$jsonLine[0]) | ConvertFrom-Json
    } catch {
        Stop-WithCode -Code "INFISICAL_VERIFY_INJECTION_JSON_PARSE_FAILED" -Message "Injection child returned non-parseable JSON."
    }

    return [pscustomobject]@{
        status = if ($result.exit_code -eq 0 -and [bool]$json.present) { "passed" } else { "failed" }
        exit_code = $result.exit_code
        secret_name = [string]$json.secret_name
        present = [bool]$json.present
        value_length = [int]$json.length
        sha256 = $json.sha256
        sha256_12 = if ($json.sha256) { ([string]$json.sha256).Substring(0, 12) } else { $null }
        matches_direct_read = ($json.sha256 -eq $script:directRead.sha256)
        value_printed = [bool]$json.value_printed
        error_code = $json.error_code
        output_tail = Get-OutputTail -Lines $result.lines
    }
}

try {
    Assert-Inputs
    $versionResult = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments @("--version")
    if ($versionResult.exit_code -ne 0) {
        Stop-WithCode -Code "INFISICAL_VERIFY_CLI_UNAVAILABLE" -Message "Infisical CLI version check failed."
    }
    $cliVersion = (($versionResult.lines | Select-Object -First 1) -join "").Trim()

    $authRead = Test-InfisicalAuthentication
    if ($authRead.status -ne "passed") {
        $authCode = if ($authRead.error_code) { [string]$authRead.error_code } else { "INFISICAL_VERIFY_AUTH_REQUIRED" }
        Stop-WithCode -Code $authCode -Message "Infisical CLI is not authenticated. Run 'infisical login' in an interactive terminal or provide INFISICAL_TOKEN; do not rotate or test DeepSeek until this preflight passes."
    }

    $directRead = Read-InfisicalSecret
    if ($directRead.status -ne "passed") {
        $directCode = if ($directRead.error_code) { [string]$directRead.error_code } else { "INFISICAL_VERIFY_GET_FAILED" }
        Stop-WithCode -Code $directCode -Message "Direct Infisical secret read failed."
    }

    $injectionRead = Read-InfisicalInjection
    if ($injectionRead.status -ne "passed") {
        $childCode = if ($injectionRead.error_code) { [string]$injectionRead.error_code } else { "INFISICAL_VERIFY_RUN_FAILED" }
        Stop-WithCode -Code $childCode -Message "Infisical run injection verification failed."
    }
    if (-not $injectionRead.matches_direct_read) {
        Stop-WithCode -Code "INFISICAL_VERIFY_INJECTION_MISMATCH" -Message "Injected value hash did not match direct Infisical read hash."
    }

    $status = "passed"
    $exitCode = 0
} catch {
    if ([string]::IsNullOrWhiteSpace($errorCode)) {
        $errorCode = "INFISICAL_VERIFY_UNHANDLED"
    }
    $errorMessage = Protect-Text -Text $_.Exception.Message
    $status = "failed"
    $exitCode = 1
} finally {
    $report = [ordered]@{
        schema_version = 1
        status = $status
        verified_present = ($status -eq "passed")
        error_code = $errorCode
        error_message = $errorMessage
        cli_version = $cliVersion
        project_id = $ProjectId
        environment = $Environment
        secret_path = $SecretPath
        secret_name = $SecretName
        auth_read = $authRead
        direct_read = $directRead
        injection_read = $injectionRead
        report_path = $reportPath
        started_at_utc = $startedAtUtc.ToString("O")
        ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
    }
    $reportJson = $report | ConvertTo-Json -Depth 8
    if (-not [string]::IsNullOrEmpty($secretValue) -and $reportJson.Contains($secretValue)) {
        $report = [ordered]@{
            schema_version = 1
            status = "failed"
            verified_present = $false
            error_code = "INFISICAL_VERIFY_REPORT_LEAK_GUARD"
            error_message = "Sanitized report construction attempted to include the raw secret value."
            cli_version = $cliVersion
            project_id = $ProjectId
            environment = $Environment
            secret_path = $SecretPath
            secret_name = $SecretName
            report_path = $reportPath
            started_at_utc = $startedAtUtc.ToString("O")
            ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
        }
        $reportJson = $report | ConvertTo-Json -Depth 8
        $exitCode = 1
    }

    $reportJson | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    $directNode = $readback.direct_read
    $injectionNode = $readback.injection_read
    $directPresent = Get-ObjectPropertyValue -Object $directNode -Name "present" -Default $false
    $directLength = Get-ObjectPropertyValue -Object $directNode -Name "value_length" -Default ""
    $directSha = Get-ObjectPropertyValue -Object $directNode -Name "sha256_12" -Default ""
    $injectionMatches = Get-ObjectPropertyValue -Object $injectionNode -Name "matches_direct_read" -Default $false
    $verifiedPresent = Get-ObjectPropertyValue -Object $readback -Name "verified_present" -Default ($readback.status -eq "passed")
    Write-Host "INFISICAL_SECRET_VERIFY_REPORT_READBACK path=$reportPath status=$($readback.status) error_code=$($readback.error_code) verified_present=$verifiedPresent direct_get_present=$directPresent length=$directLength sha256_12=$directSha injection_matches_direct_read=$injectionMatches"
}

if ($exitCode -ne 0) {
    Write-Error $errorMessage
}
exit $exitCode
