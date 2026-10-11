# local-asset-vault Runbook (#1569)

Operator/agent runbook for (re)materializing `local-asset-vault`. This exists
because the vault was emptied to 0 files mid-session (#1569 data-loss incident),
and at that time there was NO doc describing how to restore it - so an empty
vault read as a mysterious environmental failure instead of "run X". This is
that "run X".

For the full design/provenance model see `docs/assets/asset-foundation.md`; this
runbook is the focused restore-and-verify procedure.

## What local-asset-vault Is (and Why It Is Empty-able)

- It is a large directory at `<primary checkout>/local-asset-vault`, i.e.
  `C:\code\GameEditor\local-asset-vault` on this host. It lives INSIDE the
  primary working tree but is **gitignored** (`git check-ignore
  local-asset-vault` matches), so its contents are NOT recoverable from git.
- It holds real downloaded CC0 art (meshes, textures, HDRIs, game kits) plus the
  manifests that prove each file's license/provenance/hash. It is materialized
  OUT OF BAND by the asset-intake pipeline (#782 / #1242), never committed.
- A linked git worktree does NOT get its own copy; the runtime resolver in
  `crates/litd-engine/src/asset_vault_root.rs` finds the primary checkout's vault
  via `git rev-parse --git-common-dir`. So all worktrees share the ONE real vault
  beside the primary checkout - which is exactly why a careless recursive delete
  through a worktree junction (see Prevention below) can empty it for everyone.

Expected top-level layout of a materialized vault (the resolver's
`EXPECTED_VAULT_ROOTS` = at least one of these must be present, else it reports
`ASSET_VAULT_EMPTY`):

```
local-asset-vault/
  sources/      downloaded source files, per source/asset
  extracted/    unzipped usable files (e.g. Kenney GLB kits)
  manifests/    asset_intake_manifest.jsonl + extraction + state summaries
  logs/         intake run logs
```

## Which Packs the Failing Tests Need

The tests that break on an empty vault load three fixtures. Traced from the test
constants and the intake scripts, each fixture maps to an exact source pack and
an exact restore path:

| Fixture (vault-relative)                                                        | Source pack                 | Restored by                     |
| ------------------------------------------------------------------------------- | --------------------------- | ------------------------------- |
| `sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf`           | Quaternius `animatedmech`   | `--source quaternius` (Drive)   |
| `extracted/kenney/game-kit/castle-kit/Models/GLB_format/wall.glb`               | Kenney `castle-kit`         | `--source kenney`               |
| `extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb`             | Kenney `mini-dungeon`       | `--source kenney`               |

Accuracy note (verified against the scripts, correcting the incident's shorthand):

- `Stan.gltf` comes from the **automated** Quaternius Google-Drive intake, whose
  files land under `sources/quaternius/game-kit/<slug>/...` (see
  `scripts/assets/asset_intake/source_game_kits.py`). Its slug `animatedmech` is
  a publicly Drive-backed pack; it is NOT one of the manual Itch/source-key
  packs.
- `wall.glb` and `gate.glb` are **Kenney** kits, not Quaternius. Kenney kits are
  downloaded as zips and unzipped under `extracted/kenney/game-kit/<slug>/...`.
- The **manual** Quaternius ingest path (below) writes to a DIFFERENT tree,
  `sources/quaternius/manual-game-kit/<slug>/...`, and covers the Itch/source-key
  megakits in `TARGET_SLUGS`. Those packs are needed for a COMPLETE vault but are
  NOT what the three failing test fixtures depend on. Do not expect the manual
  ingester to reproduce the `game-kit/animatedmech` path - it cannot.

`TARGET_SLUGS` (the manual Itch/source-key Quaternius packs, from
`scripts/assets/ingest_quaternius_manual_archive.py`): `3dcardkitfantasy`,
`downtowncitymegakit`, `fantasypropsmegakit`, `medievalvillagemegakit`,
`modularcharacteroutfitsfantasy`, `modularscifimegakit`, `scifiessentialskit`,
`stylizednaturemegakit`, `universalanimationlibrary`, `universalanimationlibrary2`,
`universalbasecharacters`.

## Re-Materialize: Exact Command Sequence

All commands run from the primary checkout root (`C:\code\GameEditor`). The
stable entrypoint is `scripts\assets\intake_cc0_assets.py`; it is fail-loud and
idempotent (already-present, hash-verified files are skipped, so re-running only
fills the gaps).

### Step 1 - Restore what the failing tests need (minimum)

This covers all three fixtures above (Kenney kits + the Drive-backed Quaternius
packs, which includes `animatedmech`):

```powershell
python scripts\assets\intake_cc0_assets.py `
  --source kenney `
  --source quaternius `
  --max-gb 300 `
  --download-workers 24 `
  --drive-metadata-workers 16 `
  --quaternius-pack-workers 8
```

To pull only the single Quaternius pack the tests need (faster; the slug must
match a discovered pack), swap `--source quaternius` for:

```powershell
  --source quaternius --quaternius-pack-slug animatedmech
```

(`--quaternius-pack-slug` is repeatable and mutually exclusive with
`--quaternius-pack-limit`.)

### Step 2 - Full vault (all approved sources)

For a complete vault (adds Poly Haven, ambientCG, KayKit):

```powershell
python scripts\assets\intake_cc0_assets.py `
  --source polyhaven --source ambientcg --source kenney --source kaykit --source quaternius `
  --max-gb 300 --download-workers 24 --drive-metadata-workers 16 --quaternius-pack-workers 8
```

(With no `--source`, the pipeline defaults to Poly Haven + ambientCG ONLY, so
always name the sources you need.)

### Step 3 - Manual Quaternius Itch/source-key packs (for a complete vault)

The `TARGET_SLUGS` megakits do NOT expose a public Drive folder; they are
acquired by the operator through the sanctioned browser flow (official Quaternius
page -> Itch/product/download-key page), and the downloaded local zip is then
ingested. This path does NOT discover or request hidden URLs; it only verifies
and copies a zip you already downloaded:

```powershell
python scripts\assets\ingest_quaternius_manual_archive.py `
  --slug scifiessentialskit `
  --archive "$env:USERPROFILE\Downloads\Sci-Fi Essentials Kit[Standard].zip" `
  --expected-filename "Sci-Fi Essentials Kit[Standard].zip" `
  --expected-size <bytes> `
  --expected-sha256 <64-hex>
```

Real flags (from `ingest_quaternius_manual_archive.py`): `--slug` (required, must
be one of `TARGET_SLUGS`), `--archive` (required, a local `.zip`), and the
OPTIONAL integrity guards `--expected-filename`, `--expected-size`,
`--expected-sha256`. The `--expected-*` guards are optional but strongly
recommended - they make the ingest fail closed on the wrong or a corrupted file.
The ingester also fails closed unless the official Quaternius page carries a
CC0/free-commercial-use marker and exposes a Quaternius Itch product URL, and the
zip contains real model + image files. Repeat per slug for each manual pack you
need.

## Verify the Vault Is Populated

Do a real filesystem readback afterward - script success alone is not proof.

```powershell
# 1. Vault is present and NOT empty (child count > 0).
(Get-ChildItem C:\code\GameEditor\local-asset-vault | Measure-Object).Count

# 2. The three failing-test fixtures exist on disk.
$v = "C:\code\GameEditor\local-asset-vault"
Test-Path "$v\sources\quaternius\game-kit\animatedmech\Flat_Colors\glTF\Stan.gltf"
Test-Path "$v\extracted\kenney\game-kit\castle-kit\Models\GLB_format\wall.glb"
Test-Path "$v\extracted\kenney\game-kit\mini-dungeon\Models\GLB_format\gate.glb"

# 3. The expected top-level roots exist (matches EXPECTED_VAULT_ROOTS).
"sources","manifests","extracted" | ForEach-Object { Test-Path "$v\$_" }
```

All `Test-Path` calls must print `True` and the child count must be `> 0`. If the
directory exists but the count is `0`, the resolver emits the distinct
`ASSET_VAULT_EMPTY: resolved <path> but it contains 0 asset files ...
(data-loss incident #1569)` diagnostic instead of a per-file `os error 3`
cascade - that is the present-but-empty shape this runbook restores.

As a stronger check, the asset-backed tests themselves are the acceptance gate
(e.g. `litd-engine` `asset_vault_root::*` and `animation::*`,
`litd-editor` gltf/asset suites). Run them per the normal test-gate procedure
after restoring; env setup for the editor asset tests is in
`docs/assets/asset-foundation.md` and the ONNX bootstrap note in `AGENTS.md`.

## Prune Redundant Assets Safely (#1821)

Never hand-delete vault files: the loader (`AssetVaultCatalog`) fail-louds on any
manifest record whose file is missing, so a bare `rm` leaves dangling records
that break every catalog load until the JSONL is repaired by hand (this is what
#1811 had to do outside the tool). Use the fail-loud `prune` subcommand instead;
it removes files AND their manifest records atomically and junction-safely.

```powershell
# Dry run (default): show what WOULD be removed, mutate nothing.
python scripts\assets\intake_cc0_assets.py prune `
  --vault-root C:\code\GameEditor\local-asset-vault `
  --path sources\quaternius\game-kit\demo\dup.fbx

# Apply: delete the file(s) + drop their records from BOTH manifests, purge stale
# .litd-verify-cache receipts, and regenerate asset_vault_state_summary.json.
python scripts\assets\intake_cc0_assets.py prune `
  --vault-root C:\code\GameEditor\local-asset-vault `
  --path sources\quaternius\game-kit\demo\dup.fbx --apply

# Clean any pre-existing dangling records (record present, file already gone).
python scripts\assets\intake_cc0_assets.py prune `
  --vault-root C:\code\GameEditor\local-asset-vault --prune-dangling --apply
```

What prune guarantees (all checked BEFORE any mutation, so a rejected prune leaves
the vault byte-for-byte unchanged):

- Junction-safe: refuses any target that resolves OUTSIDE the vault or crosses a
  reparse point (junction/symlink), and only ever `os.unlink`s individually named
  files -- never a recursive/mirroring delete (the #1569 vault-wipe class).
- Load-bearing test fixtures (`Stan.gltf`, `Leela.gltf`, `gate.glb`, `wall.glb`,
  `Rock1.fbx`) can never be pruned.
- Atomic manifest/file consistency: records are rewritten first (temp file +
  atomic rename) so the catalog never observes a dangling record; a file that
  cannot be deleted is left as a benign orphan (no record) a re-run cleans, and is
  reported LOUD.
- Vault tripwire: the count of TOP-LEVEL vault entries must be unchanged before vs
  after; a drop (the junction-purge signature) aborts LOUD.

## Prevention: the robocopy /MIR junction footgun (#1569, #1518)

The vault was emptied because a worktree held a directory JUNCTION pointing at
the real `local-asset-vault`, and a cleanup step ran `robocopy /MIR` over that
worktree. **`robocopy /MIR` follows directory junctions and purges THROUGH them
by default** (no `/XJ`), so mirroring an empty dir over the worktree deleted the
junction target - the real vault - for every checkout. This is the same
reparse-point class as #1518.

Rules (also stated loudly in `AGENTS.md`):

- **NEVER** use `robocopy /MIR` (or any recursive purge without junction
  exclusion) to delete a git worktree or any tree that may contain directory
  junctions.
- Unlink junction LINKS first, THEN delete the tree:
  `[System.IO.Directory]::Delete($junction, $false)` (no-recurse) or
  `cmd /c rmdir <junction>` (WITHOUT `/S`) removes the link without touching its
  target. Only after the links are gone is a recursive delete safe.
- Tripwire: capture `(Get-ChildItem local-asset-vault | Measure-Object).Count`
  before and after ANY worktree cleanup; the count must be unchanged. A drop to
  0 means a junction was purged through - stop and restore via this runbook.
