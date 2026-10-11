use super::*;
use crate::{
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint, to_stable_ron},
    play::{PlayCameraRig, PlaySession},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn assigns_real_quaternius_mesh_product_to_unit_document() {
    let record = real_record("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    let mut document = fixture_document("rev-asset-1");
    let receipt = assign_vault_model_to_unit(&mut document, "unit_knight", &record).unwrap();
    let unit = document.objects.get("unit_knight").unwrap();
    let product = document.asset_products.get(&receipt.product_id).unwrap();
    let stable = to_stable_ron(&document).unwrap();

    assert_eq!(receipt.product_kind, AssetProductKind::Mesh);
    assert_eq!(
        unit.properties[MODEL_PRODUCT_ID_PROPERTY],
        receipt.product_id
    );
    assert_eq!(unit.properties[MODEL_SOURCE_SHA256_PROPERTY], record.sha256);
    assert_eq!(product.source_relative_path, record.relative_path);
    assert_eq!(product.source_bytes, 15_212);
    assert!(stable.contains("model_product_id"));
    println!(
        "assignment object={} product={} source={} sha={} document_fingerprint={}",
        receipt.object_id,
        receipt.product_id,
        receipt.source_relative_path,
        receipt.source_sha256,
        receipt.document_fingerprint
    );
}

#[test]
fn play_build_uses_assigned_product_as_render_mesh_id() {
    let record = real_record("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    let mut document = fixture_document("rev-play-asset");
    let receipt = assign_vault_model_to_unit(&mut document, "unit_knight", &record).unwrap();
    let session = PlaySession::start(&document, PlayCameraRig::TopDown).unwrap();
    let instance = &session.build.presentation.instances[0];

    assert_eq!(session.build.spawned_bodies.len(), 1);
    assert_eq!(instance.render_instance.mesh_id, receipt.product_id);
    assert_eq!(session.build.visible_instance_ids, vec![1]);
    println!(
        "play_build mesh_id={} visible={}",
        instance.render_instance.mesh_id,
        session
            .build
            .visible_instance_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join("|")
    );
}

#[test]
fn real_gltf_product_records_external_dependencies() {
    let record = real_record("sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf");
    let product = product_from_vault_record(&record).unwrap();

    assert_eq!(product.product_kind, AssetProductKind::Mesh);
    assert_eq!(product.dependencies.len(), 4);
    assert!(
        product
            .dependencies
            .contains(&"sources/polyhaven/models/mountainside/2k/mountainside.bin".to_string())
    );
    assert!(product.dependencies.contains(
        &"sources/polyhaven/models/mountainside/2k/textures/mountainside_diff_2k.jpg".to_string()
    ));
    println!(
        "gltf_product product={} deps={}",
        product.product_id,
        product.dependencies.join("|")
    );
}

#[test]
fn uppercase_source_sha_fails_canonical_validation() {
    let record = real_record("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    let mut product = product_from_vault_record(&record).unwrap();
    let before = product.source_sha256.clone();
    product.source_sha256 = before.to_ascii_uppercase();

    let error = product.validate().unwrap_err().to_string();

    assert!(error.contains("editor.asset_product.source_sha256"));
    println!(
        "edge=uppercase_sha before={} after={} error={}",
        before, product.source_sha256, error
    );
}

#[test]
fn texture_record_cannot_be_assigned_as_unit_model() {
    let record = real_record(
        "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png",
    );
    let mut document = fixture_document("rev-texture-reject");
    let error = assign_vault_model_to_unit(&mut document, "unit_knight", &record)
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.asset_product.assignment.kind"));
    assert!(document.asset_products.is_empty());
    println!("edge=texture_as_model product_count=0 error={error}");
}

#[test]
fn missing_assignment_target_fails_without_mutating_document() {
    let record = real_record("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    let mut document = fixture_document("rev-missing-target");
    let before = stable_fingerprint(&document).unwrap();
    let error = assign_vault_model_to_unit(&mut document, "unit_missing", &record)
        .unwrap_err()
        .to_string();
    let after = stable_fingerprint(&document).unwrap();

    assert!(error.contains("editor.asset_product.assignment.object"));
    assert_eq!(before, after);
    println!("edge=missing_target before={before} after={after} error={error}");
}

#[test]
fn play_build_fails_loud_when_unit_has_no_model_assignment() {
    let document = fixture_document("rev-no-model");
    let error = PlaySession::start(&document, PlayCameraRig::TopDown)
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.play.unit.model_product_id"));
    println!("edge=no_model_assignment error={error}");
}

fn real_record(relative_path: &str) -> crate::asset_vault::AssetVaultRecord {
    AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "{}",
                crate::asset_vault_fixture::missing_fixture(relative_path)
            )
        })
}

fn fixture_document(revision: &str) -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/ashen-citadel", revision).unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_knight",
                "Unit",
                None,
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    document
}
