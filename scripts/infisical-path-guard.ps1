# Shared guard against MSYS2 / Git-Bash POSIX-to-Windows path mangling of the
# Infisical --path argument (GitHub #1546). ASCII-only for PowerShell 5.1.
#
# Background: MSYS2 (the runtime under Git Bash / the Bash tool) rewrites any
# argument that looks like an absolute POSIX path (e.g. /gameeditor) into the
# corresponding Windows path under the Git install root
# (e.g. C:/Program Files/Git/gameeditor). The Infisical CLI then faithfully
# requests that garbage secretPath and the SERVER returns:
#
#   error: failed to fetch secrets for path "C:/Program Files/Git/gameeditor":
#   ... secretPath=C%3A%2FProgram+Files%2FGit%2Fgameeditor ...
#   [status-code=400] [message="Invalid secret path. Only alphanumeric
#   characters, dashes, and underscores are allowed."]
#   If you are using a service token to fetch secrets, please ensure it is valid
#
# That 400 message and the service-token hint both point at AUTH, which is a
# red herring: it is neither an Infisical bug nor an auth/key problem. An agent
# that trusts the message is led toward re-verifying or ROTATING the API key,
# which AGENTS.md forbids. This guard intercepts the corruption BEFORE the CLI
# call so the misdiagnosis never happens.
#
# Detection is deliberately narrow: it only fires on the Windows-path signature
# MSYS mangling produces (a drive-letter prefix, a backslash, or the literal
# "Program Files"). A path that is merely invalid but NOT Windows-shaped
# (e.g. "/bad path" with a space) is left alone so a genuine server-side
# "Invalid secret path" rejection is never misattributed to MSYS.

function Test-InfisicalPathMangled {
    [OutputType([bool])]
    param(
        [AllowNull()]
        [AllowEmptyString()]
        [string]$SecretPath
    )

    if ([string]::IsNullOrEmpty($SecretPath)) {
        return $false
    }
    # Drive-letter prefix, e.g. "C:/..." or "C:\..." (MSYS always emits one).
    if ($SecretPath -match '^[A-Za-z]:[\\/]') {
        return $true
    }
    # A backslash can never appear in a legitimate POSIX Infisical path.
    if ($SecretPath -match '\\') {
        return $true
    }
    # The classic Git-for-Windows install-root tell.
    if ($SecretPath -match '(?i)Program Files') {
        return $true
    }
    return $false
}

function Get-InfisicalPathMangledMessage {
    param(
        [AllowNull()]
        [AllowEmptyString()]
        [string]$SecretPath
    )

    return (
        "--path was rewritten to '$SecretPath' by MSYS/Git-Bash path translation " +
        "(MSYS2 POSIX-to-Windows argument conversion). This is NOT an auth error and the " +
        "Infisical API key / service token is NOT the problem. Do NOT rotate, regenerate, " +
        "or visit any provider/API-key page. Fix: run this from PowerShell, OR prefix the " +
        "command with MSYS_NO_PATHCONV=1 " +
        "(e.g. MSYS_NO_PATHCONV=1 infisical run --projectId=... --env=prod --path=/gameeditor -- <cmd>), " +
        "OR call it through scripts/infisical-run.sh which sets MSYS_NO_PATHCONV=1 for you."
    )
}
