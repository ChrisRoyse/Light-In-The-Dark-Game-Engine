# Asset Foundation

The editor world view cannot reach the README visual targets with procedural
placeholder shapes alone. The current priority is a local asset vault with real
meshes, terrain, PBR materials, HDRIs, props, and later curated stylized packs.

## Source Of Truth

Large binaries are stored in the repo working tree but outside git at
`C:\code\GameEditor\local-asset-vault`.

- Download manifest:
  `C:\code\GameEditor\local-asset-vault\manifests\asset_intake_manifest.jsonl`
- Extraction manifest:
  `C:\code\GameEditor\local-asset-vault\manifests\asset_extraction_manifest.jsonl`
- Run summary:
  `C:\code\GameEditor\local-asset-vault\manifests\asset_intake_summary.json`
- Aggregate vault state summary:
  `C:\code\GameEditor\local-asset-vault\manifests\asset_vault_state_summary.json`
- Downloaded source files: `C:\code\GameEditor\local-asset-vault\sources`
- Extracted usable files: `C:\code\GameEditor\local-asset-vault\extracted`

The vault path is gitignored. Do not commit downloaded binary assets.

## Quality Bar

The asset library must support the README-grade editor and gameplay references:
`docs/readme/ED11-world-view.png`, `ED01-editor-shell.png`,
`ED06-material-assets.png`, `ED12-play-topdown.png`, `ED13-play-firstperson.png`,
and gameplay references such as `GP01-town.png` and `GP05-dungeon.png`.

