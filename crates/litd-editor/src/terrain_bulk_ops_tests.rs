use std::collections::{BTreeMap, BTreeSet};

use litd_engine::math::{Transform, Vec3};

use crate::{
    history::{EditorDocument, EditorObject, UndoRedoStack, stable_fingerprint},
    terrain_variation::TerrainTileVariation,
};

use super::{
    EditorTerrainCell, OBJECT_TYPE_ID_PROPERTY, TerrainBulkCatalog, TerrainBulkScope,
    adjust_cliff_levels, replace_cliff_type, replace_doodads, replace_tiles, replace_units,
    reset_height_field, terrain_bulk_summary,
};

#[test]
fn replace_tiles_updates_cells_and_variations_from_catalog() {
    let mut document = fixture_document();
    let before = stable_fingerprint(&document).unwrap();

    let report = replace_tiles(
        &mut document,
        &fixture_catalog(),
        "tileset.grass",
        "tileset.snow",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap();

    assert_eq!(report.matched_count, 2);
    assert_ne!(report.after_fingerprint, before);
    assert_eq!(document.terrain_cells["cell_a"].tile_id, "tileset.snow");
    assert_eq!(document.terrain_cells["cell_b"].tile_id, "tileset.snow");
    assert_eq!(document.terrain_cells["cell_c"].tile_id, "tileset.dirt");
    assert_eq!(
        document.terrain_tile_variations["var_cell_a"].tileset_id,
        "tileset.snow"
    );
    assert_eq!(
        document.terrain_tile_variations["var_cell_a"].variation_count,
        2
    );
    println!(
        "replace_tiles before={} after={} summary={}",
        report.before_fingerprint,
        report.after_fingerprint,
        terrain_bulk_summary(&document)
    );
}

#[test]
fn global_ops_replace_cliffs_doodads_units_adjust_and_reset() {
    let mut document = fixture_document();

    let cliff = replace_cliff_type(
        &mut document,
        &fixture_catalog(),
        "cliff.grass",
        "cliff.snow",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap();
    let doodads = replace_doodads(
        &mut document,
        &fixture_catalog(),
        "doodad.tree",
        "doodad.rock",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap();
    let units = replace_units(
        &mut document,
        &fixture_catalog(),
        "unit.footman",
        "unit.grunt",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap();
    let cliffs = adjust_cliff_levels(&mut document, 2, &TerrainBulkScope::WholeMap).unwrap();
    let reset = reset_height_field(&mut document, 0.0, &TerrainBulkScope::WholeMap).unwrap();

    assert_eq!(cliff.matched_count, 2);
    assert_eq!(doodads.matched_count, 2);
    assert_eq!(units.matched_count, 2);
    assert_eq!(cliffs.matched_count, 3);
    assert_eq!(reset.matched_count, 3);
    assert_eq!(document.objects["unit_a"].properties["owner"], "player_1");
    assert_eq!(document.objects["unit_a"].properties["facing"], "90");
    assert_eq!(
        document.objects["unit_a"].properties[OBJECT_TYPE_ID_PROPERTY],
        "unit.grunt"
    );
    assert!(
        document
            .terrain_cells
            .values()
            .all(|cell| cell.height == 0.0)
    );
    assert_eq!(document.terrain_cells["cell_a"].cliff_level, 3);
    println!(
        "global_ops cliff={} doodads={} units={} cliffs={} reset={}",
        cliff.after_fingerprint,
        doodads.after_fingerprint,
        units.after_fingerprint,
        cliffs.after_fingerprint,
        reset.after_fingerprint
    );
}

#[test]
fn selection_scope_changes_only_selected_cells_and_objects() {
    let mut document = fixture_document();
    let scope = TerrainBulkScope::Selection {
        terrain_cell_ids: BTreeSet::from(["cell_a".to_string()]),
        object_ids: BTreeSet::from(["unit_a".to_string(), "doodad_a".to_string()]),
    };

    replace_tiles(
        &mut document,
        &fixture_catalog(),
        "tileset.grass",
        "tileset.snow",
        &scope,
    )
    .unwrap();
    replace_units(
        &mut document,
        &fixture_catalog(),
        "unit.footman",
        "unit.grunt",
        &scope,
    )
    .unwrap();

    assert_eq!(document.terrain_cells["cell_a"].tile_id, "tileset.snow");
    assert_eq!(document.terrain_cells["cell_b"].tile_id, "tileset.grass");
    assert_eq!(
        document.objects["unit_a"].properties[OBJECT_TYPE_ID_PROPERTY],
        "unit.grunt"
    );
    assert_eq!(
        document.objects["unit_b"].properties[OBJECT_TYPE_ID_PROPERTY],
        "unit.footman"
    );
    println!("selection_after={}", terrain_bulk_summary(&document));
}

#[test]
fn invalid_target_and_no_match_fail_without_mutating() {
    let mut document = fixture_document();
    let before = stable_fingerprint(&document).unwrap();

    let missing_target = replace_tiles(
        &mut document,
        &fixture_catalog(),
        "tileset.grass",
        "tileset.missing",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap_err();
    let no_match = replace_units(
        &mut document,
        &fixture_catalog(),
        "unit.grunt",
        "unit.footman",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap_err();
    let after = stable_fingerprint(&document).unwrap();

    assert_eq!(before, after);
    assert_eq!(missing_target.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert_eq!(no_match.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    println!("missing_target={missing_target} no_match={no_match} before_after={after}");
}

#[test]
fn undo_restores_prior_bulk_operation_state() {
    let before = fixture_document();
    let mut after = before.clone();
    let report = replace_units(
        &mut after,
        &fixture_catalog(),
        "unit.footman",
        "unit.grunt",
        &TerrainBulkScope::WholeMap,
    )
    .unwrap();
    let mut history = UndoRedoStack::new(4).unwrap();
    history
        .record(
            "replace units footman to grunt",
            before.clone(),
            after.clone(),
        )
        .unwrap();

    let mut document = after;
    assert_eq!(
        history.undo(&mut document).unwrap(),
        Some("replace units footman to grunt".to_string())
    );
    assert_eq!(document, before);
    assert_eq!(
        history.redo(&mut document).unwrap(),
        Some("replace units footman to grunt".to_string())
    );
    assert_eq!(
        document.objects["unit_a"].properties[OBJECT_TYPE_ID_PROPERTY],
        "unit.grunt"
    );
    println!(
        "undo_redo before={} after={}",
        report.before_fingerprint, report.after_fingerprint
    );
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/bulk-terrain", "rev-1269").unwrap();
    for cell in [
        EditorTerrainCell::new("cell_a", 0, 0, "tileset.grass", "cliff.grass", 1, 8.0),
        EditorTerrainCell::new("cell_b", 1, 0, "tileset.grass", "cliff.grass", 0, 7.0),
        EditorTerrainCell::new("cell_c", 0, 1, "tileset.dirt", "cliff.dirt", -1, 6.5),
    ] {
        document.upsert_terrain_cell(cell).unwrap();
    }
    for variation in [
        TerrainTileVariation::new("var_cell_a", 0, 0, "tileset.grass", 4, 1, 12_690),
        TerrainTileVariation::new("var_cell_b", 1, 0, "tileset.grass", 4, 2, 12_690),
        TerrainTileVariation::new("var_cell_c", 0, 1, "tileset.dirt", 3, 0, 12_690),
    ] {
        document.upsert_terrain_tile_variation(variation).unwrap();
    }
    for object in fixture_objects() {
        document.upsert_object(object).unwrap();
    }
    document.validate().unwrap();
    document
}

fn fixture_objects() -> Vec<EditorObject> {
    vec![
        typed_object(
            "doodad_a",
            "Doodad",
            "doodad.tree",
            Vec3::new(1.0, 0.0, 1.0),
        ),
        typed_object(
            "doodad_b",
            "Doodad",
            "doodad.tree",
            Vec3::new(3.0, 0.0, 1.0),
        ),
        typed_object("unit_a", "Unit", "unit.footman", Vec3::new(4.0, 0.0, 2.0))
            .with_property("owner", "player_1")
            .with_property("facing", "90"),
        typed_object("unit_b", "Unit", "unit.footman", Vec3::new(5.0, 0.0, 2.0))
            .with_property("owner", "player_2")
            .with_property("facing", "180"),
    ]
}

fn typed_object(id: &str, kind: &str, type_id: &str, translation: Vec3) -> EditorObject {
    EditorObject::new(id, kind, None, Transform::from_translation(translation))
        .with_property(OBJECT_TYPE_ID_PROPERTY, type_id)
}

fn fixture_catalog() -> TerrainBulkCatalog {
    TerrainBulkCatalog::new(
        BTreeMap::from([
            ("tileset.grass".to_string(), 4),
            ("tileset.dirt".to_string(), 3),
            ("tileset.snow".to_string(), 2),
        ]),
        BTreeSet::from([
            "cliff.grass".to_string(),
            "cliff.dirt".to_string(),
            "cliff.snow".to_string(),
        ]),
        BTreeSet::from(["doodad.tree".to_string(), "doodad.rock".to_string()]),
        BTreeSet::from(["unit.footman".to_string(), "unit.grunt".to_string()]),
    )
}
