param(
    [string]$ProjectId = "c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5",

    [string]$Environment = "prod",

    [string]$SecretPath = "/gameeditor",

    [string]$SecretName = "GAMEEDITOR_DEEPSEEK_API_KEY",

    [string]$Model = "deepseek-v4-flash",

    [string]$ExpectedContent = "GAMEEDITOR_DEEPSEEK_OK",

    [string]$FsvRoot,

    [string]$InfisicalExecutable = "infisical"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "infisical-path-guard.ps1")

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$reportRoot = if ($FsvRoot) {
    if ([System.IO.Path]::IsPathRooted($FsvRoot)) { $FsvRoot } else { Join-Path $repoRoot $FsvRoot }
} else {
    Join-Path $repoRoot "target\fsv\deepseek-api-verification"
}
New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$reportPath = Join-Path $reportRoot ("deepseek-api-verify-{0}.json" -f $timestamp)
$startedAtUtc = [DateTimeOffset]::UtcNow
$authRead = $null

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

function Get-Sha256FileHex {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
        } finally {
            $stream.Dispose()
        }
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
        Stop-WithCode -Code "DEEPSEEK_VERIFY_PROCESS_START_FAILED" -Message "Failed to start $Command."
    }

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $exitCode = $process.ExitCode
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
        exit_code = $exitCode
        lines = @($lines)
    }
}

