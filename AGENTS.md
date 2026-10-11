# AGENTS.md

## First Rules

- State lives in GitHub issues, not Codex goal state. Start with #780, follow
  #800, and keep the active issue updated.
- Treat Codex goal objective/resume state as context only. Do not call Codex
  goal tools (`get_goal`, `create_goal`, `update_goal`) for this repo unless
  the operator explicitly asks for that Codex feature. The raw
  `{"detail":"Bad Request"}` interruption is tracked in #1268.
- If that banner appears, do bounded read-only diagnostics against
  `C:\Users\hotra\.codex\logs_2.sqlite`, `goals_1.sqlite`, and current Codex
  feature/config state, then update #1268 and continue through GitHub issue
  state. Do not directly edit Codex runtime SQLite.
- Never terminate, restart, close, or otherwise control user programs,
  terminal tabs, VS Code sessions, browsers, other Codex sessions, or unrelated
  processes while handling #1268 unless the operator gives that explicit
  process-control instruction in the current conversation. Record suspected
  stale process state as evidence only.
- Keep diagnostics bounded: exact literals, targeted row IDs, count-first
  queries, short substrings, and limited output. Avoid broad recursive scans or
  giant diffs that can create large Codex protocol frames.

## Verification

- Full State Verification must be manual against reality via Synapse/computer
  control. Tests, logs, return values, generated screenshots, and scripts are
  supporting evidence only.
- For editor/window/visual work, bind to the exact live HWND, capture that
  target, manually inspect the saved bitmap, and separately read stats/logs and
  any persisted runtime state that can affect the UI.
- Review relevant `docs/readme/*.png` references before visual/editor/gameplay
  work; those images are the quality and layout floor.

## Agent worktree survival

