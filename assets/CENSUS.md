# Asset census — animtest R1 detection run (M0)

Census date: 2026-06-11 · Driver: `scripts/census.sh` · Raw table: `assets/census.tsv`
Screenshot evidence: `artifacts/census/*.png` (3 staggered shots per model, gitignored
binaries — regenerate with the driver; results are sha256-cached in
`artifacts/census/census-cache.tsv` so unchanged models never re-render).

## Method

Per GLB: `cmd/animtest -glb <file> -out <slug>` parses the file, prints mesh /
skin / animation / joint inventory, auto-frames the bounding box, plays clip 0
when present, and saves screenshots at 300 / 700 / 1100 ms. The driver measures
`cmp -l` byte-diffs between consecutive shots (animation-playback proof) and
the distinct-color count of shot 0 (blank-frame detection), then assigns one
verdict per model. Any anomaly is an R1 flag recorded in the table — nothing
is skipped.

`census.tsv` columns (no header row): path · verdict · meshes · skins · joints
· clips · bytediff(shot0→1) · bytediff(shot1→2) · distinct-colors(shot0) ·
exit-code · error.

## Totals

627 GLBs on disk → **627 census rows** (reconciled: `find assets -name '*.glb' | wc -l`
== row count; zero parse failures, all exit codes 0).

| Verdict | Models |
|---|---|
| OK-ANIMATED | 8 |
| OK-STATIC | 512 |
| R1-EMPTY-RENDER | 107 |

| Pack | GLBs | Result |
|---|---|---|
| kaykit-adventurers | 5 | 5 OK-ANIMATED (the D-31 reference set) |
| kaykit-builder | 41 | 40 OK-STATIC · 1 R1 (`trash_B.glb`, see #289) |
| kaykit-hexagon | 221 | 221 OK-STATIC |
| kenney-castle | 76 | 75 OK-STATIC · 1 R1 flag that is a **false positive** (`ground.glb`, see #290) |
| kenney-hexagon | 72 | 69 OK-STATIC · 3 OK-ANIMATED |
| kenney-retro-medieval | 105 | **105 R1-EMPTY-RENDER** (see #288) |
| quaternius-ultimate-fantasy-rts | 107 | 107 OK-STATIC |

## Animation-playback proof (8 animated models)

All shots byte-distinct, models posed mid-clip (manually inspected — e.g.
`kaykit-adventurers_Barbarian_glb-1.png` shows the barbarian mid-swing with
both axes raised, clearly not bind pose).

| Model | Joints | Clips | diff 0→1 (bytes) | diff 1→2 (bytes) |
|---|---|---|---|---|
| kaykit-adventurers/Barbarian.glb | 41 | 76 | 38,591 | 38,337 |
| kaykit-adventurers/Knight.glb | 41 | 76 | 39,261 | 39,180 |
| kaykit-adventurers/Mage.glb | 41 | 76 | 34,723 | 32,984 |
| kaykit-adventurers/Rogue.glb | 41 | 76 | 29,901 | 29,739 |
| kaykit-adventurers/Rogue_Hooded.glb | 41 | 76 | 29,484 | 29,676 |
| kenney-hexagon/building-mill.glb | — | 1 | 27,419 | 27,486 |
| kenney-hexagon/building-watermill.glb | — | 1 | 21,414 | 20,976 |
| kenney-hexagon/unit-mill.glb | — | 1 | 25,150 | 24,836 |

Static models with zero clips are exempt from the playback rule (their three
shots are correctly byte-identical) but remain screenshot-verified to render.

## R1 flag analysis (107 rows, root-caused)

| Cluster | Count | Root cause | Issue |
|---|---|---|---|
| kenney-retro-medieval (whole pack) | 105 | All 105 declare `extensionsRequired: ["KHR_materials_unlit"]` (verified per file). The vendored g3n loader's `loadMaterialUnlit` is a stub returning `nil, nil` → nil material → nothing drawn. Frames are pure black (1 color; `barrels` shot manually inspected). R-FMT-1 allows unlit, so these are conforming assets blocked by an engine gap. | #288 (p1) |
| kaykit-builder/trash_B.glb | 1 | 7 cm prop: auto-framing distance (extent × 1.6 ≈ 0.15) puts the whole model inside the camera's 0.3 near plane → fully clipped. Frame pure black (manually inspected). | #289 |
| kenney-castle/ground.glb | 1 | **False positive.** 4-vertex flat plane renders as a clean solid-green quad (manually inspected — clearly visible) but one material × one normal = exactly 2 distinct colors, tripping the ≤2-color blank heuristic. Manual verdict override: **renders correctly**. | #290 |

True non-rendering models: **106** (105 unlit + 1 near-plane), all tool/engine
defects, not asset defects. Zero GLBs failed to parse.

## Notes

- The quaternius-ultimate-fantasy-rts ingest is the buildings/scenery subset
  (no character units), so 0 clips across the pack is expected; the R-AST-3
  Idle/Walk/Attack/Death clip rule (#33) applies to unit models only.
- After #288/#289 land, delete the affected rows from
  `artifacts/census/census-cache.tsv` and re-run `scripts/census.sh` — only
  those models re-render; this table's R1 section should then shrink to the
  single documented false positive (or zero once #290 fixes the heuristic).