function Get-OutputTail {
    param(
        [string[]]$Lines,

        [int]$Count = 30
    )

    return @($Lines | Select-Object -Last $Count | ForEach-Object {
        ([string]$_) `
            -replace "sk-[A-Za-z0-9]{20,}", "<redacted-secret-like-token>" `
            -replace "api key:\s*\*+[A-Za-z0-9_-]+", "api key: <redacted-key-fingerprint>"
    })
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
        Stop-WithCode -Code "DEEPSEEK_VERIFY_INVALID_PROJECT_ID" -Message "ProjectId must be non-empty."
    }
    if ([string]::IsNullOrWhiteSpace($Environment)) {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_INVALID_ENVIRONMENT" -Message "Environment must be non-empty."
    }
    if (Test-InfisicalPathMangled -SecretPath $SecretPath) {
        Stop-WithCode -Code "INFISICAL_PATH_MANGLED" -Message (Get-InfisicalPathMangledMessage -SecretPath $SecretPath)
    }
    if ([string]::IsNullOrWhiteSpace($SecretPath) -or -not $SecretPath.StartsWith("/")) {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_INVALID_PATH" -Message "SecretPath must be an absolute Infisical path."
    }
    if ($SecretName -notmatch "^[A-Za-z_][A-Za-z0-9_]*$") {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_INVALID_SECRET_NAME" -Message "SecretName must be a valid environment variable name."
    }
    if ($Model -notin @("deepseek-v4-flash", "deepseek-v4-pro")) {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_INVALID_MODEL" -Message "Use a current DeepSeek V4 model id."
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
            error_code = "DEEPSEEK_VERIFY_INFISICAL_AUTH_REQUIRED"
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

function New-ChildScript {
    return @'
$ErrorActionPreference = "Stop"
$startedAtUtc = [DateTimeOffset]::UtcNow
$secretName = $env:GAMEEDITOR_DEEPSEEK_VERIFY_SECRET_NAME
$model = $env:GAMEEDITOR_DEEPSEEK_VERIFY_MODEL
$expected = $env:GAMEEDITOR_DEEPSEEK_VERIFY_EXPECTED
$endpoint = "https://api.deepseek.com/chat/completions"
$status = "failed"
$errorCode = $null
$errorMessage = $null
$httpStatus = $null
$responseText = $null
$responseJson = $null
$content = $null
$requestId = $null
$responseId = $null
$responseModel = $null
$usageTotalTokens = $null
$key = $null

function Protect-Text {
    param(
        [AllowNull()]
        [string]$Text
    )
    if ($null -eq $Text) {
        return $null
    }
    $safe = [string]$Text
    if (-not [string]::IsNullOrEmpty($script:key)) {
        $safe = $safe.Replace($script:key, "<redacted-secret-value>")
    }
    return $safe `
        -replace "sk-[A-Za-z0-9]{20,}", "<redacted-secret-like-token>" `
        -replace "api key:\s*\*+[A-Za-z0-9_-]+", "api key: <redacted-key-fingerprint>"
}

function Set-Failure {
    param(
        [string]$Code,
        [string]$Message
    )
    $script:errorCode = $Code
    throw "$($Code): $Message"
}

try {
    if ([string]::IsNullOrWhiteSpace($secretName)) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SECRET_NAME_MISSING" -Message "Secret environment variable name was not provided."
    }
    $key = [Environment]::GetEnvironmentVariable($secretName)
    if ([string]::IsNullOrEmpty($key)) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SECRET_MISSING" -Message "Infisical did not inject the DeepSeek API key."
    }
    try {
        Add-Type -AssemblyName System.Net.Http
    } catch {
        Set-Failure -Code "DEEPSEEK_VERIFY_HTTP_CLIENT_UNAVAILABLE" -Message "System.Net.Http could not be loaded."
    }

    $body = [ordered]@{
        model = $model
        messages = @(
            [ordered]@{
                role = "system"
                content = "Return the user's requested sentinel exactly. Do not add other text."
            },
            [ordered]@{
                role = "user"
                content = "Return exactly: $expected"
            }
        )
        thinking = [ordered]@{ type = "disabled" }
        stream = $false
        temperature = 0
        max_tokens = 16
    } | ConvertTo-Json -Depth 8

    $client = [System.Net.Http.HttpClient]::new()
    try {
        $client.Timeout = [TimeSpan]::FromSeconds(60)
        $client.DefaultRequestHeaders.Authorization = [System.Net.Http.Headers.AuthenticationHeaderValue]::new("Bearer", $key)
        $requestContent = [System.Net.Http.StringContent]::new($body, [System.Text.Encoding]::UTF8, "application/json")
        $response = $client.PostAsync($endpoint, $requestContent).GetAwaiter().GetResult()
        $httpStatus = [int]$response.StatusCode
        if ($response.Headers.Contains("x-request-id")) {
            $requestId = ($response.Headers.GetValues("x-request-id") | Select-Object -First 1)
        }
        $responseText = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    } finally {
        if ($null -ne $client) {
            $client.Dispose()
        }
    }

    if ($httpStatus -lt 200 -or $httpStatus -gt 299) {
        Set-Failure -Code "DEEPSEEK_VERIFY_HTTP_FAILED" -Message "DeepSeek returned HTTP $httpStatus."
    }
    if ([string]::IsNullOrWhiteSpace($responseText)) {
        Set-Failure -Code "DEEPSEEK_VERIFY_EMPTY_RESPONSE" -Message "DeepSeek returned an empty response body."
    }

    try {
        $responseJson = $responseText | ConvertFrom-Json
    } catch {
        Set-Failure -Code "DEEPSEEK_VERIFY_JSON_PARSE_FAILED" -Message "DeepSeek returned non-JSON content."
    }

    $responseId = [string]$responseJson.id
    $responseModel = [string]$responseJson.model
    $choices = @($responseJson.choices)
    if ($choices.Count -lt 1) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SCHEMA_MISSING_CHOICES" -Message "DeepSeek response did not include choices[0]."
    }
    $content = [string]$choices[0].message.content
    if ($content -ne $expected) {
        Set-Failure -Code "DEEPSEEK_VERIFY_UNEXPECTED_CONTENT" -Message "DeepSeek response content did not match the sentinel."
    }
    if ([string]::IsNullOrWhiteSpace($responseId)) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SCHEMA_MISSING_ID" -Message "DeepSeek response did not include an id."
    }
    if ([string]::IsNullOrWhiteSpace($responseModel)) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SCHEMA_MISSING_MODEL" -Message "DeepSeek response did not include a model."
    }
    if ($null -ne $responseJson.usage -and $null -ne $responseJson.usage.total_tokens) {
        $usageTotalTokens = [int]$responseJson.usage.total_tokens
    }
    if ($null -eq $usageTotalTokens -or $usageTotalTokens -le 0) {
        Set-Failure -Code "DEEPSEEK_VERIFY_SCHEMA_MISSING_USAGE" -Message "DeepSeek response did not include positive token usage."
    }

    $status = "passed"
} catch {
    if ([string]::IsNullOrWhiteSpace($errorCode)) {
        $errorCode = "DEEPSEEK_VERIFY_UNHANDLED"
    }
    $errorMessage = Protect-Text -Text ([string]$_.Exception.Message)
}

