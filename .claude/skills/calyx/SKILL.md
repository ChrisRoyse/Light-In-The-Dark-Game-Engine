---
name: calyx
description: Work on or with Calyx (the vendored AI substrate) correctly the first time. Use when editing any Calyx/ crate, ingesting engine structured data, adding lenses/panels, or when tests fail with CALYX_LENS_UNREACHABLE. Covers the blueprint doctrine, the atoms->associations->bits->kernel->compose build order, embed-vs-encode, single-source-of-truth (no side stores), Aster vault paths, and the ONNX runtime env.
---

# Calyx

Calyx is GameEditor's vendored, association-native AI database at `C:\code\GameEditor\Calyx`.
`docs2/BUILDING_ON_CALYX.md` is the BINDING handbook; read it before non-trivial Calyx work.
This skill is the fast on-ramp. Depth: `references/method.md`.

## 1. Calyx is a BLUEPRINT (edit it freely)

`Calyx/` is NOT a pristine snapshot to keep in sync with upstream. It is this app's own
copy, meant to be fully customized in place. Doctrine (`docs2/decisions/DEC-1519-calyx-is-a-blueprint.md`):

- Edit any Calyx crate freely when the app needs it. There is NO "preserve on re-vendor"
  or "clean snapshot" constraint anymore.
- Fix problems at the RIGHT layer -- often that is inside Calyx itself, not a workaround above it.
- Still binding: keep Calyx advisory/license clean -- the `#1517` cargo-deny gate
  (`scripts/calyx-deny.toml` + `scripts/lint.ps1`) still applies.
- `Calyx/` is its own cargo workspace; root `Cargo.toml` does `exclude = ["Calyx"]`. The
  editor/engine build against it via path-deps. It has no own `.git`.
- Calyx-internal `#NNNN` refs are Calyx-Dev's OWN tracker, NOT this repo's GitHub issues.

## 2. The build order (BINDING, from BUILDING_ON_CALYX.md)

The four verbs and the one method. Never shortcut the decomposition:

**decompose to atoms -> weave ALL base associations -> differentiate (bits) -> distill the kernel -> compose.**

- **Measure** -- view one input through a panel of frozen **lenses** -> a **constellation**
  (many typed **slots**, kept separate, never flattened).
- **Count** -- derive associations BETWEEN slots (cross-terms) and between records (kNN graph).
  N lenses over n inputs yield `n*(N + C(N,2) + 1)` signals (Derived Data Abundance).
- **Differentiate** -- measure the unique **bits** each lens/association adds about a grounded
  **anchor** (KSG mutual information). Admission contract: >=0.05 bits, <=0.6 pairwise
  correlation, >=50 samples. Sufficiency gate: `I(panel;anchor) >= H(anchor)`.
- **Compose** -- distill the **kernel** (~1% feedback-vertex-set, recall gate ~0.95), then
  build search / guard / oracle / imputation / self-optimize FROM it.

Core rules: **embed what is latent, encode what is explicit** (never embed an already-meaningful
value; dual-encode text labels as an extra slot). **No-flatten** (slots stay typed; guard
per-slot). **Grounding mandatory** (ungrounded = provisional, never "trusted"). **Fail closed**
(a `{code,message,remediation}` error, never a silent fallback/mock). **>=10 diverse lenses per
panel.** Full detail in `references/method.md`.

## 3. Single source of truth -- NO side stores (issue #1348)

Every piece of engine/editor structured data lives inside Calyx (the Aster vault + Ledger)
and NOWHERE else. Forbidden as a source of truth: JSON, RON, SQLite, TOML state, bespoke
on-disk `*_store` formats.

- If Calyx cannot accept a datatype, CODE IT IN (new lens runtime, Modality, encoder, column
  family, MCP tool) -- do not spill to a side file.
- Only non-Calyx bytes permitted: content-addressed opaque blobs (raw glTF/PNG/audio) whose
  metadata/provenance still live in Calyx, and **lowered** deterministic artifacts (derived-from-
  Calyx, fingerprinted, reproducible -- never authoritative).
- The `#1455` pattern is the canonical violation: content baked into `crates/litd-engine/src/rpg/*.rs`
  as Rust constants (the compiled binary as source of truth). Do not repeat it.
- **Verify with the gate:** `pwsh scripts/check-calyx-side-store-eradication.ps1` -- greps
  litd-editor/engine/game/mcp for structured writes outside Calyx and RPG content factories that
  lack the shared authored-content validator. It fails closed (exit 1) on any violation.
- Never call Calyx from a deterministic hot loop (sim tick / render frame). LOWER the needed
  intelligence into frozen fingerprinted data and consume that (issue #1348 section 6).

## 4. Runtime env -- avoid the CALYX_LENS_UNREACHABLE false failure

Any litd-editor test that drives the real content-measurement path (the A38 CPU-explicit ONNX
lens) fails in a bare `cargo test` with `CALYX_LENS_UNREACHABLE: ORT_DYLIB_PATH must point to a
sm_120-capable ONNX Runtime`. This is NOT a regression -- it is the documented env requirement
(closed #1493). In ONE PowerShell session:

```
. .\scripts\Set-GameEditorCalyxRuntimeEnv.ps1          # sets ORT_DYLIB_PATH, CALYX_ORT_CAPI, CALYX_HOME, ...
$env:LITD_EDITOR_PROJECT_ROOT = "C:\code\GameEditor"    # MCP path-confinement root (#1518)
cargo test -p litd-editor --lib <filter>
```

Both vars are required: without ORT the lens is unreachable; without `LITD_EDITOR_PROJECT_ROOT`
the MCP filesystem tools refuse. The pre-commit gate does NOT run these tests, so a bare local
failure is usually just missing env -- apply this recipe before calling it a regression.

## 5. Aster vault paths are DIRECTORIES -- never fs::read them

The `document_path` / `ledger_path` / `state_path` args to the editor MCP tools (world, object,
project, ...) are Calyx Aster vault DIRECTORIES, not JSON files -- even when named
`world-document.json`. They persist via `write_calyx_json_bytes(vault_dir)` / `AsterVault::open(dir)`.

- NEVER `fs::read(path)` / `sha256_file(path)` on these paths. On Windows `fs::read` of a
  directory aborts with `Os { code: 5, kind: PermissionDenied }`; POSIX gives `IsADirectory`.
  This bit the `#1529` FSV examples.
- For FSV evidence, use the SHA-256 the store already computed over the canonical stored bytes:
  `read_mcp_{world,object}_document(&path)?.sha256`, and each tool output's
  `ledger_readback.sha256` / `document_readback.sha256`.

## 6. Canonical paths

All Calyx paths flow through `scripts/CalyxCanonicalPaths.ps1`. The canonical root is hard-pinned
to `C:\code\GameEditor\Calyx` (rejects junctions/links/alternate roots -- a linked git worktree
still uses the ONE canonical Calyx via `Set-GameEditorCalyxRuntimeEnv.ps1`, not its own copy).
Use `Get-GameEditorCanonicalCalyxHome`, `Resolve-GameEditorCalyxPath`, and
`Resolve-GameEditorEngineContentVault` rather than hand-building paths.

## Anti-patterns (refuse)

Flattening the panel; embedding an already-explicit value; selling associations past the
data-processing-inequality ceiling; labeling ungrounded output "trusted"; mutating a frozen lens;
making lens plug-in harder; bolting on a separate search/graph/vector DB; a side store as a
source of truth; a green test/harness standing in for verification against the bytes.

For authoring CF content on top of this substrate, see the [[content-pipeline]] skill.