> **`DOCTRINE-OBSOLETE-NAV(#944)`** (ADR-944 / #944, #1510, #1537): `ED12-play-topdown.png`,
> `GP01-town.png` and `GP05-dungeon.png` are **legacy composites**. Their always-on minimaps,
> quest trackers, and player-position markers are OBSOLETE and forbidden in packaged RPG
> runtime (map access is diegetic/acquired). Reproduce their environment art, camera/framing
> and combat-HUD craft ONLY — never their omniscient-navigation surfaces. 19 of the 111
> reference images carry forbidden navigation; the authoritative list is
> `scripts/reference-artifact-doctrine.tsv`, enforced on every commit by
> `scripts/check-reference-artifacts.ps1`.

The first library pass prioritizes:

- game-ready environment kits for medieval/fantasy towns, ruins, dungeons,
  props, trees, rocks, torches, walls, floors, and set dressing;
- PBR materials for stone, cobblestone, brick, dirt, moss, wood, lava/volcanic
  ground, terrain, and dungeon walls;
- HDRIs for credible viewport lighting and material preview;
- explicit license/provenance records for every downloaded file.

## Allowed Initial Sources

- Poly Haven: CC0 assets; public API exposes asset lists, download URLs, hashes,
  and file sizes. API use must include a unique User-Agent and commercial API
  usage needs a separate license/sponsorship discussion.
- ambientCG: CC0 assets; API v3 exposes searchable metadata and downloads.
- Kenney: CC0 game-ready packs; official asset pages expose direct zip downloads
  and a CC0 marker.
- KayKit: CC0 game-ready packs from the official
  `KayKit-Game-Assets` GitHub account. Intake validates each repo's README and
  `LICENSE.txt`, uses the GitHub tree API for exact blob sizes, and downloads
  raw files with one manifest record per file.
- Quaternius: CC0 game-ready packs discovered from the official Quaternius pack
  index when the page links a single public Google Drive folder. Intake
  validates the official page license/use marker, walks the Drive folder,
  requires an attachment filename plus `Content-Length` for every file, and
  records one manifest entry per file. Packs without a public Drive folder, or
  Drive folders that do not expose parseable entries, are rejected loudly and
  listed in the run summary.

No other source is approved until its license and redistribution terms are
verified from an official source or from a downloaded license file. Quaternius
megakits that route only through itch/source-kit flows are ingested only through
the explicit manual-acquisition path below; hidden Itch URLs and guessed source
keys remain forbidden.

Generated KTX2 fixture assets are allowed only when they are derived from an
already verified CC0 vault record, written under
`local-asset-vault\sources\generated\ktx2-fixtures`, and recorded in
`asset_intake_manifest.jsonl` with the original source record's license,
license URL, source URL, byte count, SHA-256, and derivation fields. The
`litd-editor` `ktx2_fixture_fsv` example is the supported path: it writes the
physical `.ktx2`, upserts the manifest row, reloads the vault catalog from disk,
and validates the container through the KTX2 parser before reporting success.

## Intake Contract

Run:

```powershell
python scripts\assets\intake_cc0_assets.py --max-gb 20 --download-workers 8
```

The script:

- accepts only allowlisted CC0 source/license pairs;
- downloads exact requested profiles instead of silently downgrading quality;
- verifies Poly Haven MD5 checksums where available and records SHA-256 for
  every download;
- verifies KayKit GitHub blob sizes and Quaternius Google Drive attachment
  sizes exactly;
- extracts ambientCG zips into manifest-backed directories;
- fails on partial downloads, size/hash mismatches, and ambiguous extracted
  directories;
- writes logs under `C:\code\GameEditor\local-asset-vault\logs`.

The editor runtime also stream-verifies the manifest SHA-256 for every loaded
download and extraction record before it can report the #1242 P0 asset
foundation status. A missing file, byte-count mismatch, or same-size hash
mismatch fails startup/evidence generation with `editor.asset_vault.*` fields
instead of allowing stale or corrupted assets into the editor.

The stable CLI entrypoint remains `scripts\assets\intake_cc0_assets.py`.
Implementation is split by responsibility under `scripts\assets\asset_intake`:
`config.py` for allowlisted source policy, `source_*.py` for source adapters,
`net.py` for HTTP/Drive metadata, `storage.py` for download/hash/extract and
manifest state, and `cli.py` for orchestration.

KayKit and Quaternius can be refreshed with:

```powershell
python scripts\assets\intake_cc0_assets.py --source kaykit --source quaternius --max-gb 5 --download-workers 24 --drive-metadata-workers 16
```

For full Quaternius refreshes, the Drive metadata walk runs in bounded parallel:

```powershell
python scripts\assets\intake_cc0_assets.py --source kaykit --source quaternius --max-gb 300 --download-workers 24 --drive-metadata-workers 16 --quaternius-pack-workers 8 --no-extract-zips
```

For Quaternius pages that do not expose a strict public Google Drive folder,
download the archive through the visible sanctioned browser flow first
(Quaternius official page -> Itch/product/download-key/source-key page), then
ingest the resulting local zip:

```powershell
python scripts\assets\ingest_quaternius_manual_archive.py `
  --slug scifiessentialskit `
  --archive "$env:USERPROFILE\Downloads\Sci-Fi Essentials Kit[Standard].zip" `
  --expected-filename "Sci-Fi Essentials Kit[Standard].zip" `
  --expected-size <bytes> `
  --expected-sha256 <sha256>
```

The manual ingester fails closed unless the slug is one of the tracked
Quaternius source-key/Itch packs, the official Quaternius page contains a
CC0/free-commercial-use marker, the page exposes a Quaternius Itch product URL,
the archive is a valid zip with real model and image files, and the optional
expected filename/size/hash checks match exactly. It records the official page,
Itch/product artifact URL, license marker, filename, byte length, SHA-256,
manual acquisition timestamp, and extraction manifest rows. This path copies a
local archive that was acquired through the sanctioned browser/auth flow; it
does not discover or request hidden Itch download URLs.

Manual Full State Verification must read the manifest and the actual files on
disk after any run. Script success alone is not proof that the vault is correct.

## Verified Local State

As of the 2026-06-28 manual Quaternius intake pass, direct readback from
`asset_vault_state_summary.json`, the manifests, and the source/extracted
directories showed:

- 15,195 download manifest records and 15,195 source files.
- 10,869,970,817 source bytes on disk.
- 5,295 KayKit records from 10 official GitHub repos.
- 9,214 Quaternius source records.
- 5,719 extraction manifest records and 5,742 extracted files.
- 965,257,787 extracted bytes on disk.
- 0 `.part` files.

Verified manual Quaternius Standard zips:

- `scifiessentialskit`: 166,910,545 bytes, SHA-256
  `a08346d538aa39fbea9fa492e03620d1860fc6214eedd62a4f5db373ac6fca01`,
  292 extracted records.
- `medievalvillagemegakit`: 161,003,471 bytes, SHA-256
  `e60dea67c10f30dccccfbff92a7933f5ea5cfe99be0e2a0fa5118cceabeec5c4`,
  936 extracted records.

The full expanded Drive-backed Quaternius/KayKit run log is
`C:\code\GameEditor\local-asset-vault\logs\asset_intake_20260627_190949.log`;
it records 71 discovered official Drive-backed Quaternius pages, 68 accepted
packs, 13,495 verified records for that KayKit+Quaternius refresh, and
4.353 GiB downloaded/verified for that run. Quaternius pages without strict
public Drive entries must use the sanctioned manual path above.
