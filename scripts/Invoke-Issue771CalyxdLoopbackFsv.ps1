[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path,
    [string]$CalyxHome = (Join-Path $PSScriptRoot "..\Calyx"),
    [string]$VaultPath = "",
    [string]$RegistryPath = "",
    [int]$VramBudgetMiB = 28672
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Set-CalyxWindowsEnv.ps1") -CalyxHome $CalyxHome | Out-Host
$CalyxHome = $env:CALYX_HOME
if (-not $VaultPath) {
    $VaultPath = Resolve-GameEditorEngineContentVault -CalyxHome $CalyxHome
} else {
    $VaultPath = Resolve-GameEditorCalyxPath -Path $VaultPath -Label "VaultPath" -Type Container
}
if (-not $RegistryPath) {
    $RegistryPath = Join-Path $CalyxHome "lenses\registry.json"
}
$RegistryPath = Resolve-GameEditorCalyxPath -Path $RegistryPath -Label "RegistryPath" -Type Leaf

$calyxDir = $env:CALYX_HOME
$exe = Join-Path $calyxDir "target\debug\calyxd.exe"
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "ISSUE771_FSV_MISSING_BINARY: build calyxd first at $exe"
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$fsvRoot = Join-Path $RepoRoot "target\fsv\issue771_calyxd_loopback\$stamp"
$logs = Join-Path $fsvRoot "logs"
New-Item -ItemType Directory -Force -Path $logs | Out-Null

function Get-FreeLoopbackPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), 0)
    $listener.Start()
    try {
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

function ConvertTo-TomlPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return $Path.Replace("\", "/")
}

function New-CalyxdConfig {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$MetricsPort,
        [Parameter(Mandatory = $true)][int]$McpPort,
        [Parameter(Mandatory = $true)][string]$VaultPath,
        [Parameter(Mandatory = $true)][string]$RegistryPath,
        [Parameter(Mandatory = $true)][int]$VramBudget
    )
    $health = Join-Path $logs "health.json"
    $text = @"
bind_addr = "127.0.0.1:$MetricsPort"
mcp_bind_addr = "127.0.0.1:$McpPort"
mcp_loopback_plaintext = true
vault_path = "$(ConvertTo-TomlPath $VaultPath)"
registry_path = "$(ConvertTo-TomlPath $RegistryPath)"
vram_budget_mib = $VramBudget
log_dir = "$(ConvertTo-TomlPath $logs)"
health_log_path = "$(ConvertTo-TomlPath $health)"
tei_endpoints = []
healthcheck_timeout_secs = 30
"@
    Set-Content -LiteralPath $Path -Value $text -Encoding UTF8
}

function Start-OwnedCalyxd {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $exe
    $psi.WorkingDirectory = $calyxDir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($arg in $Arguments) {
        [void]$psi.ArgumentList.Add($arg)
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $psi
    [void]$process.Start()
    [pscustomobject]@{
        Process = $process
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
    }
}

function Complete-OwnedProcess {
    param(
        [Parameter(Mandatory = $true)]$Owned,
        [int]$TimeoutMs = 10000
    )
    $process = [System.Diagnostics.Process]$Owned.Process
    if (-not $process.HasExited) {
        $process.Kill()
        [void]$process.WaitForExit($TimeoutMs)
    }
    $stdoutText = $process.StandardOutput.ReadToEnd()
    $stderrText = $process.StandardError.ReadToEnd()
    Set-Content -LiteralPath $Owned.StdoutPath -Value $stdoutText -Encoding UTF8
    Set-Content -LiteralPath $Owned.StderrPath -Value $stderrText -Encoding UTF8
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        Stdout = $stdoutText
        Stderr = $stderrText
        StdoutPath = $Owned.StdoutPath
        StderrPath = $Owned.StderrPath
    }
}

