use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::stable_fingerprint,
    terrain_pathing::{
        TerrainPathMovement, TerrainPathingFlags, TerrainPathingPaintMode,
        effective_pathing_for_cell, paint_pathing_cells, query_path_between_cells,
        set_global_tile_pathing, terrain_pathing_summary,
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
        "LITD_TERRAIN_PATHING_FSV",
        "issue1213_terrain_pathing",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1213 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");

    let mut record = test_world("blocked");
    block_non_corridor_cells(&mut record.document)?;
    sync_record(&mut record, "corridor_setup")?;
    let before_query = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )?;
    let center_before = effective_pathing_for_cell(&record.document, "terrain_cell_1_0")?;
    let paint_report = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Blocked,
    )?;
    sync_record(&mut record, "blocked_center")?;
    let blocked_state = write_read_record(&root, &aster_dir, &record)?;
    let blocked_record = parse_world_record(&blocked_state.readback.bytes)?;
    let blocked_query = query_path_between_cells(
        &blocked_record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )?;

    let erase_edge = erase_edge(&root, &aster_dir, &record)?;
    let global_edge = global_tile_edge(&root, &aster_dir)?;
    let bridge_edge = bridge_edge(&root, &aster_dir)?;

    let mut snapshot = json!({
        "schema": "litd.issue1213.terrain_pathing_fsv.v1",
        "issue": 1213,
        "source_of_truth": "Aster world record readback plus EditorDocument terrain_pathing maps and path query",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "corridor_query": query_summary(&before_query),
            "center_effective_flags": center_before,
            "summary": terrain_pathing_summary(&record.document)
        },
        "happy_path": {
            "after": {
                "paint_report": paint_report,
                "state_path": blocked_state.state_path,
                "state_sha256": file_sha256(&blocked_state.state_path)?,
                "readback": readback_summary(&blocked_state.readback),
                "center_effective_flags": effective_pathing_for_cell(&blocked_record.document, "terrain_cell_1_0")?,
                "query": query_summary(&blocked_query),
                "passed": !blocked_query.path_found
            }
        },
        "edges": [
            erase_edge,
            global_edge,
            bridge_edge
        ],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote terrain pathing FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!("blocked query {}", snapshot["happy_path"]["after"]["query"]);
    println!("erase edge {}", snapshot["edges"][0]["after"]);
    println!("global edge {}", snapshot["edges"][1]["after"]);
    println!("bridge edge {}", snapshot["edges"][2]["after"]);
    Ok(())
}

struct StoredRecord {
    state_path: PathBuf,
    readback: WorldAsterReadback,
}

fn erase_edge(
    root: &Path,
    aster_dir: &Path,
    blocked_record: &EditorWorldRecord,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = blocked_record.clone();
    retag_record(&mut record, "erase");
    let before_query = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )?;
    let report = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Erase,
    )?;
    sync_record(&mut record, "erase_center")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let after_query = query_path_between_cells(
        &parsed.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )?;
    Ok(json!({
        "name": "paint then erase restores walkable",
        "before": {"query": query_summary(&before_query)},
        "after": {
            "report": report,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "query": query_summary(&after_query),
            "center_override_present": parsed.document.terrain_pathing_overrides.contains_key("terrain_cell_1_0")
        }
    }))
}

fn global_tile_edge(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("global");
    let tile_id = record.document.terrain_cells["terrain_cell_0_0"]
        .tile_id
        .clone();
    let report = set_global_tile_pathing(
        &mut record.document,
        &tile_id,
        TerrainPathingFlags::blocked(),
    )?;
    sync_record(&mut record, "global_blocked")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let blocked_count = parsed
        .document
        .terrain_cells
        .keys()
        .filter(|cell_id| {
            !effective_pathing_for_cell(&parsed.document, cell_id)
                .unwrap()
                .allows(TerrainPathMovement::GroundUnit)
        })
        .count();
    let query = query_path_between_cells(
        &parsed.document,
        "terrain_cell_0_0",
        "terrain_cell_0_1",
        TerrainPathMovement::GroundUnit,
    )?;
    Ok(json!({
        "name": "global tile-pathing change applies everywhere",
        "before": {"tile_id": tile_id, "cell_count": record.document.terrain_cells.len()},
        "after": {
            "report": report,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "blocked_count": blocked_count,
            "query": query_summary(&query)
        }
    }))
}

fn bridge_edge(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = test_world("bridge");
    record
        .document
        .terrain_cells
        .get_mut("terrain_cell_1_0")
        .ok_or("missing bridge cell")?
        .tile_id = "tile.water.deep".to_string();
    set_global_tile_pathing(
        &mut record.document,
        "tile.water.deep",
        TerrainPathingFlags::blocked(),
    )?;
    paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Walkable,
    )?;
    sync_record(&mut record, "bridge_walkable")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let query = query_path_between_cells(
        &parsed.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )?;
    Ok(json!({
        "name": "bridge over blocked water walkable",
        "before": {"water_cell": "terrain_cell_1_0", "water_tile": "tile.water.deep"},
        "after": {
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "bridge_flags": effective_pathing_for_cell(&parsed.document, "terrain_cell_1_0")?,
            "query": query_summary(&query)
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
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.terrain_cell_count = record.document.terrain_cells.len();
    record.default_tile_count = record
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
        .count();
    record.summary = format!(
        "world={} issue1213={} pathing={} fingerprint={}",
        record.spec.properties.display_name,
        label,
        terrain_pathing_summary(&record.document),
        record.document_fingerprint
    );
    Ok(())
}

fn block_non_corridor_cells(
    document: &mut litd_editor::history::EditorDocument,
) -> Result<(), litd_engine::EngineError> {
    let corridor = ["terrain_cell_0_0", "terrain_cell_1_0", "terrain_cell_2_0"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let ids = document
        .terrain_cells
        .keys()
        .filter(|id| !corridor.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    paint_pathing_cells(document, ids, TerrainPathingPaintMode::Blocked)?;
    Ok(())
}

fn test_world(label: &str) -> EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1213-{label}");
    spec.revision_id = format!("issue1213-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1213 {label}");
    spec.properties.description =
        format!("Known terrain pathing settings for issue 1213 {label} FSV");
    spec.properties.width_tiles = 4;
    spec.properties.height_tiles = 4;
    spec.properties.playable_bounds.max_tile_x_exclusive = 4;
    spec.properties.playable_bounds.max_tile_z_exclusive = 4;
    let mut record = litd_editor::world_creation::create_editor_world_record(spec).unwrap();
    record.source_issue = 1213;
    record
}

fn retag_record(record: &mut EditorWorldRecord, label: &str) {
    record.spec.calyx_document_id = format!("calyx://world/issue1213-{label}");
    record.spec.revision_id = format!("issue1213-{label}-rev-1");
    record.spec.properties.display_name = format!("Issue 1213 {label}");
    record.spec.properties.description =
        format!("Known terrain pathing settings for issue 1213 {label} FSV");
    record.source_issue = 1213;
}

fn query_summary(
    query: &litd_editor::terrain_pathing::TerrainPathQueryReport,
) -> serde_json::Value {
    json!({
        "movement": format!("{:?}", query.movement),
        "start": query.start_cell_id,
        "goal": query.goal_cell_id,
        "path_found": query.path_found,
        "path": query.path_cell_ids,
        "visited_count": query.visited_count,
        "blocked_cell_ids": query.blocked_cell_ids
    })
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
                return Err("terrain pathing FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("terrain pathing FSV artifact byte-count stabilization failed".into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
