use super::*;

use crate::{
    asset_products::product_from_vault_record,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};

const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const FLAG_PENNANT_GLB: &str =
    "extracted/kenney/game-kit/castle-kit/Models/GLB_format/flag-pennant.glb";
const FOOTSTEP_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

#[test]
fn imports_real_textured_gltf_materials_and_mesh_buffers() {
    let product = real_product(MOUNTAINSIDE_GLTF);
    let receipt = import_gltf_asset_receipt(&product).unwrap();

    assert_eq!(receipt.source_relative_path, MOUNTAINSIDE_GLTF);
    assert_eq!(receipt.verified_file_sha256, product.source_sha256);
    assert_eq!(receipt.extension, "gltf");
    assert_eq!(receipt.mesh_count, 1);
    assert_eq!(receipt.primitive_count, 1);
    assert_eq!(receipt.material_count, 1);
    assert_eq!(receipt.texture_count, 3);
    assert_eq!(receipt.image_count, 3);
    assert_eq!(receipt.unique_material_texture_set_count, 1);
    assert_eq!(receipt.vertex_count, 80_238);
    assert_eq!(receipt.index_count, 460_416);
    assert_eq!(receipt.triangle_count, 153_472);
    assert_eq!(receipt.skin_count, 0);
    assert_eq!(receipt.animation_count, 0);
    assert_eq!(receipt.morph_target_count, 0);
    assert_eq!(
        receipt.materials[0]
            .base_color_texture_source_path
            .as_deref(),
        Some("sources/polyhaven/models/mountainside/2k/textures/mountainside_diff_2k.jpg")
    );
    assert_eq!(
        receipt.materials[0].normal_texture_source_path.as_deref(),
        Some("sources/polyhaven/models/mountainside/2k/textures/mountainside_nor_gl_2k.jpg")
    );
    assert_eq!(
        receipt.materials[0]
            .metallic_roughness_texture_source_path
            .as_deref(),
        Some("sources/polyhaven/models/mountainside/2k/textures/mountainside_arm_2k.jpg")
    );
    println!(
        "gltf_import_textured source={} sha={} deps={} meshes={} primitives={} materials={} textures={} images={} vertices={} indices={} triangles={} base={:?} normal={:?} mr={:?}",
        receipt.source_relative_path,
        receipt.verified_file_sha256,
        receipt.dependency_count,
        receipt.mesh_count,
        receipt.primitive_count,
        receipt.material_count,
        receipt.texture_count,
        receipt.image_count,
        receipt.vertex_count,
        receipt.index_count,
        receipt.triangle_count,
        receipt.materials[0].base_color_texture_source_path,
        receipt.materials[0].normal_texture_source_path,
        receipt.materials[0].metallic_roughness_texture_source_path
    );
}

#[test]
fn imports_real_skinned_animated_gltf_skeleton_and_clips() {
    let product = real_product(STAN_GLTF);
    let receipt = import_gltf_asset_receipt(&product).unwrap();

    assert_eq!(receipt.source_relative_path, STAN_GLTF);
    assert_eq!(receipt.verified_file_sha256, product.source_sha256);
    assert_eq!(receipt.extension, "gltf");
    assert_eq!(receipt.mesh_count, 1);
    assert_eq!(receipt.primitive_count, 6);
    assert_eq!(receipt.vertex_count, 6_214);
    assert_eq!(receipt.index_count, 17_916);
    assert_eq!(receipt.triangle_count, 5_972);
    assert_eq!(receipt.skin_count, 1);
    assert_eq!(receipt.joint_count, 43);
    assert_eq!(receipt.skinned_node_count, 1);
    assert_eq!(receipt.skinned_vertex_count, 6_214);
    assert_eq!(receipt.weighted_vertex_count, 6_214);
    assert_eq!(receipt.animation_count, 18);
    assert_eq!(receipt.animation_channel_count, 1_548);
    assert_eq!(receipt.animation_sampler_count, 1_548);
    assert_eq!(receipt.morph_target_count, 0);
    assert!(
        receipt
            .animations
            .iter()
            .any(|clip| clip.name == "SwordSlash")
    );
    assert!(
        receipt.skins[0]
            .joint_names
            .iter()
            .any(|joint| joint.contains("Armature"))
            || receipt.skins[0].joint_count == 43
    );
    println!(
        "gltf_import_skinned source={} sha={} meshes={} primitives={} skin_count={} joints={} skinned_vertices={} weighted_vertices={} clips={} channels={} samplers={} sword_slash_present={}",
        receipt.source_relative_path,
        receipt.verified_file_sha256,
        receipt.mesh_count,
        receipt.primitive_count,
        receipt.skin_count,
        receipt.joint_count,
        receipt.skinned_vertex_count,
        receipt.weighted_vertex_count,
        receipt.animation_count,
        receipt.animation_channel_count,
        receipt.animation_sampler_count,
        receipt
            .animations
            .iter()
            .any(|clip| clip.name == "SwordSlash")
    );
}