function Wait-OwnedProcess {
    param(
        [Parameter(Mandatory = $true)]$Owned,
        [int]$TimeoutMs = 180000
    )
    $process = [System.Diagnostics.Process]$Owned.Process
    if (-not $process.WaitForExit($TimeoutMs)) {
        $completed = Complete-OwnedProcess -Owned $Owned
        throw "ISSUE771_FSV_PROCESS_TIMEOUT: pid=$($process.Id) stdout=$($completed.StdoutPath) stderr=$($completed.StderrPath)"
    }
    Complete-OwnedProcess -Owned $Owned
}

function Test-LoopbackPort {
    param([Parameter(Mandatory = $true)][int]$Port)
    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $iar = $client.BeginConnect("127.0.0.1", $Port, $null, $null)
        if (-not $iar.AsyncWaitHandle.WaitOne(500)) {
            return $false
        }
        $client.EndConnect($iar)
        return $true
    } catch {
        return $false
    } finally {
        $client.Close()
    }
}

function Read-Exact {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)][int]$Count
    )
    $buffer = New-Object byte[] $Count
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($buffer, $offset, $Count - $offset)
        if ($read -le 0) {
            throw "ISSUE771_FSV_MCP_EOF: wanted $Count bytes, got $offset"
        }
        $offset += $read
    }
    $buffer
}

function Send-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)][object]$Request
    )
    $json = $Request | ConvertTo-Json -Depth 32 -Compress
    $body = [System.Text.Encoding]::UTF8.GetBytes($json)
    $len = [BitConverter]::GetBytes([UInt32]$body.Length)
    if ([BitConverter]::IsLittleEndian) {
        [Array]::Reverse($len)
    }
    $Stream.Write($len, 0, 4)
    $Stream.Write($body, 0, $body.Length)
    $Stream.Flush()
}

function Receive-JsonRpc {
    param([Parameter(Mandatory = $true)][System.IO.Stream]$Stream)
    $lenBytes = Read-Exact -Stream $Stream -Count 4
    if ([BitConverter]::IsLittleEndian) {
        [Array]::Reverse($lenBytes)
    }
    $len = [BitConverter]::ToUInt32($lenBytes, 0)
    if ($len -eq 0 -or $len -gt 4194304) {
        throw "ISSUE771_FSV_FRAME_INVALID: length=$len"
    }
    $body = Read-Exact -Stream $Stream -Count ([int]$len)
    $json = [System.Text.Encoding]::UTF8.GetString($body)
    [pscustomobject]@{
        Json = $json
        Value = ($json | ConvertFrom-Json)
    }
}

function Invoke-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)][object]$Request
    )
    Send-JsonRpc -Stream $Stream -Request $Request
    $response = Receive-JsonRpc -Stream $Stream
    $errorProperty = $response.Value.PSObject.Properties["error"]
    if ($null -ne $errorProperty -and $null -ne $errorProperty.Value) {
        throw "ISSUE771_FSV_MCP_ERROR: $($response.Json)"
    }
    $response.Value
}

function Invoke-McpTool {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)][int]$Id,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][object]$Arguments
    )
    $response = Invoke-JsonRpc -Stream $Stream -Request ([ordered]@{
        jsonrpc = "2.0"
        id = $Id
        method = "tools/call"
        params = [ordered]@{
            name = $Name
            arguments = $Arguments
        }
    })
    $response.result.content[0].text | ConvertFrom-Json
}

