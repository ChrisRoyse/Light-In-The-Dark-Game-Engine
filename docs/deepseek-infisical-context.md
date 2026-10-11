# GameEditor DeepSeek Infisical Context

Updated: 2026-07-04. Latest source-of-truth verification: 2026-07-04
14:26 -05:00. Latest routine-gate fix: 2026-07-04 16:22 -05:00.

## Source Of Truth

Infisical is the only source of truth for the GameEditor DeepSeek key. The key
is already settled there; future agents must not return to the DeepSeek
provider/API-key pages to find, copy, compare, or replace it unless the operator
explicitly asks for provider-side rotation in the current conversation.

On 2026-07-03, the operator-provided key was rotated to the same Infisical
source of truth. Later operator-provided values, including explicit
2026-07-04 refreshes, were written through the same wrapper and matched the
stored Infisical hash. A 2026-07-04 14:26 -05:00 refresh verified the stored
source of truth through direct Infisical readback, injected-process readback, a
real DeepSeek provider request, and a repository readback proving the normal
format gate no longer runs any DeepSeek/Infisical audit. Treat Infisical
storage/injection and provider acceptance as settled unless a future manual
verification run regresses.

Do not store the key in repo files, issue comments, screenshots, shell history,
Codex memory, local env files, command transcripts, test fixtures, temporary
files, or ad hoc command arguments.

No provider key hunt: use Infisical only unless the operator explicitly asks for
provider-side rotation in the current conversation.

## Shell Requirement: PowerShell required; Git Bash mangles `--path` (#1546)

Run every `infisical` command from **PowerShell**. Do **not** run the raw
`infisical run ... --path=/gameeditor ...` wrapper from **Git Bash / MSYS2**
(the Bash tool). MSYS2 argument path-translation rewrites the POSIX-looking
`/gameeditor` into a Windows path under the Git install root, so the CLI
requests a garbage `secretPath` and the server returns this exact signature:

```
error: failed to fetch secrets for path "C:/Program Files/Git/gameeditor": CallGetRawSecretsV3 Unsuccessful response [GET https://app.infisical.com/api/v3/secrets/raw?environment=prod&expandSecretReferences=true&include_imports=true&secretPath=C%3A%2FProgram+Files%2FGit%2Fgameeditor&workspaceId=c2d7e44c-...] [status-code=400] [request-id=req-...] [message="Invalid secret path. Only alphanumeric characters, dashes, and underscores are allowed."]
Could not fetch secrets
If you are using a service token to fetch secrets, please ensure it is valid
```

**That message is a liar.** The `Invalid secret path` 400 and the
`service token ... valid` hint both point at auth. This is NOT an auth error and
the API key / service token is NOT the problem. **Do NOT rotate, regenerate, or
open any DeepSeek/provider API-key page on the strength of it** — that is the
forbidden workflow this footgun pushes you toward. The tell is that the resolved
`--path` contains a drive letter, a backslash, or `Program Files` instead of
matching `^/[A-Za-z0-9_-]+$`.

Fixes, in order of preference:

1. **Run from PowerShell.** PowerShell never applies MSYS path translation.
2. If you must use Git Bash, prefix the command with `MSYS_NO_PATHCONV=1`:
   ```bash
   MSYS_NO_PATHCONV=1 infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>
   ```
   (`MSYS2_ARG_CONV_EXCL='*'` is the equivalent finer-grained knob.)
3. Or drive Infisical through `scripts/infisical-run.sh`, which sets
   `MSYS_NO_PATHCONV=1` for you and fails loud with `INFISICAL_PATH_MANGLED`
   if a `--path` value still carries a Windows-path signature:
   ```bash
   scripts/infisical-run.sh run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>
   ```

The repo wrappers (`scripts/verify-infisical-secret.ps1`,
`scripts/verify-deepseek-api.ps1`, `scripts/set-infisical-secret-from-env.ps1`)
share `scripts/infisical-path-guard.ps1` and fail closed with the greppable code
`INFISICAL_PATH_MANGLED` (naming MSYS, not auth) if handed a mangled `SecretPath`,
before any network call. Documentation alone was judged insufficient for #1546:
the failure mode misdirects, so the guard must intercept the misdiagnosis at the
tool boundary, not merely warn someone who already knows to look.

- Infisical project: `leapable-aiwonder-prod`
- Project id: `c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5`
- Environment: `prod`
- Secret path: `/gameeditor`
- Secret name: `GAMEEDITOR_DEEPSEEK_API_KEY`

