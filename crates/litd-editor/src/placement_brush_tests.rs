use super::*;

use crate::{
    asset_placement::{
        ContentBrowserAssetRef, PLACED_ASSET_RELATIVE_PATH_PROPERTY, PLACED_ASSET_SHA256_PROPERTY,
    },
    asset_vault::AssetVaultCatalog,
    history::stable_fingerprint,
    prefabs::{PREFAB_INSTANCE_ID_PROPERTY, PrefabDefinition, PrefabLibrary, PrefabObjectTemplate},
    world_creation::{EditorWorldCreationSpec, create_editor_world_record},
};

fn model_assets(count: usize) -> Vec<ContentBrowserAssetRef> {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let assets = catalog
        .records()
        .iter()
        .filter(|record| {
            let path = record.relative_path.to_ascii_lowercase();
            (path.ends_with(".gltf") || path.ends_with(".glb")) && record.bytes_on_disk > 0
        })
        .take(count)
        .map(ContentBrowserAssetRef::from_vault_record)
        .collect::<Vec<_>>();
    assert_eq!(assets.len(), count);
    assets
}

fn asset_brush_request(stamp_id: &str, requested_count: usize) -> PlacementBrushRequest {
    let variants = model_assets(3)
        .into_iter()
        .enumerate()
        .map(|(index, asset)| PlacementBrushAsset::new(asset, (index + 1) as u32))
        .collect();
    PlacementBrushRequest {
        stamp_id: stamp_id.to_string(),
        center: litd_engine::math::Vec3::new(7.5, 0.0, 5.5),
        radius: 3.0,
        requested_count,
        density_cap: requested_count,
        seed: 1220,
        rotation_y_degrees: BrushRange {
            min: 0.0,
            max: 270.0,
        },
        uniform_scale: BrushRange { min: 0.8, max: 1.2 },
        snap: PlacementBrushSnap::GridAndTerrain { grid_size: 1.0 },
        clamp_to_playable_bounds: true,
        content: PlacementBrushContent::Assets { variants },
    }
}

#[test]
fn placement_brush_stamps_real_assets_with_variation_and_surface_snap() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let request = asset_brush_request("issue1220_happy", 8);

    let report = stamp_placement_brush_in_world_record(&mut record, &request).unwrap();

    assert_eq!(report.placed_count, 8);
    assert!(report.grid_snapped);
    assert!(report.terrain_snapped);
    assert_ne!(report.before_fingerprint, report.after_fingerprint);
    let mut yaw_values = Vec::new();
    let mut scale_values = Vec::new();
    let mut variant_values = Vec::new();
    let mut terrain_heights = Vec::new();
    for readback in &report.readback {
        let object = record
            .document
            .objects
            .get(&readback.object_ids[0])
            .unwrap();
        assert_eq!(
            object.properties.get(BRUSH_STAMP_ID_PROPERTY),
            Some(&"issue1220_happy".to_string())
        );
        assert_eq!(readback.translation[0].fract(), 0.0);
        assert_eq!(readback.translation[2].fract(), 0.0);
        let cell_id = readback.terrain_cell_id.as_ref().unwrap();
        let cell = record.document.terrain_cells.get(cell_id).unwrap();
        assert_eq!(readback.translation[1], cell.height);
        assert_eq!(readback.terrain_height, Some(cell.height));
        assert!((0.0..=270.0).contains(&readback.yaw_degrees));
        assert!((0.8..=1.2).contains(&readback.uniform_scale));
        assert_eq!(
            object.transform.translation.to_array(),
            readback.translation
        );
        assert_eq!(object.transform.scale.x, readback.uniform_scale);
        yaw_values.push(format!("{:.3}", readback.yaw_degrees));
        scale_values.push(format!("{:.3}", readback.uniform_scale));
        variant_values.push(readback.variant_index.unwrap());
        terrain_heights.push(format!("{:.3}", readback.translation[1]));
    }
    yaw_values.sort();
    yaw_values.dedup();
    scale_values.sort();
    scale_values.dedup();
    variant_values.sort();
    variant_values.dedup();
    terrain_heights.sort();
    terrain_heights.dedup();
    assert!(yaw_values.len() > 1);
    assert!(scale_values.len() > 1);
    assert!(variant_values.len() > 1);
    assert!(terrain_heights.len() > 1);
    assert_eq!(
        record.document_fingerprint,
        stable_fingerprint(&record.document).unwrap()
    );
}

