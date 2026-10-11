use litd_engine::math::Vec3;

use crate::{
    history::stable_fingerprint,
    terrain_authoring::{
        TerrainObjectPlacementRequest, TerrainPlacementKind, TerrainRegionPlacementRequest,
        place_terrain_object, place_terrain_region,
    },
    terrain_pathing::{TerrainPathingPaintMode, paint_pathing_cells},
    world_creation::{EditorWorldCreationSpec, create_editor_world_record},
};

#[test]
fn places_unit_and_region_on_terrain_cells() {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())
        .expect("world should create");
    let unit = place_terrain_object(
        &mut record.document,
        &TerrainObjectPlacementRequest {
            object_id: "unit.test.footman".to_string(),
            kind: TerrainPlacementKind::Unit,
            type_id: "unit.footman".to_string(),
            target_tile_x: 2,
            target_tile_z: 3,
            yaw_degrees: 90.0,
            clamp_to_playable_bounds: false,
            reject_unwalkable_ground: true,
            owner: Some("player_1".to_string()),
        },
    )
    .expect("unit should place");
    let region = place_terrain_region(
        &mut record.document,
        &TerrainRegionPlacementRequest {
            region_id: "region.test.spawn".to_string(),
            name: "Spawn".to_string(),
            center_tile_x: 4,
            center_tile_z: 5,
            half_extent_tiles_x: 2.0,
            half_extent_tiles_z: 1.0,
            clamp_to_playable_bounds: false,
        },
    )
    .expect("region should place");
    let object = &record.document.objects["unit.test.footman"];

    assert_eq!(unit.terrain_cell_id, "terrain_cell_2_3");
    assert_eq!(unit.translation[0], 2.5);
    assert_eq!(object.kind, "Unit");
    assert_eq!(object.properties["owner"], "player_1");
    assert_eq!(region.terrain_cell_id, "terrain_cell_4_5");
    assert_eq!(
        record.document.regions["region.test.spawn"].center,
        Vec3::new(4.5, region.translation[1], 5.5)
    );
}

#[test]
fn unwalkable_ground_unit_placement_fails_without_mutation() {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())
        .expect("world should create");
    paint_pathing_cells(
        &mut record.document,
        ["terrain_cell_1_1".to_string()],
        TerrainPathingPaintMode::Blocked,
    )
    .expect("pathing paint should work");
    let before = stable_fingerprint(&record.document).expect("fingerprint");
    let error = place_terrain_object(
        &mut record.document,
        &TerrainObjectPlacementRequest {
            object_id: "unit.test.blocked".to_string(),
            kind: TerrainPlacementKind::Unit,
            type_id: "unit.footman".to_string(),
            target_tile_x: 1,
            target_tile_z: 1,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: false,
            reject_unwalkable_ground: true,
            owner: None,
        },
    )
    .expect_err("blocked unit placement must fail")
    .to_string();
    let after = stable_fingerprint(&record.document).expect("fingerprint");

    assert_eq!(before, after);
    assert!(error.contains("editor.terrain_authoring.object.pathing"));
    assert!(!record.document.objects.contains_key("unit.test.blocked"));
}

#[test]
fn out_of_bounds_target_clamps_to_existing_terrain_cell() {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())
        .expect("world should create");
    let report = place_terrain_object(
        &mut record.document,
        &TerrainObjectPlacementRequest {
            object_id: "start.test.clamped".to_string(),
            kind: TerrainPlacementKind::StartLocation,
            type_id: "start.player_1".to_string(),
            target_tile_x: -99,
            target_tile_z: 999,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: true,
            reject_unwalkable_ground: false,
            owner: Some("player_1".to_string()),
        },
    )
    .expect("clamped start location should place");

    assert!(report.clamped);
    assert_eq!(report.placed_tile, [0, 11]);
    assert_eq!(report.terrain_cell_id, "terrain_cell_0_11");
    assert_eq!(
        record.document.objects["start.test.clamped"].kind,
        "StartLocation"
    );
}