- **Your `.claude/worktrees/*` isolation worktree is EPHEMERAL and can be
  destroyed out from under you at any time (#1716, class of #1569).** The Claude
  Code harness removes an isolation worktree it created when the worktree carries
  NO committed changes, and force-cleans some worktrees on session end. Critically,
  the auto-clean heuristic does NOT count untracked/uncommitted files as "changed":
  a worktree full of authored-but-uncommitted work looks "unchanged" and is swept,
  taking that work with it. Three agents lost ~3 agent-hours of test-green content
  this way (#1716). You cannot patch the harness; you survive it by committing.
- **Commit + push to origin EARLY and after EVERY completed slice — before any
  slow step (tests, `cargo fmt`, a long build).** Push-first durability is the only
  guarantee your work survives worktree loss: a pushed commit lives on origin, an
  uncommitted file lives only in an ephemeral directory. Never test-then-commit for
  large authored work; commit-then-test, or at minimum commit before the first slow
  step. Never leave authored work uncommitted across a stall or a "done" hand-off.
- **Treat the worktree PATH as disposable.** Do not record absolute
  `.claude/worktrees/<id>/...` paths as durable artifact locations in issues; the
  directory may be gone before anyone reads them. Cite committed git paths / commit
  SHAs / pushed-branch names instead.
- For long authoring runs, prefer a **persistent sibling worktree** outside
  `.claude/worktrees/` (e.g. `git worktree add C:\code\GameEditor-<batch>
  origin/main`) which the harness does not auto-clean, over an isolation worktree.
- Pre-cleanup audit tripwire: `scripts/check-worktree-uncommitted.ps1` lists any
  `.claude/worktrees/*` holding uncommitted (tracked-modified OR untracked) work and
  exits non-zero naming each dirty worktree + its file count. Run it before any bulk
  worktree cleanup so at-risk work is committed first, never swept.

## Engineering

- No mocks, no silent fallbacks, no workaround behavior. Fail loud with enough
  structured logging to identify the failing source of truth.
- Keep the worktree clean. Commit and push verified slices, then update the
  related issue with evidence paths, hashes, and source-of-truth readback.
- Keep `target/` clean, but distinguish two kinds of build output — the rule is
  NOT "delete every target tree after every run":
  - **Ephemeral per-attempt scratch** — one-off isolated `CARGO_TARGET_DIR`
    trees for a single check, `target/codex-*`, `target/verification-logs/*`,
    and other generated artifacts tied to ONE run. Clean these up after the
    needed readback is captured and issue state is updated; if evidence must
    persist, record the hashes/actuals in GitHub issues or committed docs first.
  - **The persistent shared gate build cache** — the STABLE `cargo-target` trees
    the test gate reuses (`target\fsv\test-calyx\cargo-target`,
    `target\fsv\test-gameeditor\cargo-target`, and any `…\cargo-target` a gate is
    invoked with). These are a WARM incremental cache, not scratch: their entire
    value is being reused across runs so cargo fingerprint-skips the expensive
    95% of the graph (wgpu, naga, rapier, 40+ Calyx crates at O3) instead of a
    full cold rebuild every run. DO NOT wipe them after a run — that is exactly
    the #1498 regression that made every gate cold. `test-rust-package.ps1`
    retains its target tree on exit and only records its status; disk is bounded
    out-of-band by the scheduled GC (below), which is the correct layer.
  Before any recursive target cleanup, resolve absolute paths and prove they are
  inside this repo's `target` directory; do not delete target paths currently
  used by live processes, and never add per-run deletion of a `cargo-target`
  gate cache.
- **NEVER `robocopy /MIR` (or recursively purge) a git worktree or any tree that
  may contain directory JUNCTIONS (#1569, reparse-point class of #1518).**
  `robocopy /MIR` follows directory junctions and purges THROUGH them by default
  (no `/XJ`), so mirroring an empty dir over a worktree deletes the junction
  TARGET. This is exactly how the gitignored `local-asset-vault` was emptied to 0
  files: a worktree held a junction into the real vault, and a long-path-delete
  workaround mirrored empty over it - wiping the vault for every checkout. SAFE
  worktree/junction deletion: unlink the junction LINKS first
  (`[System.IO.Directory]::Delete($junction, $false)` no-recurse, or
  `cmd /c rmdir <junction>` WITHOUT `/S`), THEN delete the tree. As a tripwire,
  capture `(Get-ChildItem local-asset-vault | Measure-Object).Count` before and
  after ANY worktree cleanup and confirm it is unchanged; a drop to 0 means a
  junction was purged through. `local-asset-vault` is gitignored and NOT
  recoverable from git - re-materialize per
  `docs/assets/asset-vault-runbook.md`.
- Canonical scratch locations: agent-owned build/verification scratch goes under
  `target\fsv\<issue-slug>\` (bounded per-issue by the scheduled GC) or the repo
  `tmp\` dir — never loose in `target\` root. Do not invent per-attempt siblings
  (`tmp_issueNNNN_converted`, `...-v2`, `...-v3`); reuse one path. Isolated
  `CARGO_TARGET_DIR` trees for one-off checks must be removed after readback — a
  workspace build uses the ONE shared workspace target.
- Backstop (not a substitute for cleanup): `scripts/prune-build-artifacts.ps1`
  runs daily via the `LITD-GameEditor-BuildArtifactGC` scheduled task and bounds
  superseded source trees, FSV runs (newest N per issue, manifest-protected),
  loose isolated build trees in `target\` root, aged `%TEMP%` test scratch,
  abandoned persistent gate caches (`…\cargo-target` untouched for `-GateCacheMaxAgeDays`,
  default 14; an active gate touches its cache every run so it never ages out, and
  an in-use cache is never evicted), and — the one Cargo NEVER bounds itself — the
  per-crate **incremental compilation cache** (`target\debug\incremental`), hard-capped
  at `-IncrementalMaxSizeGB` (default 10) and removed wholesale above it. This last cap
  is unconditional, NOT gated on `cargo-sweep` being installed: it grew to **58 GB /
  97k files** unnoticed precisely because the only prior bound was the optional
  `cargo-sweep --maxsize` pass, which silently no-ops when the tool is absent (#1530).
  An oversized incremental cache is not just disk — on Windows it slows fingerprinting,
  linking, and AV scanning on *every* build (rust-lang/rust#137560), so bounding it
  protects agent feedback-loop speed. It also prunes **merged agent isolation worktrees**
  under `.claude/worktrees/` (#1707) — each carries its own ~276k-file `target/`, the
  repo-wide search-timeout driver — but ONLY ones that are unlocked AND clean (no
  uncommitted/untracked work, per the #1716 guard) AND merged into `origin/main` AND aged;
  removal is junction-safe (top-level junctions unlinked first, never `robocopy /MIR`
  through them) with a `local-asset-vault` count tripwire. This is the layer that bounds the warm gate
  caches AND the incremental cache — the gate itself must NOT wipe them per-run.
  Install/inspect with `scripts/install-build-artifact-gc-task.ps1`; run on demand with
  `pwsh -File scripts/prune-build-artifacts.ps1` (add `-DryRun` to preview).
- **Fast agent feedback loops are load-bearing.** A slow `cargo test -p <crate>` edit→result
  cycle compounds across every agent on every task. Never let regenerable build output
  accumulate unbounded; keep the daily GC installed and the incremental cache capped. If a
  build feels slow, first check `target\debug\incremental` size before assuming a compiler
  problem — a bloated cache is the usual cause on this host.
- **Do NOT disable incremental compilation** (`CARGO_INCREMENTAL=0` / `incremental = false`).
  Measured on this host (#1530): a one-line change to `litd-editor --lib` rebuilds in **20 s
  with incremental vs 93 s without** — a 4.6× penalty, because `litd-editor` is one giant crate
  whose full recompile dwarfs any Windows incremental-copy overhead (#137560). Incremental is
  the single biggest lever on edit→result latency and MUST stay on; the correct fix for its
  cache growth is the size cap + daily GC above, NOT switching it off. The deeper structural win
  is splitting the `litd-editor` mega-crate so a change recompiles less of it (tracked with the
  #1466 modularization debt) — until then, incremental is what keeps the loop fast.
- **Parallel-agent disk admission: gate heavy builds on free space; NEVER share a build cache (#1819).**
  Each `.claude/worktrees/*` worktree builds its OWN `target/` (3-4 GB), and litd-engine's graph
  pulls `candle-transformers` + `wasmtime`/`cranelift` (via `calyx-registry`, non-optional), so
  every test/example BINARY needs multi-GB of LLVM codegen. When several worktrees codegen the
  same crates at once the C: volume hits 100% and a build dies with `no space on device`
  MID-COMPILE, corrupting a sibling's incremental cache and blocking FSV. There is no SAFE shared
  build cache for divergent worktrees today, so the durable control is disk ADMISSION, not sharing:
  - Before a heavy build in a worktree, dot-source `. scripts/agent-build-env.ps1`. It reads the
    real free space (`Get-PSDrive`, source of truth), optionally reclaims via the sanctioned GC
    (`-ReclaimIfLow` -> `scripts/prune-build-artifacts.ps1`), then either arms the shell or REFUSES
    LOUDLY with `AGENT_BUILD_DISK_ADMISSION_DENIED` (naming free/floor GB and the reclaim command)
    so the agent waits/reclaims instead of ENOSPC-ing mid-codegen. No silent fallback. Disk is
    bounded from the other side by the daily/back-stop `prune-build-artifacts.ps1` (aged temp trees,
    merged-worktree targets, the incremental-cache cap).
  - **Do NOT set `CARGO_TARGET_DIR` to one shared workspace `target/`.** Cargo's per-package
    metadata hash omits the worktree PATH, so two worktrees collide on one fingerprint slot for
    each workspace/path crate and one SILENTLY reuses (and runs) the other's compiled code with no
    error -- a false-green in an FSV-gated repo (rust-lang/cargo#16642, lx-industries/openblob#522).
    A shared target also serializes every build behind one `.cargo-lock` (rust-lang/cargo#16804),
    and this repo's own gate already rejects a cross-checkout target with
    `TEST_GATE_TARGET_OUTSIDE_REPO` (#1520/#1567/#1590).
  - **sccache does NOT help here (measured, #1819 FSV).** It gives ZERO cache hits across
    worktrees -- Cargo passes per-worktree absolute `--out-dir`/`-L dependency=`/`--extern` paths
    that sccache folds into its cache key, so two worktree paths never share a key (its real value
    is same-path CI rebuilds). Do not reach for it to solve cross-worktree recompiles.
  - Disk-saving idiom for READ-ONLY checks only: a `cargo check` in a worktree with no warm target
    can build into the MAIN checkout's warm `target/` via `CARGO_TARGET_DIR` to avoid a fresh
    multi-GB tree -- SAFE only for check of origin/main-identical source, NEVER to build/run
    divergent worktree code (the cross-contamination above). The structural fix (an out-of-tree
    content-addressed build dir) is rust-lang/cargo#16147/#5931 and not yet stable.
- Use `scripts/check-format.ps1` as the repo format gate on Windows. Do not use
  root `cargo fmt --all -- --check` as the gate; it includes local Calyx path
  dependencies in one oversized rustfmt invocation and can fail with Windows
  `os error 206` before formatting is checked. Pass `-FormatOnly` for the fast
  rustfmt-only path (no `cargo check --all-targets`, no side-store gate) used by
  the pre-commit hook; `-Write` to apply `cargo fmt` across both workspaces.
- The format gate is enforced at commit time by a tracked pre-commit hook
  (`.githooks/pre-commit`, #1528). There is no CI, so this hook is what keeps
  formatting drift from silently accumulating (it went red on main once because
  nothing ran the gate before commits). Activate it once per clone with
  `powershell -File scripts/install-git-hooks.ps1` (sets `core.hooksPath
  .githooks`). The hook runs `check-format.ps1 -FormatOnly` and blocks any commit
  that is not rustfmt-clean; fix with `check-format.ps1 -Write` then re-commit.
- **`check-format.ps1` also runs the `<500`-line ratchet** (`scripts/check-line-count.ps1`,
  #1466/#800 §6) on every invocation, including the fast `-FormatOnly` pre-commit path. Cargo
  cannot enforce a per-file line budget, so files silently crept over 500 (audit: 33; scan: 44).
  The ratchet (best-practice pattern, imbue-ai/ratchets, Notion) keeps a committed baseline
  (`scripts/line-count-baseline.tsv`) of the current violators and enforces the set only
  DECREASES: a NEW non-baselined `.rs` file at `>=500` lines, or GROWTH of a baselined one, FAILS
  the gate. Write modular (`<500`-line) files from the start. When you legitimately split a
  baselined file below 500, run `pwsh -File scripts/check-line-count.ps1 -Update` to drop its row
  and lock the win in (it can then never regress). Goal: shrink the baseline to empty.
- Use `scripts/lint.ps1` as the local clippy + cargo-deny gate (both the
  GameEditor and Calyx workspaces). `cargo-deny check` enforces the license
  policy (#1073) via `deny.toml`; it requires `cargo install --locked cargo-deny`
  (the script fails loud if it is missing). Pass `-Deny` to fail on clippy
  warnings (`-D warnings`). As of #1545 the backlog is cleared and `-Deny` is
  GREEN for both workspaces, so it is now an ENFORCED gate wired into the
  **`.githooks/pre-push`** hook (installed by `scripts/install-git-hooks.ps1`).
  It runs at pre-push, NOT pre-commit as this file previously anticipated:
  `-Deny` compiles both workspaces `--all-targets` (~46s warm no-op, 1-2+ min
  after edits), which is far too slow to run on every commit without everyone
  reflexively reaching for `--no-verify`; pushing is infrequent enough that the
  cost amortizes while still blocking unclean code from reaching the remote
  (#1545). The fast pre-commit hook stays rustfmt-only.
- Runtime environment variables are documented in `docs/env-vars.md`
  (name → reader file → effect → default). This is enforced: `scripts/lint.ps1`
  runs `scripts/check-env-vars.ps1`, which fails loud if any production env var
  read in `crates/*/src` (literal `env::var` **or** a `*_ENV: &str` constant) is
  missing from the doc. A var read only from a `*_tests.rs` file is treated as
  test-only and needs no entry.
- ONNX Runtime is a hard dependency for editor tests that load the real Calyx
  content panel. Before a direct `cargo test -p litd-editor --lib`, dot-source
  `scripts/Set-GameEditorCalyxRuntimeEnv.ps1`; `scripts/test-gameeditor.ps1`
  performs the same fail-loud bootstrap automatically. The bootstrap accepts
  only the canonical Calyx root and sets the exact process-scoped
  `ORT_DYLIB_PATH`/`CALYX_ORT_CAPI`; do not use a system or alternate fallback.
- **GPU/CUDA kernel gate (#1480 item 3).** Default `cargo` builds compile NO CUDA
  kernels — `calyx-forge` prints `cuda feature not enabled, skipping kernel
  compilation`, so the routine gate exercises only the CPU path. The GPU kernel
  path (sm_120, validated in closed #694) is behind the `cuda` feature
  (`litd-engine/Cargo.toml: cuda = ["calyx-forge/cuda"]`). Policy: the routine
  per-edit gate stays CPU-only (fast, no toolkit); a **CUDA regression cannot be
  caught by default builds**, so validating a change that touches Forge kernels or
  the GPU path REQUIRES an explicit `cargo test -p litd-engine --features cuda` run
  on a CUDA-capable host (RTX 5090 target) — mandatory before a release and after
  any Forge/kernel edit. Do not assume a green default gate covers the GPU path.

## AI Provider Secrets

- Future agents should read `docs/deepseek-infisical-context.md` before adding,
  testing, or debugging DeepSeek behavior.
- Minimum future-agent contract: normal build, format, test, and issue work must
  not run Infisical login, Infisical secret verification, DeepSeek provider
  verification, or key rotation. Only run the DeepSeek/Infisical wrappers when
  the active task is explicitly about DeepSeek, Infisical, provider acceptance,
  or operator-requested key rotation. Do not ask the operator for the key again.
- Do not add DeepSeek/Infisical audit gates, secret-manager checks, provider
  probes, or wrapper-discovery tests to `scripts/check-format.ps1`, cargo test
  suites, CI, or unrelated issue verification. Routine gates must stay
  completely local to the code they are validating and must not mention or
  exercise the DeepSeek/Infisical workflow.
- Keep this section as policy doctrine; put refreshed source-of-truth evidence
  in `docs/deepseek-infisical-context.md` and #1343, not in duplicated notes.
- When refreshing DeepSeek evidence, update only
  `docs/deepseek-infisical-context.md` and #1343 with sanitized report paths
  and hashes. Do not duplicate or preserve raw key material anywhere.
- GameEditor DeepSeek access is managed only through Infisical, never through
  repo files, issue comments, logs, screenshots, shell history, or Codex memory.
- The key has already been settled in Infisical. Do not go back to DeepSeek
  provider/API-key pages to find, copy, compare, or replace the key unless the
  operator explicitly asks for provider-side rotation in the current
  conversation.
- The only valid future-agent workflow for explicit DeepSeek/Infisical work is:
  verify CLI auth first, verify Infisical source-of-truth state only when the
  task requires it, run DeepSeek through Infisical environment injection only
  when provider behavior must be proven, and update #1343 with sanitized report
  paths. Do not ask the operator for the key again or create a second secret
  path/name. Routine test suites must never depend on this workflow.
- Infisical is the authoritative source of truth for the GameEditor DeepSeek
  key. If local state, docs, shell env, or provider UI disagree, trust
  Infisical only and update the active GitHub issue with sanitized evidence.
- The operator-provided DeepSeek key was rotated through Infisical on
  2026-07-03. Later explicit operator refreshes on 2026-07-04 were written
  through the named-pipe wrapper, matched the stored Infisical hash, and
  passed real DeepSeek provider verification. The latest sanitized evidence is
  documented in `docs/deepseek-infisical-context.md`. Infisical
  storage/injection and real DeepSeek provider acceptance are settled by
  sanitized readback; #1343 is historical unless a future provider regression
  appears.
- Secret manager source of truth: Infisical project `leapable-aiwonder-prod`,
  project id `c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5`, environment `prod`,
  secret path `/gameeditor`.
- The GameEditor DeepSeek API key secret name is
  `GAMEEDITOR_DEEPSEEK_API_KEY`. Agents may document and reference this name,
  but must never print or persist the value.
- Never pass the raw key as a shell argument, write it into a temporary file,
  paste it into issue text, or capture it in screenshots. Use Infisical CLI
  operations and the repo verification wrappers only.
- If the operator explicitly requests provider-side rotation later, still do
  not put the raw key in command-line arguments, shell history, screenshots,
  repo files, or persisted temp files. Put the value in a process environment
  variable for that owned run and use `scripts/set-infisical-secret-from-env.ps1`,
  which writes through a named pipe and persists only a sanitized report.
  Do not call raw `infisical secrets set KEY=value`; this Infisical CLI version
  can echo the secret value in set output. Immediately verify with the repo
  wrappers and update #1343 or the active issue with sanitized readback only.
- To run a command with the key, use Infisical injection from **PowerShell**:
  `infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- <command>`.
- Do NOT run that raw wrapper from Git Bash / MSYS2 (the Bash tool): MSYS
  path-translation rewrites `--path=/gameeditor` into
  `C:/Program Files/Git/gameeditor`, and the server returns a misleading
  `[status-code=400] [message="Invalid secret path..."]` plus a service-token
  hint. That is NOT an auth error and the key is NOT the problem; do NOT rotate
  or open provider key pages over it (see #1546 and the "Shell Requirement"
  section of `docs/deepseek-infisical-context.md`). From Git Bash, prefix with
  `MSYS_NO_PATHCONV=1` or use `scripts/infisical-run.sh`. The repo wrappers fail
  closed with the greppable code `INFISICAL_PATH_MANGLED` if handed a mangled path.
- To verify the key without leaking it, use the repo wrapper:
  `scripts/verify-infisical-secret.ps1`. This wrapper is manual-only and must
  not be wired into normal tests, format gates, CI, or unrelated issue work. It
  prints/writes only sanitized presence, length, and hash readbacks and fails
  closed on unauthenticated CLI state, missing/empty secrets, wrong paths, CLI
  errors, or injection mismatch.
- To verify the provider path without leaking the key, use
  `scripts/verify-deepseek-api.ps1`. This wrapper is manual-only and must not be
  wired into normal tests, format gates, CI, or unrelated issue work. It injects
  the key through Infisical, sends a minimal real chat-completions request,
  validates the response schema and exact sentinel output, writes a sanitized
  report, and reads that report back from disk.
- To rotate the key after explicit operator instruction, use
  `scripts/set-infisical-secret-from-env.ps1` with the new value supplied by an
  owned process environment variable. The script fails closed on invalid
  inputs, command-line leakage, CLI failure, or report leak guard failure and
  records whether Infisical CLI output required sanitization.
- Check `docs/deepseek-infisical-context.md` and #1343 before assuming the
  provider accepts the stored key. A passing Infisical verification proves
  secret-manager state only; provider acceptance is a separate API check.
- To add DeepSeek to code, read only `GAMEEDITOR_DEEPSEEK_API_KEY` from the
  process environment after Infisical injection. Do not add `.env` loading,
  provider fallbacks, hardcoded key names beyond this env var, or alternate key
  discovery paths.
- When implementing a DeepSeek client, make the env var absence, empty value,
  HTTP non-2xx, invalid JSON, missing response id/model/content/usage, and
  sentinel mismatch explicit errors. Persist sanitized provider/request
  diagnostics where the caller can read them back; never degrade to local text,
  another provider, a stub response, or a legacy model id.
- DeepSeek API calls use bearer authentication from
  `GAMEEDITOR_DEEPSEEK_API_KEY`, the OpenAI-compatible chat-completions API at
  `https://api.deepseek.com`, and `/chat/completions`.
- Use current DeepSeek model ids `deepseek-v4-flash` or `deepseek-v4-pro`.
  Legacy `deepseek-chat` and `deepseek-reasoner` are compatibility aliases
  scheduled for deprecation on 2026-07-24 15:59 UTC; do not add new GameEditor
  integrations that depend on those legacy names.
- Code that consumes DeepSeek must fail closed if `GAMEEDITOR_DEEPSEEK_API_KEY`
  is unset or empty, if Infisical injection fails, if the API returns non-2xx,
  or if the response schema is missing expected fields. Log provider, endpoint,
  model, status code, request id, and sanitized error bodies, but never log the
  authorization header or raw key.
- If DeepSeek returns 401 in the future, treat it as provider key validity or
  account state, not as a reason to search for keys elsewhere. Update #1343 or
  the active issue with sanitized verification evidence and wait for an
  explicit operator rotation instruction.
- Do not add fallback model providers, fake responses, mock API keys, or tests
  that pass without the real Infisical-injected key when the behavior under test
  requires DeepSeek.
