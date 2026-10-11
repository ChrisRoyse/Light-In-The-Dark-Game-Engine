use std::collections::BTreeSet;

use crate::world_creation::{EditorWorldCreationSpec, create_editor_world_record};

use super::*;

#[test]
fn blocked_paint_prevents_ground_path_through_corridor() {
    let mut record = test_world("blocked-corridor");
    block_non_corridor_cells(&mut record.document).unwrap();
    let before = terrain_pathing_summary(&record.document);

    let report = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Blocked,
    )
    .unwrap();
    let query = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )
    .unwrap();
    println!(
        "issue1213 blocked before={} after={} path_found={} blocked={:?}",
        before, report.after_summary, query.path_found, query.blocked_cell_ids
    );

    assert!(!query.path_found);
    assert!(
        query
            .blocked_cell_ids
            .contains(&"terrain_cell_1_0".to_string())
    );
    assert_eq!(
        record.document.terrain_pathing_overrides["terrain_cell_1_0"].flags,
        TerrainPathingFlags::blocked()
    );
}

#[test]
fn erase_restores_default_walkable_pathing() {
    let mut record = test_world("erase");
    block_non_corridor_cells(&mut record.document).unwrap();
    paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Blocked,
    )
    .unwrap();
    let blocked = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )
    .unwrap();

    let report = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Erase,
    )
    .unwrap();
    let restored = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )
    .unwrap();
    println!(
        "issue1213 erase before_path={} after_path={} path={:?}",
        blocked.path_found, restored.path_found, restored.path_cell_ids
    );

    assert!(!blocked.path_found);
    assert!(restored.path_found);
    assert_eq!(
        restored.path_cell_ids,
        vec!["terrain_cell_0_0", "terrain_cell_1_0", "terrain_cell_2_0"]
    );
    assert!(
        !record
            .document
            .terrain_pathing_overrides
            .contains_key("terrain_cell_1_0")
    );
    assert_eq!(
        report.changed_cell_ids,
        vec!["terrain_cell_1_0".to_string()]
    );
}

#[test]
fn global_tile_pathing_change_applies_to_every_matching_cell() {
    let mut record = test_world("global");
    let tile_id = record.document.terrain_cells["terrain_cell_0_0"]
        .tile_id
        .clone();
    let report = set_global_tile_pathing(
        &mut record.document,
        &tile_id,
        TerrainPathingFlags::blocked(),
    )
    .unwrap();
    let blocked_count = record
        .document
        .terrain_cells
        .keys()
        .filter(|cell_id| {
            !effective_pathing_for_cell(&record.document, cell_id)
                .unwrap()
                .allows(TerrainPathMovement::GroundUnit)
        })
        .count();
    println!(
        "issue1213 global changed={} blocked_count={} summary={}",
        report.changed_cell_ids.len(),
        blocked_count,
        terrain_pathing_summary(&record.document)
    );

    assert_eq!(blocked_count, record.document.terrain_cells.len());
    assert_eq!(
        report.changed_cell_ids.len(),
        record.document.terrain_cells.len()
    );
}

#[test]
fn bridge_override_over_blocked_water_is_walkable() {
    let mut record = test_world("bridge");
    record
        .document
        .terrain_cells
        .get_mut("terrain_cell_1_0")
        .unwrap()
        .tile_id = "tile.water.deep".to_string();
    set_global_tile_pathing(
        &mut record.document,
        "tile.water.deep",
        TerrainPathingFlags::blocked(),
    )
    .unwrap();
    paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_0".to_string()],
        TerrainPathingPaintMode::Walkable,
    )
    .unwrap();
    let query = query_path_between_cells(
        &record.document,
        "terrain_cell_0_0",
        "terrain_cell_2_0",
        TerrainPathMovement::GroundUnit,
    )
    .unwrap();
    println!(
        "issue1213 bridge path_found={} path={:?} bridge_flags={:?}",
        query.path_found,
        query.path_cell_ids,
        effective_pathing_for_cell(&record.document, "terrain_cell_1_0").unwrap()
    );

    assert!(query.path_found);
    assert_eq!(
        query.path_cell_ids,
        vec!["terrain_cell_0_0", "terrain_cell_1_0", "terrain_cell_2_0"]
    );
}

#[test]
fn invalid_pathing_inputs_fail_without_mutating_document() {
    let mut record = test_world("invalid");
    let before = record.document.clone();
    let error = paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_missing".to_string()],
        TerrainPathingPaintMode::Blocked,
    )
    .unwrap_err()
    .to_string();
    println!(
        "issue1213 invalid before_eq_after={} error={}",
        before == record.document,
        error
    );

    assert_eq!(before, record.document);
    assert!(error.contains("editor.terrain_pathing.cell"));

    let mut stale_rule = before.clone();
    stale_rule.terrain_tile_pathing_rules.insert(
        "tile.missing".to_string(),
        EditorTilePathingRule {
            tile_id: "tile.missing".to_string(),
            flags: TerrainPathingFlags::blocked(),
        },
    );
    let stale_error = stale_rule.validate().unwrap_err().to_string();
    assert!(stale_error.contains("editor.terrain_pathing.tile_rule.matches"));
}

fn block_non_corridor_cells(document: &mut EditorDocument) -> Result<(), litd_engine::EngineError> {
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

fn test_world(label: &str) -> crate::world_creation::EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1213-{label}");
    spec.revision_id = format!("issue1213-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1213 {label}");
    spec.properties.width_tiles = 4;
    spec.properties.height_tiles = 4;
    spec.properties.playable_bounds.max_tile_x_exclusive = 4;
    spec.properties.playable_bounds.max_tile_z_exclusive = 4;
    create_editor_world_record(spec).unwrap()
}