Use Infisical injection for any command that needs the key. **PowerShell is
required** (see "Shell Requirement" below; Git Bash silently corrupts `--path`):

```powershell
infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>
# -> INF Injecting 1 Infisical secrets into your application process
```

Under Git Bash / MSYS, path translation rewrites the POSIX-looking `--path=/gameeditor`
into a Windows path under the Git install root, and the CLI faithfully requests a garbage
`secretPath`:

```
error: failed to fetch secrets for path "C:/Program Files/Git/gameeditor":
... [status-code=400] [message="Invalid secret path. Only alphanumeric characters, dashes,
and underscores are allowed."]
```

**That 400 is a path bug, not an auth bug.** The message — and the CLI's trailing hint about
service tokens — both point at credentials, which is a red herring that leads straight into
the key-rotation workflow this document forbids. If you must use bash, disable the mangling:

```bash
MSYS_NO_PATHCONV=1 infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>
```

Code that needs DeepSeek must read only `GAMEEDITOR_DEEPSEEK_API_KEY` from the
process environment after that injection. Do not add `.env` loading, alternate
secret names, provider-page scraping, backup providers, fake keys, or fallback
model paths.

## Explicit Rotation Only

The current key is already stored in Infisical. Future agents must not rotate it,
search for another key, or ask the operator for the key again unless the
operator explicitly requests provider-side rotation in that same conversation.

If explicit rotation is requested, keep the same Infisical project id,
environment, path, and secret name. Do not put the raw key in command-line
arguments, shell history, repo files, screenshots, persisted temporary files, or
issue text. Put the value in a process environment variable for that owned run
and call `scripts\set-infisical-secret-from-env.ps1`; it writes the value to
Infisical through a named pipe and persists only a sanitized report. This
Infisical CLI version can echo the raw secret in `secrets set` output, so do
not call raw `infisical secrets set KEY=value` or capture unsanitized set
output. Then immediately run `scripts\verify-infisical-secret.ps1` and
`scripts\verify-deepseek-api.ps1` as manual FSV for that rotation only, after
`infisical login status --silent` passes. These commands must not be added to
normal test suites. Update #1343 or the active issue only with sanitized report
paths, hashes, status codes, request ids, model ids, and error codes.

## Future Agent Contract

For any future GameEditor DeepSeek work, use this exact operating model:

1. Treat Infisical as the source of truth. The secret is already stored at the
   project id, environment, path, and name above.
2. Do not ask the operator for the key again and do not open DeepSeek provider
   key pages unless the operator explicitly asks for provider-side rotation in
   the current conversation.
3. Do not run `scripts\verify-infisical-secret.ps1`,
   `scripts\verify-deepseek-api.ps1`, `scripts\set-infisical-secret-from-env.ps1`,
   `infisical login`, `infisical run`, or `infisical secrets` from normal build,
   format, test, CI, or unrelated issue work. These are manual-only operations
   for explicit DeepSeek/Infisical verification or operator-requested rotation.
   Do not add separate DeepSeek/Infisical audit tests to routine gates either;
   `scripts\check-format.ps1` must remain free of DeepSeek/Infisical checks.
4. When explicit DeepSeek/Infisical work is required, first confirm CLI auth
   with `infisical login status --silent` in an already-authenticated shell.
   If it fails, stop with a sanitized issue update; do not start browser login
   loops or ask for another DeepSeek key.
5. Verify secret-manager state with `scripts\verify-infisical-secret.ps1` only
   when the active task requires secret-manager evidence.
6. Run every key-consuming command through the documented `infisical run`
   command so `GAMEEDITOR_DEEPSEEK_API_KEY` exists only in that child process.
7. Use only the documented OpenAI-compatible DeepSeek chat-completions contract:
   bearer auth, base URL `https://api.deepseek.com`, endpoint
   `/chat/completions`, and model id `deepseek-v4-flash` or
   `deepseek-v4-pro`.
8. Verify provider acceptance separately with `scripts\verify-deepseek-api.ps1`
   only when provider behavior must be proven. A passing Infisical check is not
   provider proof.
9. Update #1343 or the active issue with sanitized report paths, hashes, HTTP
   status, request id, model, and failure code. Never include raw key material.

Implementation must fail closed. Missing/empty env var, Infisical failure,
non-2xx provider response, non-JSON provider body, missing response id/model,
missing `choices[0].message.content`, missing token usage, or sentinel mismatch
must produce explicit structured errors and sanitized logs. Do not add local
fallback text, alternate providers, `.env` discovery, mock keys, fake responses,
legacy model ids, or tests that pass without the real Infisical-injected key
when the behavior under test requires DeepSeek.

