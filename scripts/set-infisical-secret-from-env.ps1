param(
    [string]$ProjectId = "c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5",

    [string]$Environment = "prod",

    [string]$SecretPath = "/gameeditor",

    [string]$SecretName = "GAMEEDITOR_DEEPSEEK_API_KEY",

    [string]$ValueEnvVar = "GAMEEDITOR_DEEPSEEK_ROTATION_KEY",

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
    Join-Path $repoRoot "target\fsv\infisical-secret-set"
}
New-Item -ItemType Directory -Force -Path $reportRoot | Out-Null

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
$safeSecretName = $SecretName -replace "[^A-Za-z0-9_.-]", "_"
$reportPath = Join-Path $reportRoot ("infisical-secret-set-{0}-{1}.json" -f $safeSecretName, $timestamp)
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
        Stop-WithCode -Code "INFISICAL_SET_PROCESS_START_FAILED" -Message "Failed to start $Command."
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

function Assert-Inputs {
    if ([string]::IsNullOrWhiteSpace($ProjectId)) {
        Stop-WithCode -Code "INFISICAL_SET_INVALID_PROJECT_ID" -Message "ProjectId must be non-empty."
    }
    if ([string]::IsNullOrWhiteSpace($Environment)) {
        Stop-WithCode -Code "INFISICAL_SET_INVALID_ENVIRONMENT" -Message "Environment must be non-empty."
    }
    if (Test-InfisicalPathMangled -SecretPath $SecretPath) {
        Stop-WithCode -Code "INFISICAL_PATH_MANGLED" -Message (Get-InfisicalPathMangledMessage -SecretPath $SecretPath)
    }
    if ([string]::IsNullOrWhiteSpace($SecretPath) -or -not $SecretPath.StartsWith("/")) {
        Stop-WithCode -Code "INFISICAL_SET_INVALID_PATH" -Message "SecretPath must be an absolute Infisical path."
    }
    if ($SecretName -notmatch "^[A-Za-z_][A-Za-z0-9_]*$") {
        Stop-WithCode -Code "INFISICAL_SET_INVALID_SECRET_NAME" -Message "SecretName must be a valid environment variable name."
    }
    if ($ValueEnvVar -notmatch "^[A-Za-z_][A-Za-z0-9_]*$") {
        Stop-WithCode -Code "INFISICAL_SET_INVALID_VALUE_ENV_VAR" -Message "ValueEnvVar must be a valid environment variable name."
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
            error_code = "INFISICAL_SET_AUTH_REQUIRED"
            exit_code = $result.exit_code
            auth_source = "none"
            output_tail = @($result.lines | Select-Object -Last 20 | ForEach-Object { Protect-Text -Text $_ })
        }
    }

    return [pscustomobject]@{
        status = "passed"
        auth_source = "cli_keyring"
        exit_code = $result.exit_code
        output_tail = @($result.lines | Select-Object -Last 20 | ForEach-Object { Protect-Text -Text $_ })
    }
}

$status = "failed"
$errorCode = $null
$errorMessage = $null
$exitCode = 1
$cliExitCode = $null
$cliVersion = $null
$secretValue = $null
$secretSha256 = $null
$rawKeyInCommandLine = $false
$cliOutputContainedRawSecret = $false
$stdoutTail = @()
$stderrTail = @()
$pipePath = $null

