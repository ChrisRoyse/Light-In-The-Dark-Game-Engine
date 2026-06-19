# World-Archive Hub Index Format

The hub (`cmd/litd-hub`, package `litd/hub`) publishes a **static-friendly** index
of hosted world archives and serves the archives themselves with **no
authentication** (decision D-2026-06-11-23: anyone may download a world without an
account; accounts/ratings come later). The index is plain JSON any static host or
CDN can serve — the binary generates and serves it live, but a dumb file server
fronting the generated file behaves identically.

Implements #175. Per-entry hosting metadata comes from the archive manifest, which
carries it from day one (D-2026-06-11-14); set it at pack time with `worldpack
pack --author/--title/--description`.

## Endpoints

| Method | Path | Response |
|---|---|---|
| `GET` | `/index.json` | the index document (below); rebuilt from disk per request so a newly added archive appears on the next fetch |
| `GET` | `/worlds/<hash>.litdworld` | the archive bytes for that file hash; `404` if unknown |

Only `GET`/`HEAD` are accepted (else `405`). Unknown hashes and any non-hex /
traversal path under `/worlds/` return `404`; the service stays up. Index
regeneration is atomic — a request never observes a torn index/download pairing
(the published snapshot is swapped as a unit).

## Index document (schema `version: 1`)

```json
{
  "version": 1,
  "worlds": [
    {
      "hash": "2d7317e437448762b10234bf004bfea13907564910b88e83f7ec3d11714e305c",
      "engine_range": ">=0.1.0 <0.2.0",
      "title": "Ashen Veil",
      "author": "Ada Lovelace",
      "description": "first-flame duel",
      "size_bytes": 2848,
      "url": "/worlds/2d7317e437448762b10234bf004bfea13907564910b88e83f7ec3d11714e305c.litdworld",
      "published_at": "2026-06-19T19:11:38Z"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `version` | index schema version (this document) |
| `worlds` | entries, **sorted by `hash`** for a deterministic, diff-stable, cache-friendly index |
| `hash` | **SHA-256 of the archive file bytes** — the download-integrity hash a client checks after `GET`. Distinct from the archive's internal aggregate fingerprint. Also the content-addressed download key. |
| `engine_range` | semver range the world targets (from the verified manifest); clients filter compatibility |
| `title`, `author`, `description` | hosting metadata from the verified manifest (may be empty) |
| `size_bytes` | archive file size |
| `url` | content-addressed download path, `/worlds/<hash>.litdworld` |
| `published_at` | RFC 3339 (UTC), the archive file's modification time |

## Integrity contract

`hash` is the SHA-256 of the served file. A client downloading `url` and hashing
the bytes **must** get `hash` back — this is the no-account download-integrity
check. The hub additionally verifies every archive end-to-end through the
`worldarchive` read path (per-file hashes, aggregate, Lua re-lint, GLB re-check)
**before** indexing it: an archive that fails verification is **not** indexed and
**not** servable, and the reason is logged. Fail-closed (doctrine §2.4): a corrupt
or tampered archive never appears in the index, and one bad file never aborts the
index of the others.

## Running

```bash
go build -o bin/litd-hub ./cmd/litd-hub
./bin/litd-hub -data ./worlds -addr :8080      # -engine "" indexes all well-formed archives
curl -s :8080/index.json
curl -O :8080/worlds/<hash>.litdworld          # sha256 of the file == its index hash
```