## Agent Runbook

1. Read this file and the `AI Provider Secrets` section in `AGENTS.md`.
2. For normal repo work, do not run any DeepSeek/Infisical verification or
   rotation script. Do not add DeepSeek/Infisical audit gates to normal test,
   format, CI, or unrelated issue verification.
3. For explicit DeepSeek/Infisical work, run `infisical login status --silent`
   first. If it reports unauthenticated, stop and update #1343 with sanitized
   evidence; do not trigger browser login from an agent shell.
4. Run `scripts\verify-infisical-secret.ps1` only when secret-manager evidence is
   required by the active task.
5. Run the real command through `infisical run ... -- <command>`.
6. For provider/API behavior, run `scripts\verify-deepseek-api.ps1` only when
   provider acceptance must be proven and trust only its sanitized persisted
   report plus readback.
7. If Infisical verification fails, fix Infisical access/path/config; do not
   continue with local env values.
8. If DeepSeek returns HTTP 401, update #1343 or the active issue with
   sanitized evidence. Do not hunt for a different key or open provider key
   pages unless the operator explicitly asks for rotation.

## Verification Commands

These commands are manual-only. Do not wire them into `cargo test`, format
checks, CI, or unrelated issue verification. Run them only for explicit
DeepSeek/Infisical work after `infisical login status --silent` passes.

Run the presence and injection check when secret-manager evidence is required:

```powershell
scripts\verify-infisical-secret.ps1
```

For an explicit operator-requested rotation, place the new value in a process
environment variable for that owned shell and run the named-pipe wrapper:

```powershell
scripts\set-infisical-secret-from-env.ps1
```

Run the provider check only when provider/API behavior must be proven:

```powershell
scripts\verify-deepseek-api.ps1
```

Both scripts write sanitized readback reports under `target\fsv\...`; these
reports may include secret name, path, value length, hashes, request ids, HTTP
status, model, and response schema evidence. They must never include the raw
secret value.

Latest Infisical source-of-truth readback:

- Report:
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142\after-edges-infisical\infisical-secret-verify-GAMEEDITOR_DEEPSEEK_API_KEY-20260704-142457-357.json`
- SHA256:
  `b75e04727be64cd738e5b61e29c36e34e9caa39399601f9003bf5dce02ed7013`
- Status: `passed`
- Secret source: project id `c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5`,
  environment `prod`, path `/gameeditor`, name
  `GAMEEDITOR_DEEPSEEK_API_KEY`
- Sanitized readback: present, non-empty, direct Infisical read and injected
  process value hashes match, value length `35`, hash prefix `539228bca0bb`,
  and neither the direct read nor injected process printed the raw secret
- Auth readback: CLI keyring authenticated; `infisical version 0.43.96`

Latest Infisical write readback:

- Report:
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142\set\infisical-secret-set-GAMEEDITOR_DEEPSEEK_API_KEY-20260704-142217-921.json`
- SHA256:
  `81b9f602c2be09a3b2fe8fc159dea1d588a82f8213232152bd99fd148cbfbf3c`
- Status: `passed`
- Sanitized readback: wrapper set exit code `0`, value length `35`, hash prefix
  `539228bca0bb`, raw key not in command line, Infisical CLI set output
  contained the raw value internally, and the wrapper sanitized it and did not
  persist it.

Latest DeepSeek provider acceptance readback:

- Report:
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142\provider-after-final-script\deepseek-api-verify-20260704-142654-130.json`
- SHA256:
  `66d2aafad9d4c8e86e2b641d1cdf185aed34c347f5b35e28bf31baff80fc7259`
- Status: `passed`
- Error: none
- HTTP status: `200`
- Endpoint: `https://api.deepseek.com/chat/completions`
- Model: `deepseek-v4-flash`
- Sanitized provider readback: exact sentinel `GAMEEDITOR_DEEPSEEK_OK`,
  response schema accepted, positive token usage, and `raw_secret_printed=false`.
- Interpretation: Infisical is settled, injection works, and DeepSeek accepts
  the stored operator-provided key.

Latest DeepSeek wrapper fail-closed edge readbacks:

