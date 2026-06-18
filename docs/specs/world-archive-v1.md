# World Archive Format v1

**Status:** v1 (`litdworld-version: 1`). **Owner:** `tools/worldpack` (producer)
+ `tools/assetcheck archive` (consumer/validator).
**Producer:** `go run ./tools/worldpack pack [flags] <src-dir> <out.litdworld>`.
**Validator:** `go run ./tools/assetcheck archive [--json] [--engine-version X.Y.Z] <archive.litdworld>`.
**Decisions:** D-14 (archive format), D-21 (format spec is public surface),
D-23 (hosting metadata present from day one). **Posture:** R-FMT-2 (loud refusal,
no partial load), R-SEC-1 (Lua sandbox lint).

A world archive (`.litdworld`) is the single-file distribution unit for a
playable world: its map data, Lua scripts, and custom assets, plus a manifest
that makes the bundle content-addressed and tamper-evident. One file packs,
ships (hub / LAN join), and loads. The format is **deterministic**: a given
source directory always packs to byte-identical archive bytes, so the archive's
own SHA-256 is a stable identity across machines and OSes.

## Container

A `.litdworld` archive is a standard **ZIP** with:

- **Deflate** compression, fixed entry mod-time (1980-01-01 UTC, the ZIP epoch),
  uniform `0644` mode, and entries written in sorted path order — these remove
  every source of nondeterminism so identical inputs hash identically.
- One reserved entry, **`.litdworld-manifest`**, the content-hash table of
  contents. The name is reserved: a source tree containing it is a pack error.
- All other entries are the world's payload files at their original relative
  paths (slash-separated). Case-only collisions (e.g. `Foo.txt` vs `foo.txt`)
  are a pack error, never a silent overwrite.

## Manifest

`.litdworld-manifest` is a UTF-8 line format: a header block, then one row per
payload file. Header lines are `key: value`; field order is fixed (so the
manifest — and thus the archive — stays byte-stable).

```
litdworld-version: 1
engine-range: >=0.1.0 <0.2.0
author: Paul Ascenzi
title: First Flame
description: ashen-veil duel
aggregate-sha256: 2c11403cbe485d9145e5aed3d2ed39200de6d4f0a8e3986b0e428eed1c74a878
files: 2
8c10d11872b51bca…7de605af 5 scripts/main.lua
afeea2a0126e2a1c…6cb93856 11 world.toml
```

### Header fields

| Field | Required | Meaning |
|---|---|---|
| `litdworld-version` | yes | Format version. `1` for this spec. |
| `engine-range` | yes | Semver range the world targets (see grammar below). May be `*`. |
| `author` | yes (value may be empty) | Hosting metadata (D-23). |
| `title` | yes (value may be empty) | Hosting metadata (D-23). |
| `description` | yes (value may be empty) | Hosting metadata (D-23). |
| `aggregate-sha256` | yes | Whole-archive fingerprint (see below). |
| `files` | yes | Count of payload rows that follow. Terminates the header. |

**Hosting metadata (D-23)** must be *present from day one*. The values may be
empty pre-M9 (the hosting product is not built yet), but the **fields are
mandatory** so hosting/catalog tooling can rely on them. A missing field is a
schema error; a present-but-empty value passes. Newlines in a value are stripped
at pack time (the manifest is a line format). Set them with
`worldpack pack --author … --title … --description …`.

### Payload rows

Each row is `<sha256-hex> <size-bytes> <relative-path>`, split on the first two
spaces — so a path may itself contain spaces. The hash is the SHA-256 of that
file's exact bytes, computed at pack time.

### Aggregate hash (D-14)

`aggregate-sha256` is the SHA-256 over each payload row's per-file hash, taken in
relative-path-sorted order (`hash + "\n"` concatenated). It is a single
whole-archive fingerprint: one comparison detects any added, removed, or
re-hashed entry. The validator recomputes it independently from the parsed rows
(a writer bug cannot mask a verification bug). It can be reproduced externally:

```sh
unzip -p w.litdworld .litdworld-manifest \
  | awk 'f{print $3" "$1} /^files:/{f=1}' | sort | awk '{print $2}' \
  | sha256sum
```

### Engine-range grammar

Space-separated comparators, each `(>=|<=|>|<|=)?MAJOR.MINOR.PATCH`, or the
single token `*` (admits all). Example: `>=0.1.0 <0.2.0`. A version *satisfies*
the range when it satisfies every comparator.

## Validation (consumer)

`assetcheck archive` opens the zip and reports findings; a non-empty finding set
exits non-zero. The check table (validation-and-data.md §3.6):

| Rule | Refusal |
|---|---|
| `ARCHIVE-SCHEMA` | No manifest entry; malformed header/row; missing required field (incl. hosting metadata or `aggregate-sha256`). |
| `ARCHIVE-HASH` | A payload's bytes do not match its manifest hash; a manifest row with no entry (or vice-versa); `aggregate-sha256` ≠ recomputed aggregate. |
| `ARCHIVE-VERSION` | No engine-range; range not well-formed; or (with `--engine-version X.Y.Z`) the engine does not satisfy the range. |
| `ARCHIVE-READ` | A zip entry cannot be read. |
| Sandbox lint (R-SEC-1) | A bundled `.lua` references `io`/`os`/`net` or uses `require`/`loadfile`/`dofile`. |
| Full asset catalog | **An archive is not a validator bypass:** embedded `.glb`/`.ogg` run the *same* §2 asset catalog as loose assets (tri budgets, KHR-extension allowlist, audio limits). E.g. an embedded Draco-compressed GLB is `GLTF-COMPRESS`. |

**Refusal posture (R-FMT-2):** every failure is loud and the load is
all-or-nothing — a broken archive never loads partially. The engine load path
applies the same checks plus the `--engine-version` satisfaction guard against
the running build.

## Producer constraints

- Archives are packed only from inputs that already pass `assetcheck` (loose
  assets + map data); manifest hashes are computed at pack time.
- `worldpack pack` is deterministic by construction (sorted entries, fixed
  mod-time/mode, Deflate) — packing the same source twice yields identical bytes.

## Round-trip

`worldpack unpack <archive.litdworld> <dest-dir>` restores every payload file's
exact bytes, verifying each against the manifest hash; the manifest entry itself
is not written into the restored tree. A tampered entry fails the hash check.

## See also

- pipeline.md §10 — container contents, producer/consumer paths.
- validation-and-data.md §3.6 — the check table this validator implements.
- [map-format.md](map-format.md) — the map data an archive bundles.
- decisions.md D-14, D-21, D-23.