try {
    Assert-Inputs
    $secretValue = [Environment]::GetEnvironmentVariable($ValueEnvVar)
    if ([string]::IsNullOrEmpty($secretValue)) {
        Stop-WithCode -Code "INFISICAL_SET_VALUE_MISSING" -Message "Value environment variable is missing or empty."
    }
    $secretSha256 = Get-Sha256Hex -Value $secretValue

    $versionInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $versionInfo.FileName = $InfisicalExecutable
    $versionInfo.Arguments = "--version"
    $versionInfo.WorkingDirectory = $repoRoot
    $versionInfo.UseShellExecute = $false
    $versionInfo.RedirectStandardOutput = $true
    $versionInfo.RedirectStandardError = $true
    $versionInfo.CreateNoWindow = $true
    $versionProcess = [System.Diagnostics.Process]::Start($versionInfo)
    $versionStdout = $versionProcess.StandardOutput.ReadToEnd()
    $versionStderr = $versionProcess.StandardError.ReadToEnd()
    $versionProcess.WaitForExit()
    if ($versionProcess.ExitCode -ne 0) {
        Stop-WithCode -Code "INFISICAL_SET_CLI_UNAVAILABLE" -Message "Infisical CLI version check failed."
    }
    $cliVersion = (($versionStdout, $versionStderr -join "`n") -split "\r?\n" | Select-Object -First 1).Trim()
    $versionProcess.Dispose()

    $authRead = Test-InfisicalAuthentication
    if ($authRead.status -ne "passed") {
        $authCode = if ($authRead.error_code) { [string]$authRead.error_code } else { "INFISICAL_SET_AUTH_REQUIRED" }
        Stop-WithCode -Code $authCode -Message "Infisical CLI is not authenticated. Run 'infisical login' in an interactive terminal or provide INFISICAL_TOKEN; the setter will not trigger browser login."
    }

    $pipeName = "gameeditor_secret_set_" + ([Guid]::NewGuid().ToString("N"))
    $pipePath = "\\.\pipe\$pipeName"
    $secretArgument = "$SecretName=@$pipePath"
    $args = @(
        "secrets",
        "set",
        $secretArgument,
        "--projectId=$ProjectId",
        "--env=$Environment",
        "--path=$SecretPath",
        "--silent"
    )
    $commandLine = (($args | ForEach-Object { ConvertTo-CommandLineArgument -Value $_ }) -join " ")
    $rawKeyInCommandLine = $commandLine.Contains($secretValue)

    $pipe = [System.IO.Pipes.NamedPipeServerStream]::new(
        $pipeName,
        [System.IO.Pipes.PipeDirection]::Out,
        1,
        [System.IO.Pipes.PipeTransmissionMode]::Byte,
        [System.IO.Pipes.PipeOptions]::Asynchronous
    )
    $process = $null
    $stdout = ""
    $stderr = ""
    try {
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $InfisicalExecutable
        $startInfo.Arguments = $commandLine
        $startInfo.WorkingDirectory = $repoRoot
        $startInfo.UseShellExecute = $false
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $startInfo.CreateNoWindow = $true

        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $startInfo
        if (-not $process.Start()) {
            Stop-WithCode -Code "INFISICAL_SET_PROCESS_START_FAILED" -Message "Failed to start Infisical CLI."
        }

        $connectionTask = $pipe.WaitForConnectionAsync()
        if (-not $connectionTask.Wait(30000)) {
            if (-not $process.HasExited) {
                $process.Kill()
            }
            Stop-WithCode -Code "INFISICAL_SET_PIPE_CONNECT_TIMEOUT" -Message "Infisical did not read the named pipe within 30 seconds."
        }

        $writer = [System.IO.StreamWriter]::new($pipe, [System.Text.UTF8Encoding]::new($false))
        $writer.Write($secretValue)
        $writer.Flush()
        $writer.Dispose()

        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        $cliExitCode = $process.ExitCode
    } finally {
        if ($null -ne $pipe) {
            $pipe.Dispose()
        }
        if ($null -ne $process) {
            $process.Dispose()
        }
    }

    $cliOutputContainedRawSecret = ($stdout + $stderr).Contains($secretValue)
    $stdoutTail = @(($stdout -split "\r?\n") | Where-Object { $_ } | Select-Object -Last 12 | ForEach-Object { Protect-Text -Text $_ })
    $stderrTail = @(($stderr -split "\r?\n") | Where-Object { $_ } | Select-Object -Last 12 | ForEach-Object { Protect-Text -Text $_ })

    if ($rawKeyInCommandLine) {
        Stop-WithCode -Code "INFISICAL_SET_COMMAND_LINE_LEAK_GUARD" -Message "Raw secret would have appeared in the Infisical command line."
    }
    if ($cliExitCode -ne 0) {
        Stop-WithCode -Code "INFISICAL_SET_CLI_FAILED" -Message "Infisical CLI set command failed."
    }

    $status = "passed"
    $exitCode = 0
} catch {
    if ([string]::IsNullOrWhiteSpace($errorCode)) {
        if ($_.Exception.Message -match "^(INFISICAL_SET_[A-Z0-9_]+):") {
            $errorCode = $Matches[1]
        } else {
            $errorCode = "INFISICAL_SET_UNHANDLED"
        }
    }
    $errorMessage = Protect-Text -Text ([string]$_.Exception.Message)
    $status = "failed"
    $exitCode = 1
} finally {
    $report = [ordered]@{
        schema_version = 1
        status = $status
        error_code = $errorCode
        error_message = $errorMessage
        cli_version = $cliVersion
        auth_read = $authRead
        project_id = $ProjectId
        environment = $Environment
        secret_path = $SecretPath
        secret_name = $SecretName
        value_env_var = $ValueEnvVar
        value_length = if ($secretValue) { $secretValue.Length } else { 0 }
        value_sha256_12 = if ($secretSha256) { $secretSha256.Substring(0, 12) } else { $null }
        cli_exit_code = $cliExitCode
        raw_key_in_command_line = $rawKeyInCommandLine
        cli_output_contained_raw_secret = $cliOutputContainedRawSecret
        cli_output_was_sanitized = $true
        stdout_tail = $stdoutTail
        stderr_tail = $stderrTail
        report_path = $reportPath
        started_at_utc = $startedAtUtc.ToString("O")
        ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
    }

    $reportJson = $report | ConvertTo-Json -Depth 8
    if (-not [string]::IsNullOrEmpty($secretValue) -and $reportJson.Contains($secretValue)) {
        $leakReport = [ordered]@{
            schema_version = 1
            status = "failed"
            error_code = "INFISICAL_SET_REPORT_LEAK_GUARD"
            error_message = "Sanitized set report attempted to include the raw secret value."
            project_id = $ProjectId
            environment = $Environment
            secret_path = $SecretPath
            secret_name = $SecretName
            value_env_var = $ValueEnvVar
            raw_key_in_command_line = $rawKeyInCommandLine
            cli_output_contained_raw_secret = $cliOutputContainedRawSecret
            cli_output_was_sanitized = $true
            report_path = $reportPath
            started_at_utc = $startedAtUtc.ToString("O")
            ended_at_utc = ([DateTimeOffset]::UtcNow).ToString("O")
        }
        $reportJson = $leakReport | ConvertTo-Json -Depth 8
        $exitCode = 1
    }

    $reportJson | Set-Content -LiteralPath $reportPath -Encoding UTF8
    $reportSha256 = Get-Sha256FileHex -Path $reportPath
    $readback = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
    Write-Host "INFISICAL_SECRET_SET_REPORT_READBACK path=$reportPath status=$($readback.status) error_code=$($readback.error_code) exit_code=$($readback.cli_exit_code) raw_key_in_command_line=$($readback.raw_key_in_command_line) cli_output_contained_raw_secret=$($readback.cli_output_contained_raw_secret) sha256_12=$($readback.value_sha256_12) report_sha256=$reportSha256"
}

exit $exitCode
