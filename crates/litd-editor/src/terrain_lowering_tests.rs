use litd_engine::{
    math::Vec3,
    nav::{NavCellCoord, bake_navmesh, find_path},
    render::terrain::TerrainRuntime,
};

use crate::{
    history::EditorDocument,
    terrain_authoring::{
        TerrainObjectPlacementRequest, TerrainPlacementKind, place_terrain_object,
    },
    terrain_pathing::{TerrainPathingPaintMode, paint_pathing_cells},
    world_creation::{EditorWorldCreationSpec, create_editor_world_record},
};

use super::*;

fn world_4x4() -> EditorDocument {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = "calyx://world/terrain-lowering-test".to_string();
    spec.revision_id = "terrain-lowering-test-rev-1".to_string();
    spec.properties.display_name = "Terrain lowering test".to_string();
    spec.properties.width_tiles = 4;
    spec.properties.height_tiles = 4;
    spec.properties.playable_bounds.max_tile_x_exclusive = 4;
    spec.properties.playable_bounds.max_tile_z_exclusive = 4;
    spec.properties.random_height_seed = None;
    create_editor_world_record(spec).unwrap().document
}

#[test]
fn height_field_lowering_is_deterministic_and_reflects_cliffs() {
    let mut document = world_4x4();
    // Author a cliff on one column: level 3 (raised) at x==2.
    for z in 0..4 {
        let cell = document
            .terrain_cells
            .get_mut(&format!("terrain_cell_2_{z}"))
            .unwrap();
        cell.cliff_level = 3;
    }
    let params = HeightFieldLowerParams::default();
    let a = lower_height_field(&document, &params).unwrap();
    let b = lower_height_field(&document, &params).unwrap();
    assert_eq!(
        a.fingerprint, b.fingerprint,
        "lowering must be deterministic"
    );

    let size = params.leaf_nodes_per_side * params.leaf_sample_span + 1;
    assert_eq!(a.field.size, size);
    assert_eq!(a.sample_count, (size * size) as usize);
    // The cliff (level 3 * step 4.0 = 12.0) is the max height; flat ground is 0.
    assert!(
        (a.max_height - 12.0).abs() < 1e-3,
        "cliff raised max height"
    );
    assert!(a.min_height.abs() < 1e-3, "flat ground stayed at 0");

    // Runtime consumption: the CDLOD runtime builds + selects a frame from it.
    let mut runtime = TerrainRuntime::new(&a.field, a.config.clone()).unwrap();
    let stats = runtime.select_frame(Vec3::new(0.0, 2.0, 0.0), None).stats;
    assert!(stats.tile_count >= 1);
    assert_eq!(
        stats.total_vertices,
        stats.tile_count as u64 * a.config.tile_vertex_count() as u64
    );
}

#[test]
fn cliffed_terrain_refines_to_more_tiles_than_flat() {
    let flat = world_4x4();
    let params = HeightFieldLowerParams::default();
    let flat_lowered = lower_height_field(&flat, &params).unwrap();
    let flat_tiles = TerrainRuntime::new(&flat_lowered.field, flat_lowered.config.clone())
        .unwrap()
        .select_frame(Vec3::new(0.0, 2.0, 0.0), None)
        .stats
        .tile_count;

    let mut cliffed = world_4x4();
    for z in 0..4 {
        cliffed
            .terrain_cells
            .get_mut(&format!("terrain_cell_2_{z}"))
            .unwrap()
            .cliff_level = 3;
    }
    let cliffed_lowered = lower_height_field(&cliffed, &params).unwrap();
    let cliffed_tiles = TerrainRuntime::new(&cliffed_lowered.field, cliffed_lowered.config.clone())
        .unwrap()
        .select_frame(Vec3::new(0.0, 2.0, 0.0), None)
        .stats
        .tile_count;
    assert!(
        cliffed_tiles > flat_tiles,
        "vertical error at the cliff must force more subdivision ({cliffed_tiles} vs {flat_tiles})"
    );
}

