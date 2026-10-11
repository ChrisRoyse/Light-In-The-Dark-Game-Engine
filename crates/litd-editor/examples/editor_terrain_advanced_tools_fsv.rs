use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::stable_fingerprint,
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
        TerrainMagicWandMode, apply_terrain_brush, flood_fill_tile, magic_wand_select,
    },
    world_creation::{
        EditorWorldCreationSpec, EditorWorldRecord, WorldAsterReadback, parse_world_record,
        read_world_record, write_world_record,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_TERRAIN_ADVANCED_FSV",
        "issue1211_terrain_advanced",
        "manual-fsv.json",
    )?;
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1211 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");

    let flood = flood_fill_happy(&root, &aster_dir)?;
    let magic = magic_wand_edge()?;
    let size_one = size_one_brush_edge(&root, &aster_dir)?;
    let large = large_brush_edge(&root, &aster_dir)?;

    let mut snapshot = json!({
        "schema": "litd.issue1211.terrain_advanced_tools_fsv.v1",
        "issue": 1211,
        "source_of_truth": "Aster world record readback plus FSV artifact selection reports",
        "artifact_path": path,
        "happy_path": flood,
        "edges": [magic, size_one, large],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote terrain advanced tools FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!("flood {}", snapshot["happy_path"]["after"]);
    println!("magic {}", snapshot["edges"][0]["after"]);
    println!("size-one {}", snapshot["edges"][1]["after"]);
    println!("large {}", snapshot["edges"][2]["after"]);
    Ok(())
}

struct StoredRecord {
    state_path: PathBuf,
    readback: WorldAsterReadback,
}

fn flood_fill_happy(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("flood-fill", 8, 6);
    paint_tiles(
        &mut record,
        &[(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2), (7, 5)],
        "tile.sand",
    );
    let before_selection = magic_wand_select(
        &record.document,
        "terrain_cell_1_1",
        TerrainMagicWandMode::SameTile,
    )?;
    let flood = flood_fill_tile(&mut record.document, "terrain_cell_1_1", "tile.moss")?;
    sync_record(&mut record, "flood_fill")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let changed_tiles = flood
        .changed_cell_ids
        .iter()
        .map(|id| parsed.document.terrain_cells[id].tile_id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(changed_tiles, BTreeSet::from(["tile.moss".to_string()]));
    assert_eq!(
        parsed.document.terrain_cells["terrain_cell_7_5"].tile_id,
        "tile.sand"
    );
    Ok(json!({
        "before": {
            "selection_count": before_selection.selected_count,
            "selected_cell_ids": before_selection.selected_cell_ids,
            "separate_same_tile": "terrain_cell_7_5"
        },
        "after": {
            "report": flood,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "changed_tiles": changed_tiles,
            "separate_same_tile_after": parsed.document.terrain_cells["terrain_cell_7_5"].tile_id,
            "passed": true
        }
    }))
}

fn magic_wand_edge() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("magic-boundary", 8, 6);
    paint_tiles(
        &mut record,
        &[(1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (3, 2), (7, 5)],
        "tile.sand",
    );
    let report = magic_wand_select(
        &record.document,
        "terrain_cell_1_1",
        TerrainMagicWandMode::SameTile,
    )?;
    let selection = report.selection_set();
    assert_eq!(report.selected_count, 6);
    assert!(!selection.selected_ids.contains("terrain_cell_7_5"));
    Ok(json!({
        "name": "magic wand stops at tile boundary and disconnected same-tile patch",
        "before": {"seed": "terrain_cell_1_1", "disconnected_same_tile": "terrain_cell_7_5"},
        "after": {
            "selected_count": report.selected_count,
            "selected_cell_ids": report.selected_cell_ids,
            "boundary_cell_ids": report.boundary_cell_ids,
            "selection_summary": selection.summary(),
            "passed": true
        }
    }))
}

fn size_one_brush_edge(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("size-one-brush", 8, 6);
    let before_center = record.document.terrain_cells["terrain_cell_4_3"].height;
    let before_neighbor = record.document.terrain_cells["terrain_cell_4_4"].height;
    let brush = TerrainBrushSpec {
        center_tile_x: 4,
        center_tile_z: 3,
        size_tiles: 1,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::RaiseHeight { delta: 2.0 },
    };
    let report = apply_terrain_brush(&mut record.document, &brush)?;
    sync_record(&mut record, "size_one_brush")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    assert_eq!(report.mutation.changed_count, 1);
    assert!(
        (parsed.document.terrain_cells["terrain_cell_4_3"].height - (before_center + 2.0)).abs()
            < 0.0001
    );
    assert_eq!(
        parsed.document.terrain_cells["terrain_cell_4_4"].height,
        before_neighbor
    );
    Ok(json!({
        "name": "size-1 brush changes only center cell",
        "before": {"center_height": before_center, "neighbor_height": before_neighbor},
        "after": {
            "changed_count": report.mutation.changed_count,
            "weighted_count": report.weighted_cells.len(),
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "center_height": parsed.document.terrain_cells["terrain_cell_4_3"].height,
            "neighbor_height": parsed.document.terrain_cells["terrain_cell_4_4"].height,
            "passed": true
        }
    }))
}

fn large_brush_edge(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("large-brush", 128, 128);
    let brush = TerrainBrushSpec {
        center_tile_x: 64,
        center_tile_z: 64,
        size_tiles: 1001,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::PaintTile {
            tile_id: "tile.large.brush".to_string(),
        },
    };
    let report = apply_terrain_brush(&mut record.document, &brush)?;
    sync_record(&mut record, "large_brush")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let large_tile_count = parsed
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == "tile.large.brush")
        .count();
    assert_eq!(large_tile_count, 128 * 128);
    Ok(json!({
        "name": "1000+ brush covers max 128x128 world",
        "before": {"world_cells": 128 * 128, "brush_size_tiles": 1001},
        "after": {
            "changed_count": report.mutation.changed_count,
            "weighted_count": report.weighted_cells.len(),
            "max_weight": report.max_weight,
            "min_nonzero_weight": report.min_nonzero_weight,
            "large_tile_count": large_tile_count,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "passed": true
        }
    }))
}

fn write_read_record(
    root: &Path,
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<StoredRecord, Box<dyn std::error::Error>> {
    let state_path = root.join(format!("{}.json", record.spec.revision_id));
    write_world_record(aster_dir, record)?;
    let readback = read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id))?;
    Ok(StoredRecord {
        state_path,
        readback,
    })
}

