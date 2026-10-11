use std::{fs, path::Path};

use litd_editor::{
    history::{EditorDocument, UndoRedoStack, stable_fingerprint, to_stable_ron},
    map_build::{
        StaticShadowBakeSettings, calculate_shadows_and_save_map, export_script,
        load_saved_map_build, parse_generated_map_script,
    },
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
        apply_terrain_brush,
    },
    terrain_authoring::{
        TerrainObjectPlacementRequest, TerrainPlacementKind, TerrainRegionPlacementRequest,
        place_terrain_object, place_terrain_region,
    },
    terrain_pathing::{
        TerrainPathMovement, TerrainPathingPaintMode, effective_pathing_for_cell,
        paint_pathing_cells, query_path_between_cells,
    },
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, WorldAsterReadback,
        create_editor_world_record, parse_world_record, read_world_record, validate_world_record,
        write_world_record,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "support/issue734_preview.rs"]
mod issue734_preview;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, report_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_TERRAIN_ISSUE734_FSV",
        "issue734_terrain_editor",
        "manual-fsv.json",
    )?;
    let root = report_path
        .parent()
        .ok_or("FSV report path has no parent")?;
    fs::create_dir_all(root)?;

    let before_path = root.join("document_before.ron");
    let after_path = root.join("document_after.ron");
    let script_path = root.join("map.lua");
    let shadow_path = root.join("static_shadows.ron");
    let map_path = root.join("saved_map.ron");
    let preview_path = root.join("mapdata-preview.png");
    let aster_dir = root.join("aster");

    let mut record = issue734_world()?;
    let before_document = record.document.clone();
    write_text(&before_path, &to_stable_ron(&before_document)?)?;

    let paint_report = apply_terrain_brush(
        &mut record.document,
        &TerrainBrushSpec {
            center_tile_x: 2,
            center_tile_z: 2,
            size_tiles: 3,
            shape: TerrainBrushShape::Square,
            falloff: TerrainBrushFalloff::Constant,
            operation: TerrainBrushOperation::PaintTile {
                tile_id: "tile.issue734.moss".to_string(),
            },
        },
    )?;
    let raise_report = apply_terrain_brush(
        &mut record.document,
        &TerrainBrushSpec {
            center_tile_x: 2,
            center_tile_z: 2,
            size_tiles: 1,
            shape: TerrainBrushShape::Square,
            falloff: TerrainBrushFalloff::Constant,
            operation: TerrainBrushOperation::RaiseHeight { delta: 2.5 },
        },
    )?;
    let pathing_report = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_3_2".to_string()],
        TerrainPathingPaintMode::Blocked,
    )?;
    let unit_report = place_terrain_object(
        &mut record.document,
        &unit_request("unit.issue734.guard", 2, 2),
    )?;
    let region_report = place_terrain_region(
        &mut record.document,
        &TerrainRegionPlacementRequest {
            region_id: "region.issue734.spawn".to_string(),
            name: "Issue 734 Spawn".to_string(),
            center_tile_x: 4,
            center_tile_z: 3,
            half_extent_tiles_x: 1.5,
            half_extent_tiles_z: 1.0,
            clamp_to_playable_bounds: false,
        },
    )?;
    let start_report = place_terrain_object(
        &mut record.document,
        &TerrainObjectPlacementRequest {
            object_id: "start.issue734.player_1".to_string(),
            kind: TerrainPlacementKind::StartLocation,
            type_id: "start.player_1".to_string(),
            target_tile_x: 0,
            target_tile_z: 0,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: false,
            reject_unwalkable_ground: false,
            owner: Some("player_1".to_string()),
        },
    )?;
    sync_record(&mut record)?;
    validate_world_record(&record)?;

    write_text(&after_path, &to_stable_ron(&record.document)?)?;
    let script_export = export_script(&record.document, &script_path)?;
    let save_receipt = calculate_shadows_and_save_map(
        &record.document,
        &shadow_path,
        &map_path,
        StaticShadowBakeSettings::default(),
    )?;
    let aster = write_and_read_world(&aster_dir, &record)?;
    let aster_record = parse_world_record(&aster.bytes)?;
    issue734_preview::write_preview_png(&aster_record.document, &preview_path)?;

    let after_readback = read_document(&after_path)?;
    let map_readback = load_saved_map_build(&map_path)?;
    if after_readback != record.document || map_readback.document != record.document {
        return Err("persisted mapdata readback did not match edited document".into());
    }
    if aster_record.document != record.document {
        return Err("Aster world record did not match edited document".into());
    }
    let script_readback = fs::read_to_string(&script_path)?;
    let script_summary = parse_generated_map_script(&script_readback)?;
    let path_query = query_path_between_cells(
        &record.document,
        "terrain_cell_2_2",
        "terrain_cell_4_2",
        TerrainPathMovement::GroundUnit,
    )?;
    if !path_query.path_found
        || path_query
            .path_cell_ids
            .contains(&"terrain_cell_3_2".to_string())
    {
        return Err("pathing query did not route around the blocked center cell".into());
    }

    let blocked_edge = edge_unwalkable_blocked(&record.document)?;
    let clamped_edge = edge_out_of_bounds_clamped(&record.document)?;
    let undo_edge = edge_undo_restores(&before_document, &record.document)?;
    let mut snapshot = json!({
        "schema": "litd.issue734.terrain_editor_fsv.v1",
        "issue": 734,
        "source_of_truth": "EditorDocument RON, generated Lua map script, saved map RON readback, Aster world record, effective pathing grid, and deterministic mapdata PNG preview",
        "artifact_path": report_path,
        "readme_refs": ["docs/readme/ED02-terrain-editor.png", "docs/readme/ED11-world-view.png", "docs/readme/ED01-editor-shell.png"],
        "happy_path": {
            "before": {
                "document_path": before_path,
                "document_sha256": sha256_file(&before_path)?,
                "object_count": before_document.objects.len(),
                "region_count": before_document.regions.len(),
                "terrain_cell_count": before_document.terrain_cells.len()
            },
            "after": {
                "paint_report": paint_report,
                "raise_report": raise_report,
                "pathing_report": pathing_report,
                "unit_report": unit_report,
                "region_report": region_report,
                "start_report": start_report,
                "after_document_path": after_path,
                "after_document_sha256": sha256_file(&after_path)?,
                "script_path": script_path,
                "script_sha256": script_export.script_sha256,
                "script_readback_sha256": sha256_hex(script_readback.as_bytes()),
                "script_counts": script_summary,
                "saved_map_path": map_path,
                "saved_map_sha256": save_receipt.artifact_sha256,
                "saved_map_readback_matches": map_readback.document == record.document,
                "aster": readback_summary(&aster),
                "aster_document_fingerprint": aster_record.document_fingerprint,
                "pathing_grid": pathing_grid(&record.document)?,
                "path_query": path_query,
                "blocked_cell_flags": effective_pathing_for_cell(&record.document, "terrain_cell_3_2")?,
                "preview_png": preview_path,
                "preview_png_sha256": sha256_file(&preview_path)?,
                "preview_png_bytes": fs::metadata(&preview_path)?.len()
            }
        },
        "edges": [blocked_edge, clamped_edge, undo_edge],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&report_path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "wrote issue734 terrain FSV to {} sha256={}",
        report_path.display(),
        report_readback.sha256
    );
    println!(
        "preview={} sha256={} map={} grid={:?}",
        preview_path.display(),
        sha256_file(&preview_path)?,
        sha256_file(&map_path)?,
        pathing_grid(&record.document)?
    );
    Ok(())
}

