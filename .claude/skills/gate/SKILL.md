---
name: gate
description: Pass the GameEditor local commit/push gate on the first try. Use before git commit or git push, or when a commit/push is blocked by FORMAT_GATE_FAILED, LINE_COUNT_GATE_FAILED, ASCII_GATE_FAILED, DOC_REFS_GATE_FAILED, FSV_IDIOM_GATE_FAILED, CALYX_SIDE_STORE, or PRE_PUSH_GATE_FAILED. Enumerates every check-format.ps1 sub-check, the <500-line ratchet, and the exact repair for each failure class.
---

# Local commit/push gate (GameEditor)

There is NO CI (#284/#447/#689); tracked git hooks are the ONLY enforcement. Install once per
clone (also see references/hooks.md):

```powershell
pwsh -File scripts/install-git-hooks.ps1     # sets core.hooksPath .githooks (tracked)
```

- pre-commit -> `check-format.ps1 -FormatOnly` (rustfmt + fast ratchets, NO compile). Seconds.
- pre-push  -> `pre-push-gate.ps1` (cargo check + examples + test gate + lint). Minutes.

Run the full pre-commit checks yourself before committing:

```powershell
pwsh -File scripts/check-format.ps1          # full: adds cargo check + side-store gate
```

## What check-format.ps1 runs

Steps 1-2 (rustfmt) and 4-10 (ratchets) run on EVERY invocation including the `-FormatOnly`
pre-commit path. Steps 3 and 11 run ONLY in full mode (skipped by `-FormatOnly`).

| # | Step | What it enforces |
|---|------|------------------|
| 1 | `cargo fmt -p <pkg> -- --check` (each GameEditor pkg) | rustfmt-clean (per-package; #1720 avoids the deep-worktree os error 206) |
| 2 | `cargo fmt -p <pkg> -- --check` (each Calyx pkg) | rustfmt-clean |
| 3 | `cargo check --workspace --all-targets` | compiles (full mode only; via retry helper) |
| 4 | line-count-ratchet | no .rs file >=500 lines (#1466/#800 sec.6) |
| 5 | temp-dir-hygiene-ratchet | no raw `std::env::temp_dir()` helpers (#1535) |
| 6 | doctrine-refs-integrity | issue-body-cited files are git-tracked (#1465) |
| 7 | doc-refs | docs2/docs/prompts paths in tracked .md are git-tracked |
| 8 | reference-artifact-doctrine | docs PNGs carry a doctrine verdict (#1510) |
| 9 | ascii-gate-scripts | scripts/*.ps1 + .githooks/* are ASCII-only (#1570) |
| 10 | fsv-idioms | no blind-count / exit-code-corrupt idioms (#1556) |
| 11 | calyx-side-store-eradication | prod stores use Calyx Aster, not JSON/fs (full only) |

## FAILURE -> FIX

| Message | Fix |
|---------|-----|
| `FORMAT_GATE_FAILED: step=gameeditor-package:*` / `step=calyx-package:*` | `pwsh -File scripts/check-format.ps1 -Write` (runs cargo fmt), then re-stage |
| `FORMAT_GATE_FAILED: step=gameeditor-all-targets` with `error[...]`/`could not compile` | real compile error -- fix the code shown in diagnostics |
| same step, exit!=0 but NO diagnostics | transient target-lock; auto-retried. If it persists, a hung cargo/rustc holds `target\.cargo-lock` -- kill it, re-run |
| `LINE_COUNT_GATE_FAILED` -- `NEW` | file reached >=500 lines: split into cohesive submodules (keep tests in a `_tests.rs`). Do NOT line-shave to 499 |
| `LINE_COUNT_GATE_FAILED` -- `GREW` | you enlarged a baselined file: split it back down or move new code to a new <500 module |
| `LINE_COUNT_GATE_FAILED` -- `LOCK-IN` / `STALE` | you split below 500 (or a row is stale): `pwsh -File scripts/check-line-count.ps1 -Update`, review the TSV diff, commit it |
| `temp-dir-hygiene` fail | use `litd_engine::test_temp::TestTempDir` instead of raw `std::env::temp_dir()`; then `pwsh -File scripts/check-temp-dir-hygiene.ps1 -Update` |
| `DOC_REFS_GATE_FAILED` / doctrine-refs | commit the referenced file (on-disk existence does NOT count -- being git-tracked does), fix the reference, or allowlist in `scripts/doc-refs-allowlist.txt` |
| `ASCII_GATE_FAILED` | replace the flagged non-ASCII char: em-dash -> `-`/`--`, smart quotes -> `'`/`"`, arrow -> `->`, ellipsis -> `...`, section -> `sec.` |
| `FSV_IDIOM_GATE_FAILED` -- `BLIND_COUNT` | replace `Measure-Object -Line` (skips blank lines) with `@(...).Count` or `wc -l` | <!-- fsv-idioms:allow (teaching the trap) -->
| `FSV_IDIOM_GATE_FAILED` -- `EXITCODE_CORRUPT` | capture first: `$out = & cmd 2>&1 \| Out-String; $code = $LASTEXITCODE`. If the `Select-Object -First` is on in-memory data, add `# fsv-idioms:allow` on that line |
| `CALYX_SIDE_STORE_VIOLATION` | route prod structured data through Calyx Aster; do not `fs::write`/`File::create` a JSON/RON/SQLite store or name one as `source_of_truth` |
| `PRE_PUSH_GATE_FAILED: step=*` | run the exact reproduce command the gate prints (cargo check / `scripts/check-examples.ps1` / `scripts/test-gameeditor.ps1 -Package <p>` / `scripts/lint.ps1 -Deny`) |

## Non-negotiables

- Gate scripts must be ASCII-only. The pre-commit hook falls back from pwsh 7 to Windows
  PowerShell 5.1, which decodes these no-BOM files as ANSI and turns any non-ASCII byte into a
  MISLEADING parse error. Applies to your new/edited scripts too.
- Write .rs files modular from day one; do not grow the 44 legacy line-count violators.
- `git push --no-verify` bypasses pre-push. It is a deliberate, VISIBLE bypass -- use only when
  you know why; a scheduled main-health run will detect a bad main (#1567).

## References
- references/hooks.md -- hook wiring, pre-push steps, ratchet/baseline mechanics