#[test]
fn zero_jitter_is_seed_deterministic_and_density_cap_limits_objects() {
    let mut first =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let mut second =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let mut request = asset_brush_request("issue1220_zero", 9);
    request.density_cap = 3;
    request.rotation_y_degrees = BrushRange::constant(45.0);
    request.uniform_scale = BrushRange::constant(1.0);
    request.seed = 77;

    let first_report = stamp_placement_brush_in_world_record(&mut first, &request).unwrap();
    let second_report = stamp_placement_brush_in_world_record(&mut second, &request).unwrap();

    assert_eq!(first_report.placed_count, 3);
    assert!(first_report.capped);
    assert_eq!(first_report.readback, second_report.readback);
    for readback in &first_report.readback {
        assert_eq!(readback.yaw_degrees, 45.0);
        assert_eq!(readback.uniform_scale, 1.0);
    }
}

#[test]
fn prefab_brush_expands_real_asset_prefab_and_marks_generated_objects() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let asset = model_assets(1).remove(0);
    let template =
        PrefabObjectTemplate::new("real_asset_doodad", "Doodad", None, Transform::IDENTITY)
            .with_property(
                PLACED_ASSET_RELATIVE_PATH_PROPERTY,
                asset.relative_path.clone(),
            )
            .with_property(PLACED_ASSET_SHA256_PROPERTY, asset.sha256.clone());
    let definition = PrefabDefinition::new(
        "issue1220_real_asset_prefab",
        "Issue 1220 Real Asset Prefab",
        "1220",
        vec![template],
        Vec::new(),
    )
    .unwrap();
    let mut library = PrefabLibrary::new(8).unwrap();
    library.insert_definition(definition).unwrap();
    let request = PlacementBrushRequest {
        stamp_id: "issue1220_prefab".to_string(),
        center: litd_engine::math::Vec3::new(6.0, 0.0, 4.0),
        radius: 1.0,
        requested_count: 2,
        density_cap: 2,
        seed: 9001,
        rotation_y_degrees: BrushRange::constant(0.0),
        uniform_scale: BrushRange::constant(1.0),
        snap: PlacementBrushSnap::GridAndTerrain { grid_size: 1.0 },
        clamp_to_playable_bounds: true,
        content: PlacementBrushContent::Prefab {
            prefab_id: "issue1220_real_asset_prefab".to_string(),
            library,
            overrides: Vec::new(),
        },
    };

    let report = stamp_placement_brush_in_world_record(&mut record, &request).unwrap();

    assert_eq!(report.placed_count, 2);
    for readback in &report.readback {
        assert_eq!(readback.content_kind, "prefab");
        assert_eq!(readback.object_ids.len(), 1);
        let object = record
            .document
            .objects
            .get(&readback.object_ids[0])
            .unwrap();
        assert_eq!(
            object.properties.get(PREFAB_INSTANCE_ID_PROPERTY),
            Some(&format!("issue1220_prefab_{}", readback.stamp_index))
        );
        assert_eq!(
            object.properties.get(BRUSH_CONTENT_KIND_PROPERTY),
            Some(&"prefab".to_string())
        );
        assert_eq!(
            object.properties.get(PLACED_ASSET_RELATIVE_PATH_PROPERTY),
            Some(&asset.relative_path)
        );
    }
}

#[test]
fn invalid_brush_config_fails_without_mutating_document() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let before_fingerprint = record.document_fingerprint.clone();
    let before_objects = record.document.objects.len();
    let mut request = asset_brush_request("issue1220_invalid", 4);
    request.rotation_y_degrees = BrushRange {
        min: 90.0,
        max: 45.0,
    };

    let error = stamp_placement_brush_in_world_record(&mut record, &request).unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("range min must be <= max"));
    assert_eq!(record.document_fingerprint, before_fingerprint);
    assert_eq!(record.document.objects.len(), before_objects);
}
