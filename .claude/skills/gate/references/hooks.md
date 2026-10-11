# Hook wiring, pre-push steps, ratchet mechanics

## Installation (tracked hooksPath)

Git does not clone hooks. `scripts/install-git-hooks.ps1` runs `git config core.hooksPath
.githooks` and reads it back. `.githooks/` is tracked and contains: `pre-commit`, `pre-push`,
`post-checkout`, `post-commit`, `post-merge`. Run the installer once per clone.

The hooks are `#!/bin/sh` and prefer `pwsh`, falling back to Windows `powershell` (5.1) -- which
is exactly why gate scripts must be ASCII-only (no BOM + ANSI decode = parse error).

## pre-commit

Runs `check-format.ps1 -FormatOnly`: rustfmt (GameEditor ws + each Calyx package) + ratchets
4-10 (line-count, temp-dir, doctrine-refs, doc-refs, reference-artifacts, ascii, fsv-idioms).
NO cargo check, NO side-store gate -- kept to a couple of seconds. Fix formatting with
`pwsh -File scripts/check-format.ps1 -Write`.

## pre-push (pre-push-gate.ps1)

Publish boundary where correctness is proven. Steps, each failing loud with a reproduce command:
1. `cargo check --workspace --all-targets` (compile the -FormatOnly path skipped)
2. `scripts/check-examples.ps1` -- the ~230 FSV example binaries (#1527)
3. `scripts/test-gameeditor.ps1 -Package <pkg>` for every workspace package
4. `scripts/lint.ps1 -Deny` -- clippy `-D warnings` + cargo-deny, over GameEditor AND Calyx

Steps 1-3 build into the STABLE warm cache `target\fsv\test-gameeditor\cargo-target`, so a warm
push is incremental. This gate NEVER wipes that cache. Bypass: `git push --no-verify` (visible,
by design #1553).

`lint.ps1` also runs `check-env-vars.ps1` first (every prod env var read in `crates/*/src` must
be documented in `docs/env-vars.md`).

## Line-count ratchet (check-line-count.ps1)

A committed baseline `scripts/line-count-baseline.tsv` lists every .rs file already >=500 lines
with its count. The gate enforces the violation set only ever DECREASES:
- a non-baselined file reaching >=500 -> FAIL (NEW)
- a baselined file growing beyond its recorded count -> FAIL (GREW)
- a baselined file now under 500 -> FAIL (LOCK-IN): remove its row via `-Update` to lock the win
- a baseline row whose file is gone -> FAIL (STALE): `-Update`

`-Update` rewrites the baseline from the current tree. Run it only after a legitimate split;
review the diff before committing. Scans tracked `.rs` under `crates\` (excludes `target\`).

## Side-store eradication (check-calyx-side-store-eradication.ps1)

Scans prod `crates/*/src` (excludes examples/tests). Flags: `source_of_truth` naming
JSON/RON/SQLite/local store instead of Calyx Aster; direct `fs::write`/`File::create`/
`OpenOptions` in a structured-store file not on the allowed-write list; RPG content factories
with compiled `rpg://` literals, `AnchorKind::TestPass`, or missing the shared authored-content
validator. The permitted authoritative structured source of truth is the Calyx Aster vault.
