use std::fs;

use crate::{
    asset_products::{
        MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY, MODEL_SOURCE_SHA256_PROPERTY,
        product_from_vault_record,
    },
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, EditorRegion, EditorTerrainStamp, stable_fingerprint},
    terrain_bulk_ops::EditorTerrainCell,
    terrain_variation::{
        TerrainRandomGroup, TerrainRandomGroupMember, TerrainRandomGroupMemberKind,
        TerrainTileVariation,
    },
};
use litd_engine::math::{Transform, Vec3};

use super::*;

const REAL_STATIC_MODEL: &str =
    "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

#[test]
fn script_export_writes_reopens_and_parses_known_project() {
    let document = fixture_document("rev-script-export");
    let root = temp_root("script");
    let script_path = root.join("map.lua");

    let export = export_script(&document, &script_path).unwrap();
    let readback = fs::read_to_string(&script_path).unwrap();
    let parsed = parse_generated_map_script(&readback).unwrap();

    assert_eq!(export.summary, parsed);
    assert_eq!(export.script_sha256, sha256_hex(readback.as_bytes()));
    assert_eq!(parsed.object_count, 2);
    assert_eq!(parsed.region_count, 1);
    assert_eq!(parsed.terrain_stamp_count, 1);
    assert_eq!(parsed.terrain_cell_count, 2);
    assert_eq!(parsed.terrain_variation_count, 2);
    assert_eq!(parsed.random_group_count, 1);
    assert!(readback.contains("doodad_rock"));
    assert!(readback.contains(REAL_STATIC_MODEL));
    assert!(readback.contains("terrain_tile_variations"));
    assert!(readback.contains("tile_spawn_grass"));
    assert!(readback.contains("random_groups"));
    assert!(readback.contains("forest_floor_mix"));
    println!(
        "script_export file={} sha={} fingerprint={} lines={}",
        script_path.display(),
        export.script_sha256,
        parsed.document_fingerprint,
        parsed.line_count
    );
}

#[test]
fn shadow_bake_and_save_map_are_reproducible_and_readable() {
    let document = fixture_document("rev-shadow-save");
    let root = temp_root("shadow");
    let shadow_path = root.join("static_shadows.ron");
    let map_path = root.join("saved_map.ron");

    let receipt = calculate_shadows_and_save_map(
        &document,
        &shadow_path,
        &map_path,
        StaticShadowBakeSettings::default(),
    )
    .unwrap();
    let shadow_readback = load_static_shadow_bake(&shadow_path).unwrap();
    let map_readback = load_saved_map_build(&map_path).unwrap();
    let second = bake_static_shadows(&document, StaticShadowBakeSettings::default()).unwrap();

    validate_static_shadow_bake_for_document(&shadow_readback, &document).unwrap();
    assert_eq!(shadow_readback, receipt.shadow_receipt.bake);
    assert_eq!(map_readback, receipt.saved_map);
    assert_eq!(map_readback.document.terrain_cells.len(), 2);
    assert_eq!(map_readback.document.terrain_tile_variations.len(), 2);
    assert_eq!(map_readback.document.random_groups.len(), 1);
    assert_eq!(
        map_readback
            .document
            .terrain_tile_variations
            .get("tile_spawn_grass")
            .unwrap()
            .variation_index,
        2
    );
    assert_eq!(
        map_readback
            .document
            .random_groups
            .get("forest_floor_mix")
            .unwrap()
            .members
            .len(),
        3
    );
    assert_eq!(shadow_readback.cells, second.cells);
    assert_eq!(shadow_readback.cells_sha256, second.cells_sha256);
    assert_eq!(shadow_readback.casters.len(), 2);
    assert!(shadow_readback.nonzero_cell_count() > 0);
    println!(
        "shadow_save shadow={} shadow_sha={} map={} map_sha={} cells_sha={} nonzero={}",
        shadow_path.display(),
        receipt.shadow_receipt.artifact_sha256,
        map_path.display(),
        receipt.artifact_sha256,
        shadow_readback.cells_sha256,
        shadow_readback.nonzero_cell_count()
    );
}

#[test]
fn stale_shadow_bake_fails_when_terrain_changes() {
    let document = fixture_document("rev-stale-shadow");
    let bake = bake_static_shadows(&document, StaticShadowBakeSettings::default()).unwrap();
    let before = stable_fingerprint(&document).unwrap();
    let mut changed = document.clone();
    changed
        .terrain_stamps
        .get_mut("terrain_ramp")
        .unwrap()
        .height_delta = 4.0;
    let after = stable_fingerprint(&changed).unwrap();
    let error = validate_static_shadow_bake_for_document(&bake, &changed).unwrap_err();

    assert_ne!(before, after);
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("shadow.stale"));
    println!("stale_shadow before={before} after={after} error={error}");
}

