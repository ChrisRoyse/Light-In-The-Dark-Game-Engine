# Asset Validation and Game Data

> Expands [PRD §6](../../PRD.md#6-asset--data-pipeline): the asset-validation CLI required by **R-FMT-2** ([PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only)) and **R-AST-2/R-AST-3**, the game-data table system of **R-AST-1** (the WC3 "SLK/object data" analogue), plus provenance checks for generated assets (**R-AST-5**, D-2026-06-11-12) and world-archive validation (D-2026-06-11-14, **R-SEC-1**). *Revised 2026-06-11.*
>
> Related: [Asset Pipeline](./pipeline.md) · [Materials and Lighting](../05-rendering/materials-and-lighting.md) · [Batching and Draw Calls](../05-rendering/batching-and-draw-calls.md)

---

## 1. Role in the build

`tools/assetcheck` is the single gate between the export stage and `assets/` ([Pipeline §6](./pipeline.md)). Its contract: **if an asset is in `assets/`, the runtime may assume it is conformant** — loaders carry no defensive fallbacks for malformed content (R-FMT-2: reject at build time, not runtime). It is a standalone Go CLI with no G3N/cgo dependency (it parses GLB containers and glTF JSON directly), so it runs in headless CI from M0 ([PRD §7](../../PRD.md#7-milestones): "asset packs downloaded + validated").

Usage shape:

```
assetcheck validate assets/...            # all assets, exit non-zero on any violation
assetcheck validate --json out.json ...   # machine-readable report for CI annotation
assetcheck inspect assets/models/units/human/footman.glb   # human-readable summary
assetcheck archive worlds/example.litdworld               # world-archive gate (§3.6, D-14)
```

Checks are versioned; the check-set version is recorded so a tightened rule re-flags previously passing assets explicitly rather than silently.

## 2. Check catalog (model assets)

Severity **error** fails the build; **warn** is reported but passes (warns are reserved for heuristics; every hard requirement below is an error).

### 2.1 Container and profile (R-FMT-1/2/3)

| ID | Check |
|---|---|
| FMT-GLB | File is a well-formed GLB (binary glTF 2.0 container, single binary buffer, no external URIs — textures embedded per [Pipeline §5](./pipeline.md)) |
| FMT-CORE | `extensionsUsed`/`extensionsRequired` ⊆ {`KHR_materials_unlit`}. Any other extension — including `KHR_materials_specular`, `KHR_materials_ior` ([g3n#296](https://github.com/g3n/engine/issues/296)), `KHR_draco_mesh_compression`, `EXT_meshopt_compression` — is an error |
| FMT-FEAT | Only features G3N's loader handles: triangles topology; accessor types in the supported set; ≤ 4 joint influences/vertex; no sparse accessors; no multi-scene files |

### 2.2 Geometry budgets (R-RND-2 via R-AST-2)

| ID | Check |
|---|---|
| GEO-TRI | Triangle count within the class budget — units ≤ 1,500, buildings ≤ 4,000, doodads/FX per the working figures in [Materials §2](../05-rendering/materials-and-lighting.md). Class inferred from the asset path (`assets/models/<class>/…`, [Pipeline §8](./pipeline.md)) |
| GEO-XFORM | Single root node; no non-identity object scales; origin at ground contact ([Pipeline §3](./pipeline.md) normalization contract) |
| GEO-BONES | Skinned models: one skin, ≤ 64 joints, normalized weights |

### 2.3 Atlas usage (R-RND-2 atlas clause)

| ID | Check |
|---|---|
| ATL-ONE | Exactly one material; exactly one base-color texture |
| ATL-MATCH | The embedded texture's content hash matches the declared faction/biome atlas for the asset's path (`assets/models/*/human/*` ⇒ `human` atlas). This is what makes runtime material-rebinding safe ([Batching §2](../05-rendering/batching-and-draw-calls.md)) |
| ATL-MAPS | No normal/occlusion/metallic-roughness/emissive *textures* (constant factors allowed) — [Materials §4](../05-rendering/materials-and-lighting.md) |
| ATL-TEAM | If the asset is flagged team-colorable in game data, its UVs reference the atlas team zone ([Pipeline §4](./pipeline.md)); if not flagged, they don't |

### 2.4 Animation clips (R-AST-3)

| ID | Check |
|---|---|
| ANI-REQ | Animated classes (units) contain glTF animations named exactly `Idle`, `Walk`, `Attack`, `Death`; optional `Spell`, `Portrait` permitted; **any other clip name is an error** (forces the rename discipline of [Pipeline §3](./pipeline.md)) |
| ANI-SANE | Every clip has nonzero duration; clips target the model's own skin; no clip exceeds a sanity cap (10 s) |
| ANI-STATIC | Buildings/doodads: animations optional; if present, names from the contractual set only (e.g. building `Idle` for flags/smoke) |

### 2.5 Audio and 2.6 naming

- AUD-OGG: everything under `assets/audio/` is Ogg Vorbis (R-AUD-1), with channel/sample-rate sanity caps.
- NAME-PATH: file and directory names match the conventions of [Pipeline §8](./pipeline.md) (`lower_snake_case`, class/faction layout); internal material/armature names follow the string-stable scheme there.

### 2.7 Provenance — generated assets (R-AST-5, D-2026-06-11-12)

*Added 2026-06-11 per D-2026-06-11-12.*

Assets produced by the generative pipeline ([Pipeline §2.1](./pipeline.md)) are validated like every other asset (all checks above apply) plus:

| ID | Check |
|---|---|
| PROV-MAN | Every asset whose source batch is under `sources/generated/` has a complete provenance entry: generating tool + model identifier, generation date, prompt/seed parameters where reproducible, curator + curation date, SHA-256 of the output. A generated asset without provenance is an error |
| PROV-HASH | The committed asset's content hash matches its provenance entry — a generated output silently edited after curation is an error (same regenerate-and-re-curate rule as [Pipeline §9.4](./pipeline.md)) |
| PROV-CUR | Release manifests reject generated assets lacking curator sign-off; un-curated batch leftovers must not reach `assets/` |
| PROV-LIC | Provenance entries declare the asset **owned** (no third-party license field) — generated assets join `CREDITS.md` under the owned-assets section, never under a pack license |

## 3. Game-data tables (R-AST-1)

### 3.1 Principles

The data system is the WC3 SLK/object-data analogue: **plain tables in `data/`, loaded once at startup, immutable at runtime.**

1. **Format: TOML for authored tables** (comments and multi-line readability matter for hand-edited balance data); JSON accepted for *generated* tables (e.g. map exports). One canonical in-memory schema; the loader accepts both.
2. **Immutability and determinism:** tables load into read-only structs before the first tick; the loaded-data fingerprint (hash of canonicalized content) is folded into the match's state-hash preamble (R-SIM-2), so replays and lockstep peers verify they run identical data.
3. **IDs are strings at edit time, dense indices at runtime:** entries are keyed by readable IDs (`"footman"`); the loader interns them to indices for hot-path use (no string lookups inside ticks, R-GC-3; no Go `map` iteration in gameplay — keyed slices per R-SIM-2).
4. **Stats are integers in sim units.** Anticipating the M1 fixed-point decision ([PRD §9.1](../../PRD.md#9-open-questions)): speeds in milli-units/sec, times in milliseconds (tick-quantized at load per R-EXEC-5, 50 ms granularity), percentages in basis points. No floats in gameplay-stat fields.

### 3.2 Layout and schema versioning

```
data/
  schema-version.toml          # data-format version; loader refuses mismatches
  units/      human.toml  orc.toml  neutral.toml
  abilities/  common.toml  human.toml
  upgrades/   human.toml
  maps/       <map>/map.toml  terrain.grid  doodads.toml
```

Tables are sharded by faction for review ergonomics; the loader merges shards and rejects duplicate IDs across files.

### 3.3 Unit schema with example: the footman

```toml
# data/units/human.toml
[footman]
name            = "Footman"                 # display name (UI only)
model           = "units/human/footman"     # asset ID → assets/models/units/human/footman.glb (pipeline.md §8)
icon            = "ui/icons/footman"
team_colorable  = true                      # cross-checked with ATL-TEAM (§2.3)
sounds          = { ready = "world/footman_ready", death = "world/footman_death" }

# Costs / production
gold_cost       = 135
lumber_cost     = 0
food_cost       = 2
build_time_ms   = 20000                     # quantized to 50 ms ticks at load
trained_at      = "barracks"
requires        = []                        # upgrade/building prerequisites by ID

# Combat (integers in sim units — §3.1.4)
max_life        = 420
life_regen_mbps = 250                       # milli-life per second
armor           = 2
armor_type      = "heavy"
attack          = { kind = "melee", damage_min = 11, damage_max = 13, damage_type = "normal",
                    cooldown_ms = 1350, range = 90, animation = "Attack" }   # clip per R-AST-3

# Movement / spatial
move_speed_mups = 270000                    # milli-units per second
turn_rate_mdps  = 540000                    # milli-degrees per second
collision_size  = 31                        # pathing-grid units (terrain.md §5)
sight_day       = 1400                      # fog-of-war stamp radius (fog-of-war-minimap-selection.md §2.1)
sight_night     = 800
selection_scale = 100                       # selection-circle radius, centi-units (fog-of-war §4.1)

# Abilities / upgrades
abilities       = ["defend"]
upgrades_used   = ["iron_forged_swords", "iron_plating"]
```

Ability and upgrade tables follow the same pattern: `abilities/*.toml` defines effect parameters (cast time, cooldown, mana, target filters, FX references — including VFX light priority/lifetime, validated against the ≤8-light rules of [Materials §6.2](../05-rendering/materials-and-lighting.md)); `upgrades/*.toml` defines per-level stat deltas applied by the sim's modifier system.

### 3.4 Data validation (`assetcheck data`, same CLI)

The data validator runs alongside asset checks in CI:

| Check | Description |
|---|---|
| Schema | Unknown keys rejected (catches typos — `move_sped` is an error, not a silently-defaulted stat); required keys present; values within declared ranges (e.g. `damage_min ≤ damage_max`, cooldowns > 0) |
| Cross-reference | Every `model`/`icon`/`sounds` value resolves to a validated asset; every `trained_at`/`requires`/`abilities`/`upgrades_used` ID resolves to a defined entry; no dependency cycles in requirements/tech tree |
| Asset–data coherence | A unit whose attack/ability references clip `Attack`/`Spell` must reference a model whose GLB contains that clip (joins §2.4 results); `team_colorable` consistent with ATL-TEAM |
| Determinism hygiene | All time fields divisible into 50 ms ticks (or flagged for quantization warning); numeric fields integral |
| Fingerprint | Emits the canonical data hash used in the match state-hash preamble (§3.1.2) |

### 3.5 Relationship to `jassgen`

R-AST-4's `tools/jassgen` (API manifest generator) is a sibling tool but a different domain: it processes the JASS API surface for code generation and audit ([PRD §6](../../PRD.md#6-asset--data-pipeline), §4.2). It shares the repo's tooling conventions (Go CLI, JSON report, CI gate) but no schema with `assetcheck`. The two meet only in M5's audit: API natives that read unit stats (e.g. the `GetUnitState` family collapsed per D5) must map onto fields this schema actually defines.

### 3.6 World-archive validation (`assetcheck archive`, D-2026-06-11-14)

*Added 2026-06-11 per D-2026-06-11-14.*

The world archive ([Pipeline §10](./pipeline.md)) gets its own subcommand, run in CI over fixture archives and by the engine at load time (a failing archive refuses to load — same fail-loud posture as R-FMT-2):

| Check | Description |
|---|---|
| Schema | Zip container well-formed; manifest present and schema-valid (unknown keys rejected); required fields present: per-entry content hashes, aggregate hash, engine-version requirement, hosting metadata fields (may be empty pre-M9, must exist) |
| Content hashes | SHA-256 of every entry matches its manifest hash; aggregate hash matches; any mismatch is an error (tamper/corruption gate — also what the M9 hub will trust) |
| Engine version | The required-engine-version range parses as valid semver; the loader refuses archives whose range excludes the running engine |
| Embedded assets | Every custom GLB/`.ogg`/texture in the archive passes the full §2 catalog — an archive is not a validator bypass ([Pipeline §10](./pipeline.md)) |
| Map data | The archive's tables and grids pass the §3.4 data validator, including cross-references into the archive's own custom assets |
| **Lua sandbox safety (R-SEC-1)** | Static lint of all bundled Lua: no `require`/access of `io`, `os`, network, FFI, or any module outside the game-API allowlist; no `load`/`loadstring` of non-bundled sources. The lint is a fast-fail courtesy and CI gate — the **runtime hard sandbox (D-2026-06-11-20: no-io/no-os/no-net VM, per-tick instruction + memory quotas) remains the actual security boundary**; the lint never substitutes for it |

## 4. CI integration

- **M0:** `assetcheck validate` green over the initial onboarded asset set; `assetcheck data` green over a seed data table.
- **Every build:** both validators run over all of `assets/` and `data/` (fast: no GPU, no Blender); violations annotate the PR via the `--json` report. Provenance checks (§2.7) run wherever generated assets exist — from M4, when the first `assetgen` outputs (terrain splat/cliff sets, UI icons) land. *(Revised 2026-06-11 per D-2026-06-11-12.)*
- **M6:** the vertical-slice skirmish runs entirely from validated assets + tables; zero asset/data special cases in code is an exit criterion alongside the [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram) budgets.
- **M6 onward:** `assetcheck archive` (§3.6) validates the world-archive fixture(s) on every build; the engine runs the same checks at archive load time. *(Added 2026-06-11 per D-2026-06-11-14.)*

## 5. Open items

1. Exact integer unit conventions (milli vs centi per stat family) — freeze together with the M1 fixed-point decision; the schema above is the draft.
2. Whether map terrain grids (`terrain.grid`) are text or packed binary — decide with the [Terrain](../05-rendering/terrain.md) M4 representation choice (tile-palette maps favor text).
3. Hot-reload of data tables in dev builds (convenience only; release builds stay load-once-immutable) — nice-to-have, not gated.