$report = [ordered]@{
    schema_version = 1
    status = $status
    error_code = $errorCode
    error_message = $errorMessage
    provider = "deepseek"
    endpoint = $endpoint
    model_requested = $model
    http_status = $httpStatus
    request_id = $requestId
    response_id = $responseId
    response_model = $responseModel
    expected_content = $expected
    actual_content = $content
    usage_total_tokens = $usageTotalTokens
    response_body_excerpt = if ($responseText) { Protect-Text -Text $responseText.Substring(0, [Math]::Min(512, $responseText.Length)) } else { $null }
    secret_name = $secretName
    raw_secret_printed = $false
    started_at_utc = $startedAtUtc.ToString("O")
    ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
}

$json = $report | ConvertTo-Json -Depth 8 -Compress
if (-not [string]::IsNullOrEmpty($key) -and $json.Contains($key)) {
    [ordered]@{
        schema_version = 1
        status = "failed"
        error_code = "DEEPSEEK_VERIFY_REPORT_LEAK_GUARD"
        error_message = "Sanitized child report attempted to include the raw secret value."
        provider = "deepseek"
        endpoint = $endpoint
        model_requested = $model
        secret_name = $secretName
        raw_secret_printed = $false
        started_at_utc = $startedAtUtc.ToString("O")
        ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
    } | ConvertTo-Json -Depth 8 -Compress
    exit 41
}

$json
if ($status -ne "passed") {
    exit 1
}
'@
}

$status = "failed"
$errorCode = $null
$errorMessage = $null
$cliVersion = $null
$childReport = $null
$childOutputTail = @()
$reportSha256 = $null
$exitCode = 1

