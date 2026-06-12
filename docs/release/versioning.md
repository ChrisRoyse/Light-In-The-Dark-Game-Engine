# Release Versioning & Changelog Process

**Status:** v1 process (D-2026-06-11-22 — own-site distribution).
**Consumers:** the M7 join guard (#74, build-hash compare), M9 hub
engine-version range matching (#180), the update-check manifest (#186).
**Code half (issue #184):** `litd/buildinfo` package + `-ldflags` stamping —
lands with the release pipeline (#182); this document is the process it
implements.

## Version scheme

- **SemVer** `MAJOR.MINOR.PATCH`, single source of truth = the **git tag**
  (`v0.4.2`). Nothing else declares a version; no version constants in code.
- **The sim version is part of the contract.** Any change to sim semantics —
  tick behavior, fixed-point math, data-table interpretation, opcode registry,
  PRNG call order — invalidates replays and saves, and therefore **requires at
  least a MINOR bump**, called out in the changelog under a `Sim` heading.
  Replays/saves/archives carry the version (+ data fingerprint) and refuse
  mismatches; the version bump is what makes that refusal honest.
- **Engine-version ranges** in world archives (`engine = ">=0.4 <0.6"`) are
  interpreted against this scheme; prerelease tags (`-rc.1`) never satisfy a
  range.

## Build stamping

- Release builds stamp `version` (the tag), `commit` (full SHA), and
  `builddate` via `-ldflags` into `litd/buildinfo`; every binary answers
  `--version` with all three.
- **The stamped commit SHA is the M7 join-guard build hash** — two clients
  match only if they run byte-identical releases.
- Untagged or dirty-tree builds stamp `dev+<commit>[-dirty]` and are **never
  publishable**: the release pipeline refuses to upload a `dev` build, and the
  hub/update manifest never lists one.

## Changelog

- Generated from **conventional commits** since the previous tag, grouped by
  type (`feat`/`fix`/`perf`/`docs`/…), with a mandatory `Sim` section whenever
  the release contains sim-semantics changes.
- Non-conventional commit messages land under **Uncategorized** — listed, never
  dropped.
- The generated file is **hand-edited before publish** (user-facing wording,
  upgrade notes, known issues), then published with the release on the
  download site (#182). The generated draft is an input, not the artifact.

## Release steps (summary)

1. Ensure main is green and the working tree clean.
2. Tag `vX.Y.Z` (annotated; message = one-line theme).
3. Pipeline builds the OS matrix with stamping, runs `--version` verification,
   generates the changelog draft.
4. Hand-edit changelog; publish artifacts + checksums + version manifest.
5. Update-check manifest (#186) points at the new version only after artifacts
   are live (no announce-before-available window).