fn edge_unwalkable_blocked(
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut probe = document.clone();
    let before = stable_fingerprint(&probe)?;
    let error = place_terrain_object(&mut probe, &unit_request("unit.issue734.blocked", 3, 2))
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "unwalkable_unit_placement_blocked",
        "before": {"fingerprint": before, "object_count": document.objects.len(), "target_cell": "terrain_cell_3_2"},
        "after": {
            "fingerprint": stable_fingerprint(&probe)?,
            "object_count": probe.objects.len(),
            "unchanged": before == stable_fingerprint(&probe)?,
            "error": error
        }
    }))
}

fn edge_out_of_bounds_clamped(
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut probe = document.clone();
    let report = place_terrain_object(
        &mut probe,
        &TerrainObjectPlacementRequest {
            object_id: "doodad.issue734.clamped".to_string(),
            kind: TerrainPlacementKind::Doodad,
            type_id: "doodad.marker".to_string(),
            target_tile_x: -20,
            target_tile_z: 99,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: true,
            reject_unwalkable_ground: false,
            owner: None,
        },
    )?;
    Ok(json!({
        "edge": "out_of_bounds_clamped",
        "before": {"requested_tile": [-20, 99]},
        "after": {"report": report, "expected_placed_tile": [0, 5]}
    }))
}