fn sync_record(
    record: &mut EditorWorldRecord,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    record.source_issue = 1211;
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.terrain_cell_count = record.document.terrain_cells.len();
    record.default_tile_count = record
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
        .count();
    record.summary = format!(
        "world={} issue1211={} cells={} fingerprint={}",
        record.spec.properties.display_name,
        label,
        record.terrain_cell_count,
        record.document_fingerprint
    );
    Ok(())
}

fn test_world(label: &str, width: u32, height: u32) -> EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1211-{label}");
    spec.revision_id = format!("issue1211-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1211 {label}");
    spec.properties.description =
        format!("Known terrain advanced tool settings for issue 1211 {label} FSV");
    spec.properties.width_tiles = width;
    spec.properties.height_tiles = height;
    spec.properties.playable_bounds.max_tile_x_exclusive = width as i32;
    spec.properties.playable_bounds.max_tile_z_exclusive = height as i32;
    let mut record = litd_editor::world_creation::create_editor_world_record(spec).unwrap();
    record.source_issue = 1211;
    record
}

fn paint_tiles(record: &mut EditorWorldRecord, coords: &[(i32, i32)], tile_id: &str) {
    for (x, z) in coords {
        record
            .document
            .terrain_cells
            .get_mut(&format!("terrain_cell_{x}_{z}"))
            .unwrap()
            .tile_id = tile_id.to_string();
    }
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "storage_schema": readback.storage_schema,
        "chunk_count": readback.chunk_count,
        "bytes": readback.bytes.len(),
        "sha256": readback.sha256,
        "document_fingerprint": readback.document_fingerprint
    })
}

fn write_document_with_final_state(
    path: &PathBuf,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            if fs::metadata(path)?.len() != expected_len {
                return Err("terrain advanced FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("terrain advanced FSV artifact byte-count stabilization failed".into())
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