function Run-EdgeProcess {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$ConfigPath
    )
    $owned = Start-OwnedCalyxd `
        -Arguments @("--config", $ConfigPath, "--once") `
        -StdoutPath (Join-Path $fsvRoot "$Name.stdout.log") `
        -StderrPath (Join-Path $fsvRoot "$Name.stderr.log")
    $completed = Wait-OwnedProcess -Owned $owned -TimeoutMs 180000
    [ordered]@{
        exit_code = $completed.ExitCode
        stdout_path = $completed.StdoutPath
        stderr_path = $completed.StderrPath
        stderr = $completed.Stderr
    }
}

$metricsPort = Get-FreeLoopbackPort
$mcpPort = Get-FreeLoopbackPort
$configPath = Join-Path $fsvRoot "calyxd.local.toml"
New-CalyxdConfig -Path $configPath -MetricsPort $metricsPort -McpPort $mcpPort -VaultPath $VaultPath -RegistryPath $RegistryPath -VramBudget $VramBudgetMiB

$daemon = $null
$daemonOutput = $null
$happy = $null
try {
    $daemon = Start-OwnedCalyxd `
        -Arguments @("--config", $configPath) `
        -StdoutPath (Join-Path $fsvRoot "calyxd.stdout.log") `
        -StderrPath (Join-Path $fsvRoot "calyxd.stderr.log")

    $ready = $false
    for ($i = 0; $i -lt 240; $i++) {
        $process = [System.Diagnostics.Process]$daemon.Process
        if ($process.HasExited) {
            $daemonOutput = Complete-OwnedProcess -Owned $daemon
            throw "ISSUE771_FSV_DAEMON_EXITED: code=$($daemonOutput.ExitCode) stderr=$($daemonOutput.Stderr)"
        }
        if ((Test-LoopbackPort -Port $mcpPort) -and (Test-Path -LiteralPath (Join-Path $logs "health.json"))) {
            $ready = $true
            break
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) {
        throw "ISSUE771_FSV_NOT_READY: MCP port $mcpPort did not become ready"
    }

    $client = [System.Net.Sockets.TcpClient]::new("127.0.0.1", $mcpPort)
    $client.ReceiveTimeout = 10000
    $client.SendTimeout = 10000
    $stream = $client.GetStream()
    try {
        $initialized = Invoke-JsonRpc -Stream $stream -Request ([ordered]@{
            jsonrpc = "2.0"
            id = 1
            method = "initialize"
        })
        $listed = Invoke-JsonRpc -Stream $stream -Request ([ordered]@{
            jsonrpc = "2.0"
            id = 2
            method = "tools/list"
        })
        $tools = @($listed.result.tools)
        $toolNames = @($tools | ForEach-Object { $_.name })
        foreach ($required in @("calyx.create_vault", "calyx.add_lens", "calyx.ingest", "calyx.search", "calyx.provenance")) {
            if ($toolNames -notcontains $required) {
                throw "ISSUE771_FSV_MISSING_TOOL: $required"
            }
        }

        $vaultName = "issue771-loopback-$stamp"
        $created = Invoke-McpTool -Stream $stream -Id 3 -Name "calyx.create_vault" -Arguments ([ordered]@{
            name = $vaultName
            panel_template = "text-default"
        })
        $added = Invoke-McpTool -Stream $stream -Id 4 -Name "calyx.add_lens" -Arguments ([ordered]@{
            vault = $vaultName
            name = "byte_axis"
            runtime = "algorithmic"
        })
        $ingested = Invoke-McpTool -Stream $stream -Id 5 -Name "calyx.ingest" -Arguments ([ordered]@{
            vault = $vaultName
            input = "issue 771 local calyxd loopback semantic backend proof"
        })
        $searched = Invoke-McpTool -Stream $stream -Id 6 -Name "calyx.search" -Arguments ([ordered]@{
            vault = $vaultName
            query = "local calyxd loopback proof"
            k = 1
        })
        if (@($searched.hits).Count -lt 1) {
            throw "ISSUE771_FSV_EMPTY_SEARCH"
        }
        $provenance = Invoke-McpTool -Stream $stream -Id 7 -Name "calyx.provenance" -Arguments ([ordered]@{
            vault = $vaultName
            cx_id = $ingested.cx_id
        })
    } finally {
        $stream.Dispose()
        $client.Dispose()
    }

    $healthPath = Join-Path $logs "health.json"
    $health = Get-Content -LiteralPath $healthPath -Raw | ConvertFrom-Json
    if ($health.status -ne "pass") {
        throw "ISSUE771_FSV_HEALTH_FAIL: $(Get-Content -LiteralPath $healthPath -Raw)"
    }

    $metricsText = (Invoke-WebRequest -Uri "http://127.0.0.1:$metricsPort/metrics" -UseBasicParsing -TimeoutSec 10).Content
    $metricsPath = Join-Path $fsvRoot "metrics.txt"
    Set-Content -LiteralPath $metricsPath -Value $metricsText -Encoding UTF8

    $registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
    $registryLensCount = @($registry.lenses).Count
    $indexPath = Join-Path $CalyxHome "vaults\index.json"
    $index = Get-Content -LiteralPath $indexPath -Raw | ConvertFrom-Json
    $indexEntry = @($index.vaults | Where-Object { $_.name -eq $vaultName }) | Select-Object -First 1
    if ($null -eq $indexEntry) {
        throw "ISSUE771_FSV_INDEX_MISSING: $vaultName"
    }
    $createdVaultDir = Join-Path $CalyxHome $indexEntry.path
    if (-not (Test-Path -LiteralPath $createdVaultDir -PathType Container)) {
        throw "ISSUE771_FSV_VAULT_DIR_MISSING: $createdVaultDir"
    }
    $vaultFiles = @(Get-ChildItem -LiteralPath $createdVaultDir -Recurse -File |
        Sort-Object FullName |
        Select-Object -First 20 |
        ForEach-Object {
            [ordered]@{
                relative = $_.FullName.Substring($createdVaultDir.Length).TrimStart("\")
                len = $_.Length
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        })

    $happy = [ordered]@{
        daemon_pid = ([System.Diagnostics.Process]$daemon.Process).Id
        exe = $exe
        config_path = $configPath
        metrics_bind = "127.0.0.1:$metricsPort"
        mcp_bind = "127.0.0.1:$mcpPort"
        vault_path = $VaultPath
        registry_path = $RegistryPath
        registry_lens_count = $registryLensCount
        registry_sha256 = (Get-FileHash -LiteralPath $RegistryPath -Algorithm SHA256).Hash.ToLowerInvariant()
        initialized_server = $initialized.result.serverInfo.name
        tool_count = @($tools).Count
        required_tools_present = $true
        created = $created
        added = $added
        ingested = $ingested
        search_hit_count = @($searched.hits).Count
        first_search_hit = @($searched.hits)[0]
        provenance = $provenance
        health_path = $healthPath
        health = $health
        metrics_path = $metricsPath
        metrics_has_verify_restore_ok = ($metricsText -match "calyx_verify_restore_ok")
        index_path = $indexPath
        index_sha256 = (Get-FileHash -LiteralPath $indexPath -Algorithm SHA256).Hash.ToLowerInvariant()
        created_vault_index_entry = $indexEntry
        created_vault_dir = $createdVaultDir
        created_vault_file_sample = $vaultFiles
        stdout_path = $daemon.StdoutPath
        stderr_path = $daemon.StderrPath
    }
} finally {
    if ($null -ne $daemon -and $null -eq $daemonOutput) {
        $daemonOutput = Complete-OwnedProcess -Owned $daemon
    }
}

if ($null -eq $happy) {
    throw "ISSUE771_FSV_HAPPY_PATH_INCOMPLETE"
}

$downClient = [System.Net.Sockets.TcpClient]::new()
$daemonDown = [ordered]@{
    attempted_bind = "127.0.0.1:$mcpPort"
    refused = $false
    error = $null
}
try {
    $iar = $downClient.BeginConnect("127.0.0.1", $mcpPort, $null, $null)
    if ($iar.AsyncWaitHandle.WaitOne(1000)) {
        $downClient.EndConnect($iar)
        $daemonDown.refused = $false
    } else {
        $daemonDown.refused = $true
        $daemonDown.error = "timeout"
    }
} catch {
    $daemonDown.refused = $true
    $daemonDown.error = $_.Exception.Message
} finally {
    $downClient.Close()
}

$missingCfg = Join-Path $fsvRoot "edge-vault-missing.toml"
New-CalyxdConfig -Path $missingCfg -MetricsPort (Get-FreeLoopbackPort) -McpPort (Get-FreeLoopbackPort) -VaultPath (Join-Path $fsvRoot "missing-vault") -RegistryPath $RegistryPath -VramBudget $VramBudgetMiB
$vaultMissing = Run-EdgeProcess -Name "edge-vault-missing" -ConfigPath $missingCfg

$vramCfg = Join-Path $fsvRoot "edge-vram-cap.toml"
New-CalyxdConfig -Path $vramCfg -MetricsPort (Get-FreeLoopbackPort) -McpPort (Get-FreeLoopbackPort) -VaultPath $VaultPath -RegistryPath $RegistryPath -VramBudget 1
$vramCap = Run-EdgeProcess -Name "edge-vram-cap" -ConfigPath $vramCfg

$missingRegistryCfg = Join-Path $fsvRoot "edge-registry-missing.toml"
New-CalyxdConfig -Path $missingRegistryCfg -MetricsPort (Get-FreeLoopbackPort) -McpPort (Get-FreeLoopbackPort) -VaultPath $VaultPath -RegistryPath (Join-Path $fsvRoot "missing-registry.json") -VramBudget $VramBudgetMiB
$registryMissing = Run-EdgeProcess -Name "edge-registry-missing" -ConfigPath $missingRegistryCfg

if (-not $daemonDown.refused) {
    throw "ISSUE771_FSV_EDGE_DAEMON_DOWN_DID_NOT_REFUSE"
}
if ($vaultMissing.exit_code -eq 0 -or $vaultMissing.stderr -notmatch "CALYX_DAEMON_CONFIG_INVALID") {
    throw "ISSUE771_FSV_EDGE_VAULT_MISSING_WRONG_ERROR: $($vaultMissing.stderr)"
}
if ($vramCap.exit_code -eq 0 -or $vramCap.stderr -notmatch "CALYX_FORGE_VRAM_BUDGET") {
    throw "ISSUE771_FSV_EDGE_VRAM_WRONG_ERROR: $($vramCap.stderr)"
}
if ($registryMissing.exit_code -eq 0 -or $registryMissing.stderr -notmatch "read lens registry") {
    throw "ISSUE771_FSV_EDGE_REGISTRY_WRONG_ERROR: $($registryMissing.stderr)"
}

$stdoutText = $daemonOutput.Stdout
$stderrText = $daemonOutput.Stderr
$report = [ordered]@{
    issue = 771
    timestamp = (Get-Date).ToUniversalTime().ToString("o")
    source_of_truth = "live Windows calyxd.exe process on loopback, length-prefixed MCP TCP frames, $CalyxHome vault/index and lens registry bytes, health JSON, and /metrics HTTP readback"
    fsv_root = $fsvRoot
    happy = $happy
    edges = [ordered]@{
        daemon_down = $daemonDown
        vault_missing = $vaultMissing
        vram_cap_exceeded = $vramCap
        registry_missing = $registryMissing
    }
    stdout_contains_registry_lenses_50 = ($stdoutText -match "registry_lenses=50")
    stdout_contains_mcp_bind = ($stdoutText -match [Regex]::Escape("mcp_bind=127.0.0.1:$mcpPort"))
    stdout_tail = (($stdoutText -split "`r?`n") | Select-Object -Last 20)
    stderr_tail = (($stderrText -split "`r?`n") | Select-Object -Last 20)
    owned_process_cleanup = "Killed only the FSV-owned calyxd.exe process after successful loopback proof; no unrelated processes controlled."
}

$reportPath = Join-Path $fsvRoot "issue771-calyxd-loopback-fsv.json"
$report | ConvertTo-Json -Depth 64 | Set-Content -LiteralPath $reportPath -Encoding UTF8
$sha = (Get-FileHash -LiteralPath $reportPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host "ISSUE771_FSV_REPORT=$reportPath"
Write-Host "ISSUE771_FSV_SHA256=$sha"
Write-Host "ISSUE771_FSV_ROOT=$fsvRoot"
