#!/usr/bin/env bash
# Git-Bash / MSYS2-safe wrapper for the Infisical CLI (GitHub #1546).
#
# WHY THIS EXISTS
#   Under Git Bash / MSYS2 (including the Bash tool), the msys2 runtime rewrites
#   any argument that looks like an absolute POSIX path -- e.g. --path=/gameeditor
#   -- into a Windows path under the Git install root
#   (--path=C:/Program Files/Git/gameeditor). The Infisical CLI then requests a
#   garbage secretPath and the server answers:
#
#     error: failed to fetch secrets for path "C:/Program Files/Git/gameeditor":
#     ... [status-code=400]
#     [message="Invalid secret path. Only alphanumeric characters, dashes, and
#     underscores are allowed."]
#     If you are using a service token to fetch secrets, please ensure it is valid
#
#   That message points at AUTH and is a LIE: it is neither an Infisical bug nor
#   an auth/key problem. Do NOT rotate or re-verify the API key on the strength
#   of it -- that is exactly the misdiagnosis this wrapper exists to prevent.
#
# WHAT THIS DOES
#   1. Exports MSYS_NO_PATHCONV=1 and MSYS2_ARG_CONV_EXCL='*' so MSYS leaves the
#      --path argument (and every other argument) untouched.
#   2. Fails loud with INFISICAL_PATH_MANGLED if a --path value still carries a
#      Windows-path signature (a caller may have passed an already-mangled value).
#   3. exec's the real Infisical CLI with the original arguments.
#
# USAGE (drop-in for `infisical`):
#   scripts/infisical-run.sh run --projectId=<id> --env=prod --path=/gameeditor -- <command>
#   scripts/infisical-run.sh secrets get NAME --projectId=<id> --env=prod --path=/gameeditor
#
# NOTE: On Windows, PowerShell is the recommended host (it never mangles paths).
#       This wrapper is for anyone who must drive Infisical from Git Bash.

set -euo pipefail

# Belt and suspenders: two independent MSYS knobs that both suppress the
# POSIX-to-Windows argument conversion for the child (native) process.
#   MSYS_NO_PATHCONV=1        -> Git-for-Windows global off switch.
#   MSYS2_ARG_CONV_EXCL='*'   -> MSYS2 fine-grained switch; '*' excludes all args.
export MSYS_NO_PATHCONV=1
export MSYS2_ARG_CONV_EXCL='*'

# Detect a Windows-path signature that only appears when MSYS has already
# mangled a POSIX path. Narrow on purpose: a merely-invalid path (e.g. one with
# a space) is NOT flagged, so a genuine server-side rejection is never
# misattributed to MSYS.
is_mangled_path() {
    local value="$1"
    case "$value" in
        [A-Za-z]:[/\\]*) return 0 ;;   # drive-letter prefix: C:/... or C:\...
        *\\*)            return 0 ;;   # backslash: impossible in a POSIX path
        *"Program Files"*) return 0 ;; # classic Git install-root tell
    esac
    return 1
}

for arg in "$@"; do
    case "$arg" in
        --path=*)
            path_value="${arg#--path=}"
            if is_mangled_path "$path_value"; then
                echo "INFISICAL_PATH_MANGLED: --path was rewritten to '${path_value}' by MSYS/Git-Bash path translation (MSYS2 POSIX-to-Windows argument conversion). This is NOT an auth error and the Infisical API key / service token is NOT the problem. Do NOT rotate, regenerate, or visit any provider/API-key page. Fix: run this from PowerShell, or invoke this wrapper with a clean POSIX --path=/gameeditor value." >&2
                exit 2
            fi
            ;;
    esac
done

exec infisical "$@"
