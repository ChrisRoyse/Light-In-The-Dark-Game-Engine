use super::*;

use crate::{
    asset_vault::AssetVaultCatalog,
    history::stable_fingerprint,
    world_creation::{EditorWorldCreationSpec, create_editor_world_record},
};
use litd_engine::math::Vec3;

fn model_assets(count: usize) -> Vec<ContentBrowserAssetRef> {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    catalog
        .records()
        .iter()
        .filter(|record| {
            let path = record.relative_path.to_ascii_lowercase();
            (path.ends_with(".gltf") || path.ends_with(".glb")) && record.bytes_on_disk > 0
        })
        .take(count)
        .map(ContentBrowserAssetRef::from_vault_record)
        .collect()
}

#[test]
fn content_browser_asset_drop_places_real_model_with_snapped_transform() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let asset = model_assets(1).pop().unwrap();
    let request = AssetDropRequest {
        request_id: "issue1223_happy".to_string(),
        assets: vec![asset.clone()],
        drop_world: Vec3::new(4.35, 0.0, 6.6),
        surface_valid: true,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };

    let report = place_assets_in_world_record(&mut record, &request).unwrap();
    let object_id = &report.placed_object_ids[0];
    let object = record.document.objects.get(object_id).unwrap();

    assert_ne!(report.before_fingerprint, report.after_fingerprint);
    assert_eq!(object.transform.translation, Vec3::new(4.0, 0.0, 7.0));
    assert_eq!(
        object.properties.get(PLACED_ASSET_RELATIVE_PATH_PROPERTY),
        Some(&asset.relative_path)
    );
    assert_eq!(
        report.readback[0].asset_sha256,
        object.properties[PLACED_ASSET_SHA256_PROPERTY]
    );
    assert_eq!(
        record.document_fingerprint,
        stable_fingerprint(&record.document).unwrap()
    );
}

#[test]
fn invalid_surface_rejects_without_mutating_document() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let before = record.document_fingerprint.clone();
    let request = AssetDropRequest {
        request_id: "issue1223_invalid_surface".to_string(),
        assets: model_assets(1),
        drop_world: Vec3::new(5.0, 0.0, 5.0),
        surface_valid: false,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };

    let error = place_assets_in_world_record(&mut record, &request).unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert_eq!(record.document_fingerprint, before);
    assert_eq!(record.document.objects.len(), 1);
}

#[test]
fn outside_bounds_are_clamped_and_multi_asset_drop_keeps_each_asset() {
    let mut record =
        create_editor_world_record(EditorWorldCreationSpec::issue1210_default()).unwrap();
    let assets = model_assets(2);
    assert_eq!(assets.len(), 2);
    let request = AssetDropRequest {
        request_id: "issue1223_multi".to_string(),
        assets: assets.clone(),
        drop_world: Vec3::new(999.0, 0.0, -12.0),
        surface_valid: true,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };

    let report = place_assets_in_world_record(&mut record, &request).unwrap();

    assert!(report.clamped);
    assert_eq!(report.placed_object_ids.len(), 2);
    for (index, id) in report.placed_object_ids.iter().enumerate() {
        let object = record.document.objects.get(id).unwrap();
        assert_eq!(object.transform.translation.x, 16.0);
        assert_eq!(object.transform.translation.z, 0.0);
        assert_eq!(
            object.properties.get(PLACED_ASSET_RELATIVE_PATH_PROPERTY),
            Some(&assets[index].relative_path)
        );
        assert_eq!(
            object.properties.get(PLACED_ASSET_INDEX_PROPERTY),
            Some(&index.to_string())
        );
    }
}