#[test]
fn tile_splat_palette_reconstructs_authored_tiles() {
    let mut document = world_4x4();
    // Mixed tileset: repaint a 2x2 block with a second tile id.
    for (x, z) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        document
            .terrain_cells
            .get_mut(&format!("terrain_cell_{x}_{z}"))
            .unwrap()
            .tile_id = "tile.rock.dark".to_string();
    }
    let splat = lower_tile_splat_map(&document).unwrap();
    assert!(
        splat.palette.len() >= 2,
        "mixed tileset yields a palette >1"
    );
    assert_eq!(splat.cell_count, 16);
    // Readback: every authored cell reconstructs from palette[indices].
    for cell in document.terrain_cells.values() {
        assert_eq!(
            splat.tile_id_at(cell.tile_x, cell.tile_z),
            Some(cell.tile_id.as_str()),
            "splat index must round-trip the authored tile id"
        );
    }
    let again = lower_tile_splat_map(&document).unwrap();
    assert_eq!(splat.fingerprint, again.fingerprint);
}

#[test]
fn nav_lowering_bakes_and_astar_routes_around_a_blocker() {
    let mut document = world_4x4();
    // Block the whole z=1 row except a walkable bridge at x=0.
    for x in 1..4 {
        paint_pathing_cells(
            &mut document,
            [format!("terrain_cell_{x}_1")],
            TerrainPathingPaintMode::Blocked,
        )
        .unwrap();
    }
    let lowered = lower_nav_bake_input(&document, 4.0).unwrap();
    assert_eq!(lowered.cell_count, 16);
    assert_eq!(lowered.blocked_count, 3);

    let navmesh = bake_navmesh(&lowered.input).unwrap();
    // A* from (0,0) to (0,3): the only crossing of the blocked row is the bridge at x=0.
    let path = find_path(&navmesh, NavCellCoord::new(0, 0), NavCellCoord::new(0, 3)).unwrap();
    assert!(!path.nodes.is_empty(), "A* must find the bridge route");
    // The path must stay on x=0 across z=1 (the only unblocked crossing).
    assert!(
        path.nodes.iter().any(|n| n.x == 0 && n.z == 1),
        "route must cross the blocked row via the x=0 bridge"
    );

    // A cell fully behind a blocked wall is unreachable.
    let mut walled = world_4x4();
    for x in 0..4 {
        paint_pathing_cells(
            &mut walled,
            [format!("terrain_cell_{x}_1")],
            TerrainPathingPaintMode::Blocked,
        )
        .unwrap();
    }
    let walled_mesh = bake_navmesh(&lower_nav_bake_input(&walled, 4.0).unwrap().input).unwrap();
    let blocked = find_path(
        &walled_mesh,
        NavCellCoord::new(0, 0),
        NavCellCoord::new(0, 3),
    );
    assert!(
        blocked.is_err(),
        "a fully walled-off goal must be unreachable"
    );
}

#[test]
fn doodad_placement_becomes_a_nav_blocker() {
    let mut document = world_4x4();
    place_terrain_object(
        &mut document,
        &TerrainObjectPlacementRequest {
            object_id: "doodad.tree.1".to_string(),
            kind: TerrainPlacementKind::Doodad,
            type_id: "doodad.tree".to_string(),
            target_tile_x: 2,
            target_tile_z: 2,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: false,
            reject_unwalkable_ground: false,
            owner: None,
        },
    )
    .unwrap();
    let lowered = lower_nav_bake_input(&document, 4.0).unwrap();
    assert_eq!(
        lowered.blocker_count, 1,
        "the doodad footprint is one blocker"
    );
    let blocker = &lowered.input.static_blockers[0];
    assert_eq!((blocker.min_x, blocker.min_z), (2, 2));
    // The baked mesh records the blocker against the underlying cell.
    let navmesh = bake_navmesh(&lowered.input).unwrap();
    let cell = navmesh.cells.iter().find(|c| c.x == 2 && c.z == 2).unwrap();
    assert!(
        !cell.static_blocker_ids.is_empty(),
        "the doodad's cell must carry its static blocker id"
    );
}

#[test]
fn non_dense_grid_fails_loud() {
    let mut document = world_4x4();
    document.terrain_cells.remove("terrain_cell_3_3");
    // validate() still passes (the map is internally consistent), but lowering the
    // heightfield needs a dense rectangle and must fail loud on the hole.
    let err = lower_height_field(&document, &HeightFieldLowerParams::default()).unwrap_err();
    match err {
        litd_engine::EngineError::UiRenderInvalidConfig { field, .. } => {
            assert!(field.contains("density"), "unexpected field {field}");
        }
        other => panic!("expected a density error, got {other:?}"),
    }
}
