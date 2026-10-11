# Incremental cache: cap (#1530/#1554) and corruption recovery (#1536)

Cargo never bounds the incremental cache and never GCs `target/` (rust-lang/cargo#12633). rustc
only GCs within a crate's own incremental dir when that crate recompiles; orphaned crate dirs
from other source-path hashes (multi-worktree churn) are never revisited.

## Size cap (10 GB)

`target\debug\incremental` grows ~0.7 GB/min under concurrent multi-agent builds; it once hit
58 GB / 97k files (#1530) and 50 GB / 476 dirs (#1554), taxing fingerprinting, linking, and AV
on every build (rust-lang/rust#137560 measured incr-cache copy at 83% of a 12s rebuild).

The cap is enforced by the shared primitive `Invoke-LitdIncrementalCacheCap` in
`scripts/IncrementalCacheCap.ps1` (a dot-sourced MODULE, not a standalone runner). It is called
from two places:
- `scripts/test-rust-package.ps1` -- before every gated build, under the gate mutex.
- `scripts/prune-build-artifacts.ps1` -- the out-of-band GC backstop, run on demand or by the
  daily `LITD-GameEditor-BuildArtifactGC` task.

On-demand: `pwsh -File scripts/prune-build-artifacts.ps1` (`-DryRun` to preview). Above the cap
it removes ONLY `<profile>\incremental` (keeps `deps\`), so the only cost is one non-incremental
build for that profile. Every evaluation logs `INCREMENTAL_CACHE_CHECK ... over_cap=<bool>`; a
prune logs `INCREMENTAL_CACHE_PRUNED bytes=<n> ... reason=over-cap`.

NEVER disable incremental: 20.4s WITH vs 93.0s WITHOUT on a one-line litd-editor rebuild (4.6x).

## Corruption recovery ("undefined symbol: anon.<hash>.llvm")

Debug builds run incremental ThinLTO. If one cached `.o` survives from a compilation with a
different import graph while sibling CGUs are freshly codegen'd, the fresh CGUs reference a
promoted anon constant the stale CGU no longer exports, and the link dies:

```
rust-lld: error: undefined symbol: anon.<32 hex>.<n>.llvm.<digits>
>>> referenced by serde_core-1.0.228/src/de/mod.rs:495
```

Upstream, open since 2021 (rust-lang/rust#81280). rustc self-heals corrupt incremental METADATA
but does NOT checksum cached `.o` work products, so this only ever surfaces at LINK time. It is a
FALSE-RED: the source is correct. A `.llvm.<digits>` discriminator, by construction, never
appears on a genuine missing-symbol bug.

Recovery lives in `scripts/IncrementalCacheRecovery.ps1` (also a dot-sourced MODULE). The test
gate (`test-gameeditor.ps1` -> `test-rust-package.ps1`) auto-detects the exact signature via
`Get-LitdIncrementalCorruption`, clears ONLY the implicated crate's incremental dir via
`Clear-LitdIncrementalCache` (never `deps\`, never the whole tree), STRICTLY ONCE, then rebuilds.
A second occurrence in the same run fails hard (that is a real bug, not a flake).

For raw cargo outside the gate, do the targeted clear yourself -- do NOT `cargo clean`:

```powershell
Remove-Item target\debug\incremental\<crate>-* -Recurse -Force
```

`<crate>` is the snake_case name rust-lld attributes the reference to (and the package under
test is always implicated). This costs exactly one non-incremental rebuild of that one crate.
