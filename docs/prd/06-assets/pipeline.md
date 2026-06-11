# Asset Pipeline

> Expands [PRD §6](../../PRD.md#6-asset--data-pipeline) (R-AST-2, R-AST-3) and the format constraints of [PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only) (R-FMT-1..3) into the concrete asset flow: CC0 pack → Blender normalization → core-GLB export → validation → `assets/`.
>
> Related: [Validation and Data](./validation-and-data.md) · [Materials and Lighting](../05-rendering/materials-and-lighting.md) · [Batching and Draw Calls](../05-rendering/batching-and-draw-calls.md) · [Terrain](../05-rendering/terrain.md)

---

## 1. Pipeline overview

Every model in the game travels one road, with no exceptions and no hand-tuned one-offs:

```
[1] CC0 pack download          (sources/  — pristine, never edited)
        │
[2] Blender normalization      (.blend working files + scripted batch pass)
        │   scale · orientation · atlas rebind · rig/clip naming · cleanup
        │
[3] Core-GLB export            (Blender glTF 2.0 exporter, locked settings profile)
        │
[4] Asset validator            (tools/assetcheck — R-FMT-2 / R-AST-2 / R-AST-3 gates)
        │   pass → assets/      fail → build error, asset never lands
        │
[5] assets/                    (runtime-loadable, validated GLBs + audio + data refs)
```

Two principles govern the design:

1. **Pristine sources, reproducible derivations.** Downloaded packs are immutable inputs; everything in `assets/` must be regenerable from `sources/` + scripts + `.blend` files. No binary in `assets/` is ever edited in place.
2. **The validator is the only gate.** Nothing reaches `assets/` without passing `tools/assetcheck` ([Validation and Data §2](./validation-and-data.md)) — "before entering `assets/`" per R-AST-2. Runtime code may assume every asset is conformant; there is no runtime fallback for malformed assets (R-FMT-2: reject at build time, not runtime).

## 2. Stage 1 — Pack acquisition (`sources/`)

The approved CC0/free packs are those vetted in [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasyrts-packs-zero-cost): Quaternius Ultimate Fantasy RTS, KayKit Medieval Hexagon / Builder, Kenney Castle / Hexagon / Retro Medieval.

- Each pack is stored under `sources/<vendor>-<pack>-<version>/` exactly as downloaded (original archive retained), with a `MANIFEST.toml` recording: download URL, retrieval date, license (CC0 / "free, commercial OK" with license text copied in), SHA-256 of the archive, and the pack version.
- License files travel with the pack and aggregate into a generated `assets/CREDITS.md` (Quaternius is not CC0-attribution-free in spirit; we credit everything regardless — zero-cost, not zero-courtesy, consistent with [PRD G4](../../PRD.md#21-goals)).
- Adding a *new* pack requires a license review entry in the manifest before any model from it enters stage 2.
- `sources/` is large and may live in Git LFS or a fetch script (`tools/fetch-sources`) keyed by the manifests — the repo stays buildable from manifests alone.

## 3. Stage 2 — Blender normalization

CC0 packs disagree on scale, axes, rig conventions, texture binding, and clip naming. Normalization makes them disagree no longer. The pass is **scripted** (Blender headless `--background --python`, scripts in `tools/blender/`) so it is repeatable; manual `.blend` work is reserved for genuine art edits (re-skinning a model onto the faction atlas, adding a missing Death clip) and those `.blend` files are committed.

Normalization contract (what stage 3's export must satisfy):

| Property | Convention |
|---|---|
| Scale | 1 Blender unit = 1 meter; a standard infantry unit ≈ 1.8 m tall; applied (no object-level scale ≠ 1.0) |
| Orientation | Model faces **+Y forward, +Z up** in Blender (exports to glTF's +Z forward/+Y up); facing-angle 0 in the sim = model forward |
| Origin | Ground contact point at origin (0,0,0); buildings: footprint center at origin, aligned to the pathing-grid cell size ([Terrain §5](../05-rendering/terrain.md)) |
| Transforms | All object transforms applied; no stray parent offsets; single root node per logical model |
| Geometry | Triangulated; within budget (units ≤ 1,500 tris, buildings ≤ 4,000 — R-RND-2, hard-gated in stage 4); no loose vertices, no >4 bone influences per vertex |
| Materials | Exactly one material, bound to the **faction/biome atlas** ([Materials §3](../05-rendering/materials-and-lighting.md)); models shipping per-model textures are re-UV'd/re-baked onto the atlas here; team-color regions mapped onto the atlas's team mask zone (§4) |
| Rig | One armature per animated model; bone count ≤ 64; no constraints/drivers surviving to export (baked) |
| Animation clips (**R-AST-3**) | Actions named exactly `Idle`, `Walk`, `Attack`, `Death` (+ optional `Spell`, `Portrait`); each a separate NLA track/action so the exporter emits discrete glTF animations; clip frame ranges trimmed (no dead frames); `Walk` authored loop-clean; `Death` ends on the corpse pose (no auto-loop) |

Source clips with other names (`Run`, `Attack01`, `Die`…) are **renamed, not aliased** — the runtime knows only the contractual names, and the validator enforces presence ([Validation §2.4](./validation-and-data.md)). A per-pack rename table lives in the pack's normalization script, so re-running against a pack update is mechanical.

## 4. Team-color mask convention

Fixed here, consumed by the shader work in [Batching §5](../05-rendering/batching-and-draw-calls.md) (R-RND-7):

- Each faction atlas reserves a **team zone**: a designated rectangular strip of the atlas (location recorded in the atlas's sidecar metadata, `atlas.toml`). Texels in the zone are authored grayscale (intensity = shading detail); the shader detects team-zone UVs and multiplies by the `TeamColor` uniform.
- Normalization maps every team-colorable surface (shoulder pads, banners, roofs…) onto the team zone. KayKit packs that ship 4 pre-colored team variants are normalized by remapping the *one* base variant onto the team zone and discarding the other three.
- The validator checks that models declare whether they are team-colorable and that team-colorable models actually reference team-zone UVs.

## 5. Stage 3 — Core-GLB export

Export uses Blender's glTF 2.0 exporter with a **locked settings profile** (checked-in preset, applied by the batch script — never hand-clicked):

- Format: **GLB** (single binary, textures embedded), per [PRD §3.2](../../PRD.md#32-model-format-gltf-20-binary-glb-core-profile-only).
- **Core profile only** (R-FMT-1): no KHR extensions except `KHR_materials_unlit` where the unlit material path is authored; specifically no `KHR_materials_specular`/`ior` (unsupported by G3N — [g3n#296](https://github.com/g3n/engine/issues/296)), **no Draco/Meshopt compression** (R-FMT-3 — G3N can't decode it; low-poly GLBs are small uncompressed).
- Materials: metallic-roughness with constant factors, base-color atlas as the only texture map (no normal/AO/emissive maps — [Materials §4](../05-rendering/materials-and-lighting.md)).
- Animations: sampled/baked keyframes, one glTF animation per contractual clip, names preserved exactly.
- +Y up (glTF standard); tangents not exported (no normal maps); skin weights normalized.

The exporter profile is itself under test: a fixture `.blend` exports in CI and the output GLB is validated, so a Blender version bump that changes exporter behavior is caught before it corrupts a batch re-export.

## 6. Stage 4 — Validation gate

Every exported GLB runs through `tools/assetcheck` (full check catalog in [Validation and Data §2](./validation-and-data.md)): core-glTF conformance, triangle budgets, atlas-only texturing, required animation clips, naming. The tool exits non-zero on any violation; the make target that populates `assets/` depends on it, and CI re-validates all of `assets/` on every build (cheap — parsing GLB headers and counting is milliseconds per file). M0's exit criterion "asset packs downloaded + validated" ([PRD §7](../../PRD.md#7-milestones)) means stages 1–4 run end-to-end for an initial model set.

## 7. Directory layout

```
sources/                              # pristine downloads (LFS / fetch-script)
  quaternius-ultimate-fantasy-rts-1.0/
    MANIFEST.toml                     # url, date, license, sha256
    <original archive + extraction>
  kaykit-medieval-hexagon-1.0/
  kenney-castle-kit-2.0/

tools/
  blender/                            # normalization scripts (headless)
    normalize_common.py
    pack_quaternius_rts.py            # per-pack rename tables & fixups
    export_profile.py                 # locked glTF export settings
  assetcheck/                         # validation CLI (Go) → see validation-and-data.md
  fetch-sources/

art/                                  # committed .blend working files (manual edits only)
  units/  buildings/  doodads/  atlases/
    atlases/human.atlas.png + human.atlas.toml   # authored 1024² + team-zone metadata

assets/                               # validated runtime assets — generated, never hand-edited
  models/
    units/      human/footman.glb
    buildings/  human/barracks.glb
    doodads/    forest/tree_pine_a.glb
    terrain/    forest/tile_cliff_low.glb        # tile meshes if Option B (terrain.md §6)
    fx/
  audio/        ui/  world/  music/              # .ogg only (R-AUD-1)
  ui/                                            # HUD textures, fonts
  CREDITS.md                                     # generated from MANIFESTs

data/                                 # game-data tables (R-AST-1) → validation-and-data.md §3
  units/  abilities/  upgrades/  maps/
```

## 8. Naming conventions

Enforced by the validator (lint class — [Validation §2.6](./validation-and-data.md)):

- **Files:** `lower_snake_case.glb`; pattern `assets/models/<class>/<faction|biome>/<name>.glb`. Variants suffix with `_a`, `_b` (`tree_pine_a.glb`); no version numbers in filenames (git is the version).
- **IDs:** an asset's runtime ID is its path under `assets/models/` minus extension (`units/human/footman`). Game-data tables ([Validation §3](./validation-and-data.md)) reference models by this ID — no hardcoded paths in code.
- **Clips:** exactly the R-AST-3 contractual set, `PascalCase`: `Idle`, `Walk`, `Attack`, `Death`, `Spell`, `Portrait`. Future multi-variant support (`Attack2`) requires a PRD amendment to R-AST-3, not ad-hoc names.
- **Nodes/materials inside GLBs:** material named after its atlas (`atlas_human`); armature `rig`; mesh node = model name. Keeps loader-side bindings string-stable.
- **Atlases:** `<faction|biome>.atlas.png` + `<faction|biome>.atlas.toml` (team zone, swatch map) in `art/atlases/`.
- **Audio:** `assets/audio/<channel>/<name>.ogg`, `lower_snake_case`.

## 9. Change workflow

1. **Updating a pack:** drop new version into `sources/` with a new manifest → re-run the pack's normalization script → batch export → validator → diff `assets/` in review.
2. **Adding a unit:** ensure model normalized (stage 2, possibly a new `.blend` for atlas re-bake or missing clips) → export → validate → add a data-table entry ([Validation §3.3](./validation-and-data.md)) referencing the asset ID → data validator cross-checks that the referenced GLB exists and has the clips the unit's abilities need.
3. **Asset bugs** (wrong facing, broken clip) are fixed in stage-2 scripts or `.blend` files and re-exported — never by editing the GLB.

## 10. Tooling requirements and versions

- **Blender**: single pinned version per development cycle (recorded in `tools/blender/VERSION`); normalization and export scripts refuse to run under any other version, since exporter behavior differences are exactly the silent-corruption risk §5's fixture test guards against. Upgrading Blender is a deliberate change: bump the pin, re-run the fixture test, batch re-export, review the `assets/` diff.
- **`assetcheck`**: pure Go, no cgo/GPU — runs anywhere CI runs; the same binary validates models, audio, naming, and data tables ([Validation §1](./validation-and-data.md)).
- **Make targets**: `make assets` (normalize → export → validate → stage into `assets/`), `make assets-verify` (re-validate `assets/` without rebuilding — the cheap CI path), `make credits` (regenerate `CREDITS.md`).
- The pipeline must run headlessly end-to-end on Linux CI (Blender `--background`); no step may require a GUI session, mirroring the engine's own headless-sim principle (R-SIM-4).

## 11. Acceptance summary

| Milestone | Pipeline acceptance |
|---|---|
| M0 | Stages 1–4 run end-to-end headlessly for the initial onboarded set; `make assets-verify` green in CI ([PRD §7](../../PRD.md#7-milestones)) |
| M4 | Every model in the render-core benchmark scene came through this pipeline — zero hand-placed or unvalidated assets |
| M6 | Vertical slice runs entirely from `assets/` + `data/`; `CREDITS.md` complete; a clean-checkout rebuild of `assets/` from `sources/` is byte-identical (reproducibility proof of §1.1) |

## 12. Open items

1. Git LFS vs fetch-script for `sources/` (repo-size policy) — decide in M0.
2. Whether `assets/` GLBs are committed or CI-built artifacts — draft: committed (deterministic builds without Blender installed; Blender needed only when assets change).
3. Atlas authoring workflow for merging multiple packs onto one faction atlas (UV re-bake cost) — prototype during M0 asset onboarding with one Quaternius unit.