fn edge_undo_restores(
    before: &EditorDocument,
    after: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut history = UndoRedoStack::new(4)?;
    history.record(
        "issue734 terrain paint/place",
        before.clone(),
        after.clone(),
    )?;
    let mut probe = after.clone();
    let undo_label = history.undo(&mut probe)?;
    let before_fingerprint = stable_fingerprint(before)?;
    let after_undo = stable_fingerprint(&probe)?;
    Ok(json!({
        "edge": "undo_restores_terrain_edit",
        "before": {"fingerprint": before_fingerprint},
        "after": {
            "undo_label": undo_label,
            "fingerprint": after_undo,
            "restored": before_fingerprint == after_undo,
            "object_count": probe.objects.len(),
            "region_count": probe.regions.len()
        }
    }))
}

fn issue734_world()
-> Result<litd_editor::world_creation::EditorWorldRecord, Box<dyn std::error::Error>> {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = "calyx://world/issue734-terrain-editor".to_string();
    spec.revision_id = "issue734-terrain-editor-rev-1".to_string();
    spec.properties.display_name = "Issue 734 Terrain Editor FSV".to_string();
    spec.properties.width_tiles = 8;
    spec.properties.height_tiles = 6;
    spec.properties.random_height_seed = Some(734);
    spec.properties.playable_bounds = EditorPlayableBounds {
        min_tile_x: 0,
        min_tile_z: 0,
        max_tile_x_exclusive: 8,
        max_tile_z_exclusive: 6,
    };
    Ok(create_editor_world_record(spec)?)
}

fn unit_request(object_id: &str, tile_x: i32, tile_z: i32) -> TerrainObjectPlacementRequest {
    TerrainObjectPlacementRequest {
        object_id: object_id.to_string(),
        kind: TerrainPlacementKind::Unit,
        type_id: "unit.footman".to_string(),
        target_tile_x: tile_x,
        target_tile_z: tile_z,
        yaw_degrees: 90.0,
        clamp_to_playable_bounds: false,
        reject_unwalkable_ground: true,
        owner: Some("player_1".to_string()),
    }
}

fn sync_record(
    record: &mut litd_editor::world_creation::EditorWorldRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.terrain_cell_count = record.document.terrain_cells.len();
    record.default_tile_count = record
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
        .count();
    record.summary = format!(
        "issue734 terrain editor cells={} objects={} regions={} fingerprint={}",
        record.terrain_cell_count,
        record.document.objects.len(),
        record.document.regions.len(),
        record.document_fingerprint
    );
    Ok(())
}

fn pathing_grid(document: &EditorDocument) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let max_x = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_x)
        .max()
        .unwrap_or(0);
    let max_z = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_z)
        .max()
        .unwrap_or(0);
    let mut rows = Vec::new();
    for z in 0..=max_z {
        let mut row = String::new();
        for x in 0..=max_x {
            let cell = document
                .terrain_cells
                .values()
                .find(|cell| cell.tile_x == x && cell.tile_z == z)
                .ok_or_else(|| format!("missing cell {x},{z}"))?;
            let flags = effective_pathing_for_cell(document, &cell.id)?;
            row.push(if flags.allows(TerrainPathMovement::GroundUnit) {
                'W'
            } else {
                'B'
            });
        }
        rows.push(row);
    }
    Ok(rows)
}

fn write_and_read_world(
    aster_dir: &Path,
    record: &litd_editor::world_creation::EditorWorldRecord,
) -> Result<WorldAsterReadback, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn read_document(path: &Path) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let document: EditorDocument = ron::from_str(&text)?;
    document.validate()?;
    Ok(document)
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({"seq": readback.seq, "graph_rows": readback.graph_rows, "bytes": readback.bytes.len()})
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&fs::read(path)?))
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn write_document_with_final_state(
    path: &Path,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("unable to stabilize embedded FSV artifact length".into())
}