#[test]
fn imports_real_clean_glb_static_mesh_buffers() {
    let product = real_product(FLAG_PENNANT_GLB);
    let receipt = import_gltf_asset_receipt(&product).unwrap();

    assert_eq!(receipt.source_relative_path, FLAG_PENNANT_GLB);
    assert_eq!(receipt.verified_file_sha256, product.source_sha256);
    assert_eq!(receipt.extension, "glb");
    assert_eq!(receipt.mesh_count, 1);
    assert_eq!(receipt.primitive_count, 1);
    assert_eq!(receipt.vertex_count, 238);
    assert_eq!(receipt.index_count, 354);
    assert_eq!(receipt.triangle_count, 118);
    assert_eq!(receipt.skin_count, 0);
    assert_eq!(receipt.animation_count, 0);
    assert_eq!(receipt.animation_channel_count, 0);
    assert_eq!(receipt.animation_sampler_count, 0);
    println!(
        "gltf_import_glb source={} sha={} meshes={} primitives={} vertices={} indices={} triangles={} clips={} channels={} samplers={}",
        receipt.source_relative_path,
        receipt.verified_file_sha256,
        receipt.mesh_count,
        receipt.primitive_count,
        receipt.vertex_count,
        receipt.index_count,
        receipt.triangle_count,
        receipt.animation_count,
        receipt.animation_channel_count,
        receipt.animation_sampler_count
    );
}

#[test]
fn tampered_manifest_sha_fails_before_semantic_import() {
    let mut product = real_product(FLAG_PENNANT_GLB);
    let before_sha = product.source_sha256.clone();
    product.source_sha256 = "0".repeat(64);

    let error = import_error(&product);

    assert!(error.contains("editor.gltf_import.source.sha256"));
    assert_eq!(real_product(FLAG_PENNANT_GLB).source_sha256, before_sha);
    println!(
        "gltf_import_edge_tampered_sha source={} before_sha={} after_error={}",
        product.source_relative_path, before_sha, error
    );
}

#[test]
fn non_mesh_record_fails_closed_for_gltf_import() {
    let product = real_product(FOOTSTEP_OGG);
    let error = import_error(&product);

    assert!(error.contains("editor.gltf_import.product.kind"));
    println!(
        "gltf_import_edge_non_mesh source={} kind={:?} error={}",
        product.source_relative_path, product.product_kind, error
    );
}

fn real_product(relative_path: &str) -> AssetProduct {
    product_from_vault_record(&real_record(relative_path)).unwrap()
}

fn real_record(relative_path: &str) -> AssetVaultRecord {
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

fn import_error(product: &AssetProduct) -> String {
    match import_gltf_asset_receipt(product) {
        Ok(_) => panic!("expected glTF import to fail"),
        Err(error) => error.to_string(),
    }
}