#[test]
fn empty_project_exports_and_bakes_empty_state() {
    let document = EditorDocument::new("calyx://world/empty-map-build", "rev-empty").unwrap();
    let root = temp_root("empty");
    let script_path = root.join("empty.lua");
    let shadow_path = root.join("empty_shadows.ron");

    let export = export_script(&document, &script_path).unwrap();
    let receipt =
        save_static_shadow_bake(&document, &shadow_path, StaticShadowBakeSettings::default())
            .unwrap();
    let shadow_readback = load_static_shadow_bake(&shadow_path).unwrap();

    assert_eq!(export.summary.object_count, 0);
    assert_eq!(export.summary.terrain_cell_count, 0);
    assert_eq!(export.summary.terrain_variation_count, 0);
    assert_eq!(export.summary.random_group_count, 0);
    assert_eq!(shadow_readback.casters.len(), 0);
    assert_eq!(shadow_readback.nonzero_cell_count(), 0);
    assert_eq!(shadow_readback, receipt.bake);
    println!(
        "empty_project script={} shadow={} cells_sha={}",
        script_path.display(),
        shadow_path.display(),
        shadow_readback.cells_sha256
    );
}

#[test]
fn invalid_output_extension_fails_before_creating_artifact() {
    let document = fixture_document("rev-invalid-extension");
    let root = temp_root("invalid-extension");
    let bad_path = root.join("map.txt");
    let before_exists = bad_path.exists();
    let error = export_script(&document, &bad_path).unwrap_err();
    let after_exists = bad_path.exists();

    assert!(!before_exists);
    assert!(!after_exists);
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("script.output_path"));
    println!(
        "invalid_extension path={} before_exists={} after_exists={} error={}",
        bad_path.display(),
        before_exists,
        after_exists,
        error
    );
}

fn fixture_document(revision: &str) -> EditorDocument {
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| record.relative_path == REAL_STATIC_MODEL)
        .cloned()
        .expect("real static model must exist in asset vault");
    let product = product_from_vault_record(&record).unwrap();
    let mut document = EditorDocument::new("calyx://world/map-build-known", revision).unwrap();
    document
        .asset_products
        .insert(product.product_id.clone(), product.clone());
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "doodad_rock",
                "Doodad",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(12.0, 0.0, -18.0)),
            )
            .with_property("static_shadow", "true")
            .with_property("shadow_footprint_tiles", "3x2")
            .with_property("shadow_height_tiles", "4")
            .with_property(MODEL_PRODUCT_ID_PROPERTY, product.product_id)
            .with_property(MODEL_SOURCE_SHA256_PROPERTY, product.source_sha256)
            .with_property(MODEL_SOURCE_PATH_PROPERTY, product.source_relative_path),
        )
        .unwrap();
    document
        .upsert_region(EditorRegion::new(
            "region_spawn",
            "Spawn Plateau",
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(8.0, 2.0, 8.0),
        ))
        .unwrap();
    document
        .upsert_terrain_stamp(EditorTerrainStamp::new(
            "terrain_ramp",
            "Ramp",
            Transform::from_translation(Vec3::new(-8.0, 0.0, 6.0)),
            5,
            3,
            2.25,
        ))
        .unwrap();
    document
        .upsert_terrain_cell(EditorTerrainCell::new(
            "cell_spawn_grass",
            0,
            0,
            "tileset.grass",
            "cliff.grass",
            1,
            3.5,
        ))
        .unwrap();
    document
        .upsert_terrain_cell(EditorTerrainCell::new(
            "cell_path_dirt",
            1,
            0,
            "tileset.dirt",
            "cliff.dirt",
            0,
            2.0,
        ))
        .unwrap();
    document
        .upsert_terrain_tile_variation(TerrainTileVariation::new(
            "tile_spawn_grass",
            0,
            0,
            "tileset.grass",
            4,
            2,
            12_700,
        ))
        .unwrap();
    document
        .upsert_terrain_tile_variation(TerrainTileVariation::new(
            "tile_path_dirt",
            1,
            0,
            "tileset.dirt",
            3,
            1,
            12_700,
        ))
        .unwrap();
    document
        .upsert_random_group(TerrainRandomGroup::new(
            "forest_floor_mix",
            "Forest Floor Mix",
            88_123,
            vec![
                TerrainRandomGroupMember::new(
                    "grass_a",
                    TerrainRandomGroupMemberKind::Tile,
                    "tileset.grass.variant_a",
                    5,
                ),
                TerrainRandomGroupMember::new(
                    "grass_b",
                    TerrainRandomGroupMemberKind::Tile,
                    "tileset.grass.variant_b",
                    3,
                ),
                TerrainRandomGroupMember::new(
                    "fern",
                    TerrainRandomGroupMemberKind::Doodad,
                    "doodad.fern.low",
                    2,
                ),
            ],
        ))
        .unwrap();
    document.validate().unwrap();
    document
}

fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("map-build-{label}"))
}