- Summary:
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142\manual-deepseek-infisical-permanent-fix-summary.json`
- SHA256:
  `da2d3a91bc1267c71d8d8e5f14201fe988c68ca4ba42380ec166f7a6c99ed742`
- Status: `passed`, raw DeepSeek-key-shaped matches `0`
- Missing rotation environment variable failed closed with
  `INFISICAL_SET_VALUE_MISSING`.
- Legacy model id `deepseek-chat` failed closed with
  `DEEPSEEK_VERIFY_INVALID_MODEL` before any provider request.
- Infisical readback after all failed edges still showed hash prefix
  `539228bca0bb` and direct/injected hash agreement.

Latest Synapse/source-of-truth readback:

- Synapse shell job `019f2e94-e4b4-7b02-855e-f5d623925e42` ran the named-pipe
  setter with only the environment variable name visible in persisted job
  metadata. Synapse redacted command metadata, reported `env_keys` only, and
  returned exit code `0`.
- Local manual FSV readbacks returned `status=passed`, `raw_matches=0`,
  `provider_http=200`, and `provider_actual=GAMEEDITOR_DEEPSEEK_OK`.
- Consolidated summary:
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142\manual-deepseek-infisical-permanent-fix-summary.json`
  with SHA256
  `da2d3a91bc1267c71d8d8e5f14201fe988c68ca4ba42380ec166f7a6c99ed742`.
- The current verification tree is
  `target\fsv\deepseek_rotation_20260704-user-key-refresh-20260704-141142`.
  It contains the direct/injected Infisical readbacks, provider reports, edge
  reports, and the consolidated summary.
- As of 2026-07-04 16:22 -05:00, `scripts\check-format.ps1` no longer runs any
  DeepSeek/Infisical audit gate. Routine verification must remain independent
  of the DeepSeek/Infisical workflow.
- Routine-gate removal readback:
  `target\fsv\deepseek_routine_gate_removed_20260704-1622\deepseek-routine-gate-removal-readback.json`,
  SHA256
  `b28c8f25ef81438ac930ac78fadf84dff026e883da3d9eece88ca57033043c55`.
  Status `passed`: routine entrypoints had forbidden reference count `0`, the
  obsolete audit script was absent, manual wrappers remained present but
  unreferenced by `check-format.ps1`, and docs/AGENTS rules were present.

## API Contract

DeepSeek uses an OpenAI-compatible chat-completions API:

- Base URL: `https://api.deepseek.com`
- Chat endpoint: `/chat/completions`
- Authentication: HTTP bearer token from `GAMEEDITOR_DEEPSEEK_API_KEY`
- Current model ids: `deepseek-v4-flash` and `deepseek-v4-pro`
- Recommended deterministic verification request: `temperature: 0`,
  `stream: false`, `thinking: { "type": "disabled" }`, low `max_tokens`, and a
  sentinel response that must match exactly.

Do not add new GameEditor integrations using `deepseek-chat` or
`deepseek-reasoner`; DeepSeek documents them as compatibility aliases scheduled
for deprecation on 2026-07-24 15:59 UTC.

The API contract above was rechecked against the official DeepSeek and
Infisical documentation on 2026-07-04 before this context block was refreshed.

## Failure Rules

DeepSeek integrations must fail closed when:

- Infisical CLI is unavailable or unauthenticated.
- Infisical direct read fails, injection fails, or direct/injected hashes differ.
- `GAMEEDITOR_DEEPSEEK_API_KEY` is missing or empty.
- DeepSeek returns non-2xx, non-JSON, missing `choices[0].message.content`,
  missing `id`, missing `model`, or missing positive token usage.
- A verification report cannot be written and read back from disk.

Log provider, endpoint, model, status code, request id, response id, sanitized
error body, secret name, Infisical project id, environment, and path. Never log
the authorization header or raw key.

Provider verification is separate from secret-manager verification. Infisical
passing proves the project has a persisted secret and that injection works; it
does not prove DeepSeek accepted the key. The latest provider verification now
passes. A future DeepSeek HTTP 401 must fail closed and be tracked with
sanitized evidence until a provider-valid key is explicitly rotated into the
same Infisical source of truth.

## References

- DeepSeek API quickstart: https://api-docs.deepseek.com/
- DeepSeek chat-completions reference:
  https://api-docs.deepseek.com/api/create-chat-completion
- DeepSeek authentication reference:
  https://api-docs.deepseek.com/api/deepseek-api
- DeepSeek error codes:
  https://api-docs.deepseek.com/quick_start/error_codes
- Infisical CLI quickstart: https://infisical.com/docs/cli/usage
- Infisical secrets command reference:
  https://infisical.com/docs/cli/commands/secrets
- Infisical run command reference:
  https://infisical.com/docs/cli/commands/run
- OWASP Secrets Management Cheat Sheet:
  https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html