try {
    Assert-Inputs
    $versionResult = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments @("--version")
    if ($versionResult.exit_code -ne 0) {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_CLI_UNAVAILABLE" -Message "Infisical CLI version check failed."
    }
    $cliVersion = (($versionResult.lines | Select-Object -First 1) -join "").Trim()

    $authRead = Test-InfisicalAuthentication
    if ($authRead.status -ne "passed") {
        $authCode = if ($authRead.error_code) { [string]$authRead.error_code } else { "DEEPSEEK_VERIFY_INFISICAL_AUTH_REQUIRED" }
        Stop-WithCode -Code $authCode -Message "Infisical CLI is not authenticated. Run 'infisical login' in an interactive terminal or provide INFISICAL_TOKEN; provider verification is manual-only and must not attempt browser login."
    }

    $previousSecretName = $env:GAMEEDITOR_DEEPSEEK_VERIFY_SECRET_NAME
    $previousModel = $env:GAMEEDITOR_DEEPSEEK_VERIFY_MODEL
    $previousExpected = $env:GAMEEDITOR_DEEPSEEK_VERIFY_EXPECTED
    $previousDisableUpdate = $env:INFISICAL_DISABLE_UPDATE_CHECK
    $env:GAMEEDITOR_DEEPSEEK_VERIFY_SECRET_NAME = $SecretName
    $env:GAMEEDITOR_DEEPSEEK_VERIFY_MODEL = $Model
    $env:GAMEEDITOR_DEEPSEEK_VERIFY_EXPECTED = $ExpectedContent
    $env:INFISICAL_DISABLE_UPDATE_CHECK = "true"
    try {
        $encoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes((New-ChildScript)))
        $result = Invoke-NativeCapture -Command $InfisicalExecutable -Arguments @(
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
    } finally {
        if ($null -eq $previousSecretName) { Remove-Item Env:\GAMEEDITOR_DEEPSEEK_VERIFY_SECRET_NAME -ErrorAction SilentlyContinue } else { $env:GAMEEDITOR_DEEPSEEK_VERIFY_SECRET_NAME = $previousSecretName }
        if ($null -eq $previousModel) { Remove-Item Env:\GAMEEDITOR_DEEPSEEK_VERIFY_MODEL -ErrorAction SilentlyContinue } else { $env:GAMEEDITOR_DEEPSEEK_VERIFY_MODEL = $previousModel }
        if ($null -eq $previousExpected) { Remove-Item Env:\GAMEEDITOR_DEEPSEEK_VERIFY_EXPECTED -ErrorAction SilentlyContinue } else { $env:GAMEEDITOR_DEEPSEEK_VERIFY_EXPECTED = $previousExpected }
        if ($null -eq $previousDisableUpdate) { Remove-Item Env:\INFISICAL_DISABLE_UPDATE_CHECK -ErrorAction SilentlyContinue } else { $env:INFISICAL_DISABLE_UPDATE_CHECK = $previousDisableUpdate }
    }

    $childOutputTail = Get-OutputTail -Lines $result.lines
    $jsonLine = @($result.lines | Where-Object { $_.TrimStart().StartsWith("{") } | Select-Object -Last 1)
    if ($jsonLine.Count -eq 0) {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_CHILD_JSON_MISSING" -Message "DeepSeek verification child did not return JSON."
    }

    try {
        $childReport = ([string]$jsonLine[0]) | ConvertFrom-Json
    } catch {
        Stop-WithCode -Code "DEEPSEEK_VERIFY_CHILD_JSON_PARSE_FAILED" -Message "DeepSeek verification child returned invalid JSON."
    }
    if ($result.exit_code -ne 0 -or $childReport.status -ne "passed") {
        $childCode = if ($childReport.error_code) { [string]$childReport.error_code } else { "DEEPSEEK_VERIFY_CHILD_FAILED" }
        Stop-WithCode -Code $childCode -Message "DeepSeek verification child failed."
    }

    $status = "passed"
    $exitCode = 0
} catch {
    if ([string]::IsNullOrWhiteSpace($errorCode)) {
        if ($_.Exception.Message -match "^(DEEPSEEK_[A-Z0-9_]+):") {
            $errorCode = $Matches[1]
        } else {
            $errorCode = "DEEPSEEK_VERIFY_UNHANDLED"
        }
    }
    $errorMessage = ([string]$_.Exception.Message) `
        -replace "sk-[A-Za-z0-9]{20,}", "<redacted-secret-like-token>" `
        -replace "api key:\s*\*+[A-Za-z0-9_-]+", "api key: <redacted-key-fingerprint>"
    $status = "failed"
    $exitCode = 1
} finally {
    $report = [ordered]@{
        schema_version = 1
        status = $status
        error_code = $errorCode
        error_message = $errorMessage
        cli_version = $cliVersion
        project_id = $ProjectId
        environment = $Environment
        secret_path = $SecretPath
        secret_name = $SecretName
        auth_read = $authRead
        provider = "deepseek"
        model = $Model
        expected_content = $ExpectedContent
        child_report = $childReport
        child_output_tail = $childOutputTail
        report_path = $reportPath
        started_at_utc = $startedAtUtc.ToString("O")
        ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
    }

    $reportJson = $report | ConvertTo-Json -Depth 10
    $reportJson | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $reportSha256 = Get-Sha256FileHex -Path $reportPath
    $readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    $readbackChild = $readback.child_report
    $readbackActual = [string](Get-ObjectPropertyValue -Object $readbackChild -Name "actual_content" -Default "")
    $readbackHttpStatus = Get-ObjectPropertyValue -Object $readbackChild -Name "http_status" -Default ""
    $readbackStatus = [string]$readback.status
    if ($readbackStatus -ne $status -or ($status -eq "passed" -and $readbackActual -ne $ExpectedContent)) {
        $exitCode = 1
        $status = "failed"
        Write-Error "DEEPSEEK_VERIFY_REPORT_READBACK_MISMATCH: persisted verification report did not match expected status/content."
    }
    Write-Host "DEEPSEEK_API_VERIFY_REPORT_READBACK path=$reportPath status=$($readback.status) error_code=$($readback.error_code) model=$Model http_status=$readbackHttpStatus actual_content=$readbackActual sha256=$reportSha256"
}

exit $exitCode
