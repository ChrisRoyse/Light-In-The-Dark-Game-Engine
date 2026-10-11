# Log-line verdicts - the file log is the truth channel

`litd-editor.exe` / `litd-game.exe` intentionally log via `litd_engine::init_file_logging`
to a FILE, not the console. stdout/stderr are normally EMPTY. Do not read an empty console
as "silent / no logging" (that mistake mis-filed #1531).

## Where the log is

- Default: `%LOCALAPPDATA%\LitD\logs\<app>.log` (`litd-editor.log`, `litd-game.log`).
- Override: `LITD_LOG_FILE`. No CWD fallback since #1492.
- Filter: `RUST_LOG` / `EnvFilter`. Default `warn,litd_engine=info,litd_editor=info,litd_game=info`.
  Set `$env:RUST_LOG="debug"` (or a per-target filter) if your verdict line is at debug.

## Lines worth asserting on

- `litd shared gpu context initialized` - adapter/device came up.
- `editor process returning exit_code=... last_exit_reason=...` - the authoritative exit.
- `exit_reason="close_requested" clean_shutdown=true exit_code="success"` - a clean
  `taskkill //PID` (WM_CLOSE) shutdown.
- `renderer_init_failed` + `exit_code="failure"` - the real forced-failure signal (a bad
  `LITD_EDITOR_PROJECT_ROOT` does NOT fail boot - it is only the MCP path-confinement root
  #1518, the editor tolerates it; do not use it as a forced-failure).
- `panicked` - panic hook wrote the exact cause here.

## The emit-a-distinctive-line pattern

The cheapest reality probe for a new feature: make the code emit a unique structured line,
then assert on it with the actual value ("the error is the prompt", #1166 Loop 1):

```rust
tracing::info!(target: "fsv", issueNNNN_applied = true, actual = ?computed);
```

```powershell
Select-String -Path "$env:LOCALAPPDATA\LitD\logs\litd-editor.log" -Pattern 'issueNNNN_applied' |
  Select-Object -Last 1
```

Follow live while iterating: `Get-Content $log -Tail 60 -Wait`.
