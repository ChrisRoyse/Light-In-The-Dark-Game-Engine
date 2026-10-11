use crate::world_creation::{EditorWorldCreationSpec, create_editor_world_record};

use super::*;

#[test]
fn flood_fill_changes_only_contiguous_tile_region() {
    let mut record = test_world("flood", 6, 4);
    paint_tiles(
        &mut record.document,
        &[(1, 1), (2, 1), (1, 2), (2, 2), (5, 3)],
        "tile.sand",
    );

    let report = flood_fill_tile(&mut record.document, "terrain_cell_1_1", "tile.moss").unwrap();
    println!(
        "issue1211 flood changed={} ids={:?}",
        report.changed_count, report.changed_cell_ids
    );

    assert_eq!(report.changed_count, 4);
    assert_eq!(
        report.changed_cell_ids,
        vec![
            "terrain_cell_1_1",
            "terrain_cell_1_2",
            "terrain_cell_2_1",
            "terrain_cell_2_2"
        ]
    );
    assert_eq!(
        record.document.terrain_cells["terrain_cell_5_3"].tile_id,
        "tile.sand"
    );
    assert_eq!(
        record.document.terrain_cells["terrain_cell_0_0"].tile_id,
        "tile.ashen_grass.default"
    );
}

#[test]
fn magic_wand_selects_contiguous_patch_and_reports_boundaries() {
    let mut record = test_world("wand", 6, 4);
    paint_tiles(
        &mut record.document,
        &[(1, 1), (2, 1), (1, 2), (2, 2), (5, 3)],
        "tile.sand",
    );

    let report = magic_wand_select(
        &record.document,
        "terrain_cell_1_1",
        TerrainMagicWandMode::SameTile,
    )
    .unwrap();
    let selection = report.selection_set();
    println!(
        "issue1211 wand selected={} boundary={:?}",
        selection.summary(),
        report.boundary_cell_ids
    );

    assert_eq!(report.selected_count, 4);
    assert!(selection.selected_ids.contains("terrain_cell_2_2"));
    assert!(!selection.selected_ids.contains("terrain_cell_5_3"));
    assert!(
        report
            .boundary_cell_ids
            .contains(&"terrain_cell_0_1".to_string())
    );
}

#[test]
fn height_magic_wand_uses_tolerance_without_crossing_height_boundary() {
    let mut record = test_world("height-wand", 5, 4);
    for (x, z, height) in [
        (1, 1, 3.0),
        (2, 1, 3.05),
        (1, 2, 2.95),
        (2, 2, 4.0),
        (3, 1, 3.04),
    ] {
        record
            .document
            .terrain_cells
            .get_mut(&format!("terrain_cell_{x}_{z}"))
            .unwrap()
            .height = height;
    }

    let report = magic_wand_select(
        &record.document,
        "terrain_cell_1_1",
        TerrainMagicWandMode::HeightWithin { tolerance: 0.1 },
    )
    .unwrap();
    println!(
        "issue1211 height wand selected={:?} boundary={:?}",
        report.selected_cell_ids, report.boundary_cell_ids
    );

    assert!(
        report
            .selected_cell_ids
            .contains(&"terrain_cell_1_1".to_string())
    );
    assert!(
        report
            .selected_cell_ids
            .contains(&"terrain_cell_2_1".to_string())
    );
    assert!(
        report
            .selected_cell_ids
            .contains(&"terrain_cell_1_2".to_string())
    );
    assert!(
        !report
            .selected_cell_ids
            .contains(&"terrain_cell_2_2".to_string())
    );
}

#[test]
fn brush_size_one_and_linear_falloff_mutate_expected_heights() {
    let mut record = test_world("brush", 5, 5);
    let center = record.document.terrain_cells["terrain_cell_2_2"].height;
    let single = TerrainBrushSpec {
        center_tile_x: 2,
        center_tile_z: 2,
        size_tiles: 1,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::RaiseHeight { delta: 2.0 },
    };
    let single_report = apply_terrain_brush(&mut record.document, &single).unwrap();
    assert_eq!(single_report.mutation.changed_count, 1);
    assert_approx(
        record.document.terrain_cells["terrain_cell_2_2"].height,
        center + 2.0,
    );

    let before_corner = record.document.terrain_cells["terrain_cell_0_0"].height;
    let falloff = TerrainBrushSpec {
        center_tile_x: 2,
        center_tile_z: 2,
        size_tiles: 5,
        shape: TerrainBrushShape::Circle,
        falloff: TerrainBrushFalloff::Linear,
        operation: TerrainBrushOperation::RaiseHeight { delta: 1.0 },
    };
    let falloff_report = apply_terrain_brush(&mut record.document, &falloff).unwrap();
    println!(
        "issue1211 brush size1={} falloff_changed={} min_weight={:.3}",
        single_report.mutation.changed_count,
        falloff_report.mutation.changed_count,
        falloff_report.min_nonzero_weight
    );

    assert!(falloff_report.mutation.changed_count > 1);
    assert_eq!(falloff_report.max_weight, 1.0);
    assert!(
        record.document.terrain_cells["terrain_cell_2_2"].height
            > record.document.terrain_cells["terrain_cell_0_0"].height
    );
    assert_eq!(
        record.document.terrain_cells["terrain_cell_0_0"].height,
        before_corner
    );
}

#[test]
fn large_brush_paints_entire_max_world() {
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
    let report = apply_terrain_brush(&mut record.document, &brush).unwrap();
    println!(
        "issue1211 large brush weighted={} changed={}",
        report.weighted_cells.len(),
        report.mutation.changed_count
    );

    assert_eq!(report.weighted_cells.len(), 128 * 128);
    assert_eq!(report.mutation.changed_count, 128 * 128);
    assert!(
        record
            .document
            .terrain_cells
            .values()
            .all(|cell| cell.tile_id == "tile.large.brush")
    );
}

#[test]
fn invalid_inputs_fail_without_mutation() {
    let mut record = test_world("invalid", 4, 4);
    let before = record.document.clone();
    let error = magic_wand_select(
        &record.document,
        "terrain_cell_missing",
        TerrainMagicWandMode::SameTile,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("editor.terrain_advanced_tools.cell"));

    let brush = TerrainBrushSpec {
        center_tile_x: 0,
        center_tile_z: 0,
        size_tiles: 0,
        shape: TerrainBrushShape::Circle,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::RaiseHeight { delta: 1.0 },
    };
    let brush_error = apply_terrain_brush(&mut record.document, &brush)
        .unwrap_err()
        .to_string();
    println!(
        "issue1211 invalid before_eq_after={} error={}",
        before == record.document,
        brush_error
    );

    assert_eq!(before, record.document);
    assert!(brush_error.contains("editor.terrain_advanced_tools.brush.size"));
}

fn test_world(label: &str, width: u32, height: u32) -> crate::world_creation::EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1211-{label}");
    spec.revision_id = format!("issue1211-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1211 {label}");
    spec.properties.width_tiles = width;
    spec.properties.height_tiles = height;
    spec.properties.playable_bounds.max_tile_x_exclusive = width as i32;
    spec.properties.playable_bounds.max_tile_z_exclusive = height as i32;
    create_editor_world_record(spec).unwrap()
}

fn paint_tiles(
    document: &mut EditorDocument,
    coords: &[(i32, i32)],
    tile_id: impl Into<String> + Clone,
) {
    for (x, z) in coords {
        document
            .terrain_cells
            .get_mut(&format!("terrain_cell_{x}_{z}"))
            .unwrap()
            .tile_id = tile_id.clone().into();
    }
}

fn assert_approx(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_1,
        "actual {actual} expected {expected}"
    );
}
