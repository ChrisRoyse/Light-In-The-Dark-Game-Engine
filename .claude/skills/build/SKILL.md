---
name: build
description: Fast, correct build/test loop for the Light in the Dark Rust/wgpu engine + editor. Use before cargo build/check/test, when a cargo test fails unexpectedly with CALYX_LENS_UNREACHABLE, when builds feel slow, or on "undefined symbol: anon.<hash>.llvm" link errors. Covers Calyx runtime env, editor-is-a-binary (--bin not --lib), the warm target cache, and incremental-cache cap/recovery.
---

# Build & test loop (GameEditor)

Rust/wgpu engine + editor on Windows. There is NO CI; local FSV is the only gate. Use the
narrowest verdict tier that answers your question. To ship, use the /gate skill.

## 1. Set the Calyx runtime env FIRST (or editor tests false-fail)

Any `litd-editor` test that drives the Calyx content path (A38 ONNX lens) fails a bare
`cargo test` with `CALYX_LENS_UNREACHABLE: ORT_DYLIB_PATH must point to a sm_120-capable ONNX
Runtime`. That is a MISSING-ENV symptom, NOT a regression. In ONE PowerShell 7 session:

```powershell
. .\scripts\Set-GameEditorCalyxRuntimeEnv.ps1        # sets ORT_DYLIB_PATH, CALYX_HOME, CALYX_ORT_CAPI
$env:LITD_EDITOR_PROJECT_ROOT = "C:\code\GameEditor"  # MCP path-confinement root (#1518)
```

Look for `CALYX_RUNTIME_ENV_OK ...` on success. Both vars are required: no ORT -> lens
unreachable; no `LITD_EDITOR_PROJECT_ROOT` -> MCP filesystem tools refuse. The test gate
(`scripts/test-gameeditor.ps1`) sets this itself; a bare `cargo test` does not.

## 2. The editor UI is a BINARY, not the lib

`EditorShellRuntime` and every `main_editor_*.rs` / `main_egui_renderer.rs` are `mod`'d from
`crates/litd-editor/src/main.rs` -- BINARY code. `cargo build/test -p litd-editor --lib` does
NOT compile them. After ANY change to those files verify with the binary:

```powershell
cargo build -p litd-editor --bin litd-editor
cargo test  -p litd-editor --bin litd-editor <filter>   # for bin #[cfg(test)] tests
```

A `--lib` green can hide a broken editor binary (burned #1461). See references/editor-binary.md.

## 3. NEVER wipe target/ -- it is a warm cache

`target\fsv\test-gameeditor\cargo-target` and `target\` are STABLE warm incremental caches
(cargo fingerprint-skips ~95% of the graph: wgpu, naga, rapier, 40+ Calyx crates at O3).
Wiping forces a cold rebuild. NEVER `cargo clean` or delete a `cargo-target` to fix a problem.

If a build feels slow, check the incremental cache size FIRST -- do not assume a compiler bug:

```powershell
"{0:N1} GB" -f ((Get-ChildItem target\debug\incremental -Recurse -File -Force -EA SilentlyContinue |
  Measure-Object Length -Sum).Sum / 1GB)
```

Healthy is ~2 GB; capped at 10 GB. Once hit 58 GB / 97k files (#1530) and taxed every build.
Enforce the cap on demand (removes only `<profile>\incremental`, keeps `deps\`):

```powershell
pwsh -File scripts/prune-build-artifacts.ps1            # -DryRun to preview
```

The daily `LITD-GameEditor-BuildArtifactGC` task runs this hands-off.
NEVER disable incremental (`CARGO_INCREMENTAL=0`): 20s vs 93s warm rebuild (4.6x). Bound the
cache, don't switch it off. See references/incremental-cache.md.

## 4. "undefined symbol: anon.<hash>.llvm" = cache corruption, NOT your code

A link error like `rust-lld: error: undefined symbol: anon.<32hex>.<n>.llvm.<digits>` is a
stale ThinLTO work-product from the incremental cache (upstream rust-lang/rust#81280), a
FALSE-RED. Do NOT `cargo clean`. Fixes, narrowest first:

- Run through the test gate -- `scripts/test-gameeditor.ps1` auto-detects this exact signature
  and clears ONLY the implicated crate's incremental dir, ONCE, then rebuilds.
- For raw cargo outside the gate, delete just the implicated crate's incremental dir:
  ```powershell
  Remove-Item target\debug\incremental\<crate>-* -Recurse -Force   # <crate> is snake_case
  ```

A `.llvm.<digits>` discriminator NEVER appears on a real missing-symbol bug (a genuinely
missing extern fn has no discriminator). See references/incremental-cache.md.

## 5. Verdict tiers -- pick the narrowest that answers the question

| Tier | Command | Use when |
|------|---------|----------|
| 1 fastest | `cargo check --workspace --all-targets` | "does it compile?" |
| 1b flaky check | `pwsh -File scripts/gate-cargo-check-retry.ps1`-backed step | shared target lock contention gives bare exit=-1 (no diagnostics) |
| 2 binary | `cargo build -p litd-editor --bin litd-editor` | touched editor binary code (see 2) |
| 3 targeted test | `cargo test -p <crate> <filter>` | verify one behavior (env from 1 first) |
| 4 full gate | `pwsh -File scripts/test-gameeditor.ps1 -Package <pkg>` | pre-push confidence; auto env + cache recovery |

`gate-cargo-check-retry.ps1` discriminates a REAL compile error (rustc diagnostics present ->
fail immediately) from a TRANSIENT abort (no diagnostics -> bounded retry). A non-zero exit
with NO `error[...]`/`could not compile` is usually just target-lock contention, not your bug.

## Common false failures

| Symptom | Real cause | Fix |
|---------|-----------|-----|
| `CALYX_LENS_UNREACHABLE` on a litd-editor test | Calyx runtime env not set | Section 1 (dot-source + LITD_EDITOR_PROJECT_ROOT) |
| `--lib` passes but editor "works" unverified | UI lives in the binary | build/test `--bin litd-editor` (Section 2) |
| Build suddenly slow | bloated `target\debug\incremental` | measure + prune (Section 3) |
| `undefined symbol: anon.*.llvm` at link | stale incremental work-product | targeted incremental clear (Section 4) |
| `cargo check` exit=-1, no `error[...]` lines | concurrent build holds target lock | retry; it is transient, not a compile error (Section 5) |

## References
- references/editor-binary.md -- why UI is binary-only; Clone/Debug derive rule
- references/incremental-cache.md -- cap and corruption-recovery mechanics
