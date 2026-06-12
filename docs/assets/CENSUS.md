# Asset census — animtest R1 detection run (M0)

Census date: 2026-06-11 · Driver: `scripts/census.sh` · Raw table: `docs/assets/census.tsv`
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
· clips · bytediff(shot0→1) · bytediff(shot1→2) · render-coverage(shot0) ·
exit-code · error. Render-coverage = pixels differing from the black clear
color (post-#290); rows cached before #290 carry the old distinct-colors
number in that column — verdicts are unaffected (every cached pass was
screenshot-verified).

## Totals

627 GLBs on disk → **627 census rows** (reconciled: `find assets -name '*.glb' | wc -l`
== row count; zero parse failures, all exit codes 0).

| Verdict | Models |
|---|---|
| OK-ANIMATED | 8 |
| OK-STATIC | 619 |
| R1-EMPTY-RENDER | 0 |

*Updated post-#288: the LITD-PATCH implementing `KHR_materials_unlit` in
the vendored g3n loader landed (patches/engine/0003) and all 105
kenney-retro-medieval models were re-censused — every one now renders
textured geometry (OK-STATIC). Manually re-inspected: wall-detail,
floor-stairs-corner-outer, roof-high-side — all visible brick/wood
textures.*

| Pack | GLBs | Result |
|---|---|---|
| kaykit-adventurers | 5 | 5 OK-ANIMATED (the D-31 reference set) |
| kaykit-builder | 41 | 41 OK-STATIC (`trash_B.glb` fixed by #289 near-plane scaling) |
| kaykit-hexagon | 221 | 221 OK-STATIC |
| kenney-castle | 76 | 76 OK-STATIC (`ground.glb` false flag cleared by the #290 metric fix) |
| kenney-hexagon | 72 | 69 OK-STATIC · 3 OK-ANIMATED |
| kenney-retro-medieval | 105 | 105 OK-STATIC (after #288 unlit patch; originally 105 R1-EMPTY-RENDER) |
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
| kenney-retro-medieval (whole pack) | 105 | All 105 declare `extensionsRequired: ["KHR_materials_unlit"]`; the vendored g3n loader stubbed `loadMaterialUnlit` as nil,nil with its dispatch commented out → nil material, nothing drawn. **FIXED** by LITD-PATCH 0003 (#288); all 105 re-censused OK-STATIC. | #288 (closed) |
| kaykit-builder/trash_B.glb | 1 | 7 cm prop: auto-framing distance (extent × 1.6 ≈ 0.15) put the whole model inside the camera's fixed 0.3 near plane → fully clipped. **FIXED**: animtest now scales the near plane with the framed extent; re-censused OK-STATIC (304 colors). | #289 (closed) |
| kenney-castle/ground.glb | 1 | **False positive.** 4-vertex flat plane renders as a clean solid-green quad (manually inspected) but one material × one normal = exactly 2 distinct colors, tripping the old ≤2-color heuristic. **FIXED**: the driver now counts non-background pixels (<50 ⇒ blank); re-censused OK-STATIC with 44,361 model pixels. | #290 (closed) |

True non-rendering models: **106** (105 unlit + 1 near-plane), all tool/engine
defects, not asset defects. Zero GLBs failed to parse.

## Notes

- The quaternius-ultimate-fantasy-rts ingest is the buildings/scenery subset
  (no character units), so 0 clips across the pack is expected; the R-AST-3
  Idle/Walk/Attack/Death clip rule (#33) applies to unit models only.
- #288 (unlit), #289 (near plane), and #290 (blank heuristic) all landed and
  the affected models were re-censused: **627/627 models render-verified,
  zero open R1 flags.**
