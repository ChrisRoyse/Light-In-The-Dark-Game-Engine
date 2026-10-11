use super::*;

use crate::{
    asset_products::{AssetProduct, product_from_vault_record},
    asset_vault::AssetVaultCatalog,
};

#[test]
fn loads_real_polyhaven_gltf_as_editor_world_scene() {
    let product = real_product(DEFAULT_EDITOR_WORLD_GLTF);
    let loaded = load_editor_world_asset_scene(&product).unwrap();
    let report = loaded.report;

    assert_eq!(report.product_id, product.product_id);
    assert_eq!(report.source_relative_path, DEFAULT_EDITOR_WORLD_GLTF);
    assert_eq!(report.dependency_count, 4);
    assert_eq!(report.material_count, 1);
    assert_eq!(report.texture_count, 3);
    assert_eq!(report.image_count, 3);
    assert_eq!(
        report.base_color_texture_source_path,
        "sources/polyhaven/models/mountainside/2k/textures/mountainside_diff_2k.jpg"
    );
    assert_eq!(report.base_color_texture_width, 2048);
    assert_eq!(report.base_color_texture_height, 2048);
    assert_eq!(report.base_color_texture_bytes, 16_777_216);
    assert_eq!(
        report.normal_texture_source_path,
        "sources/polyhaven/models/mountainside/2k/textures/mountainside_nor_gl_2k.jpg"
    );
    assert_eq!(report.normal_texture_width, 2048);
    assert_eq!(report.normal_texture_height, 2048);
    assert_eq!(report.normal_texture_bytes, 16_777_216);
    assert_eq!(
        report.metallic_roughness_texture_source_path,
        "sources/polyhaven/models/mountainside/2k/textures/mountainside_arm_2k.jpg"
    );
    assert_eq!(report.metallic_roughness_texture_width, 2048);
    assert_eq!(report.metallic_roughness_texture_height, 2048);
    assert_eq!(report.metallic_roughness_texture_bytes, 16_777_216);
    assert_eq!(report.vertex_count, 80_238);
    assert_eq!(report.index_count, 460_416);
    assert_eq!(report.triangle_count, 153_472);
    assert_eq!(report.mesh_primitives, 1);
    assert!((report.fitted_bounds_min[1] - 0.0).abs() < 0.001);
    assert!(longest_extent(&report) <= 96.001);
    assert_eq!(report.world_map.source_kind, "gltf_fitted_bounds");
    assert_eq!(report.world_map.outline.len(), 4);
    assert_eq!(report.world_map.feature_count(), 4);
    println!(
        "asset_scene_happy product={} source={} base_color={} base_color_dims={}x{} base_color_bytes={} normal={} normal_dims={}x{} normal_bytes={} metallic_roughness={} metallic_roughness_dims={}x{} metallic_roughness_bytes={} vertices={} indices={} materials={} textures={} images={} bounds_min={:?} bounds_max={:?}",
        report.product_id,
        report.source_relative_path,
        report.base_color_texture_source_path,
        report.base_color_texture_width,
        report.base_color_texture_height,
        report.base_color_texture_bytes,
        report.normal_texture_source_path,
        report.normal_texture_width,
        report.normal_texture_height,
        report.normal_texture_bytes,
        report.metallic_roughness_texture_source_path,
        report.metallic_roughness_texture_width,
        report.metallic_roughness_texture_height,
        report.metallic_roughness_texture_bytes,
        report.vertex_count,
        report.index_count,
        report.material_count,
        report.texture_count,
        report.image_count,
        report.fitted_bounds_min,
        report.fitted_bounds_max
    );
}

#[test]
fn blank_editor_world_scene_has_zero_authored_geometry_and_valid_map() {
    let loaded = load_blank_editor_world_asset_scene().unwrap();
    let report = loaded.report;

    assert_eq!(report.product_id, "litd.editor.blank_world.authoring_v1");
    assert_eq!(
        report.source_relative_path,
        "generated://editor/blank-world/authoring-v1"
    );
    assert_eq!(report.dependency_count, 0);
    assert_eq!(report.material_count, 0);
    assert_eq!(report.texture_count, 0);
    assert_eq!(report.image_count, 0);
    assert_eq!(report.vertex_count, 0);
    assert_eq!(report.index_count, 0);
    assert_eq!(report.triangle_count, 0);
    assert_eq!(report.mesh_primitives, 0);
    assert_eq!(report.world_map.source_kind, "blank_authoring_bounds");
    assert_eq!(report.world_map.outline.len(), 4);
    assert_eq!(report.world_map.feature_count(), 4);
    println!(
        "blank_scene product={} source={} vertices={} indices={} triangles={} primitives={} bounds_min={:?} bounds_max={:?} map_kind={} map_features={}",
        report.product_id,
        report.source_relative_path,
        report.vertex_count,
        report.index_count,
        report.triangle_count,
        report.mesh_primitives,
        report.fitted_bounds_min,
        report.fitted_bounds_max,
        report.world_map.source_kind,
        report.world_map.feature_count()
    );
}

#[test]
fn flat_color_gltf_mesh_uses_material_defaults_on_world_scene_path() {
    let product =
        real_product("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    println!(
        "flat_color_gltf before_product={} before_source={} expected_renderer_path=EditorWorldSceneBuffers",
        product.product_id, product.source_relative_path
    );

    let loaded = load_editor_world_asset_scene(&product).unwrap();
    let report = loaded.report;

    assert_eq!(
        report.base_color_texture_source_path,
        "generated://editor/gltf/base_color_texture"
    );
    assert_eq!(
        report.normal_texture_source_path,
        "generated://editor/gltf/normal_texture"
    );
    assert_eq!(
        report.metallic_roughness_texture_source_path,
        "generated://editor/gltf/metallic_roughness_texture"
    );
    assert_eq!(report.base_color_texture_width, 1);
    assert_eq!(report.normal_texture_width, 1);
    assert_eq!(report.metallic_roughness_texture_width, 1);
    assert_eq!(report.base_color_texture_bytes, 4);
    assert_eq!(report.normal_texture_bytes, 4);
    assert_eq!(report.metallic_roughness_texture_bytes, 4);
    assert_eq!(report.vertex_count, 6214);
    assert_eq!(report.index_count, 17916);
    assert_eq!(report.triangle_count, 5972);
    assert_eq!(report.mesh_primitives, 6);
    assert!((report.fitted_bounds_min[1] - 0.0).abs() < 0.001);
    assert!(longest_extent(&report) <= 96.001);
    println!(
        "flat_color_gltf after_product={} source={} vertices={} indices={} triangles={} primitives={} bounds_min={:?} bounds_max={:?}",
        product.product_id,
        report.source_relative_path,
        report.vertex_count,
        report.index_count,
        report.triangle_count,
        report.mesh_primitives,
        report.fitted_bounds_min,
        report.fitted_bounds_max
    );
}

#[test]
fn fbx_mesh_product_fails_loud_for_world_scene_loader() {
    let product =
        real_product("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    println!(
        "edge=fbx_scene before_product={} before_source={}",
        product.product_id, product.source_relative_path
    );

    let error = scene_error(&product);

    assert!(error.contains("editor.asset_scene.source.extension"));
    println!(
        "edge=fbx_scene after_product={} error={}",
        product.product_id, error
    );
}

#[test]
fn texture_product_fails_loud_for_world_scene_loader() {
    let product = real_product(
        "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png",
    );
    println!(
        "edge=texture_scene before_product={} before_kind={:?}",
        product.product_id, product.product_kind
    );

    let error = scene_error(&product);

    assert!(error.contains("editor.asset_scene.product.kind"));
    println!(
        "edge=texture_scene after_product={} error={}",
        product.product_id, error
    );
}

#[test]
fn missing_recorded_dependency_fails_loud_before_import() {
    let mut product = real_product(DEFAULT_EDITOR_WORLD_GLTF);
    let before_dependency_count = product.dependencies.len();
    product.dependencies[0] =
        "sources/polyhaven/models/mountainside/2k/missing-mountainside.bin".to_string();
    println!(
        "edge=missing_scene_dependency before_product={} before_dependency_count={}",
        product.product_id, before_dependency_count
    );

    let error = scene_error(&product);

    assert!(error.contains("editor.asset_product.dependencies.exists"));
    println!(
        "edge=missing_scene_dependency after_product={} after_dependency_count={} error={}",
        product.product_id,
        product.dependencies.len(),
        error
    );
}

fn real_product(relative_path: &str) -> AssetProduct {
    let record = AssetVaultCatalog::load_default_cached()
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
        });
    product_from_vault_record(&record).unwrap()
}

fn scene_error(product: &AssetProduct) -> String {
    match load_editor_world_asset_scene(product) {
        Ok(_) => panic!("expected editor asset scene load to fail"),
        Err(error) => error.to_string(),
    }
}

fn longest_extent(report: &EditorAssetSceneReport) -> f32 {
    let size = [
        report.fitted_bounds_max[0] - report.fitted_bounds_min[0],
        report.fitted_bounds_max[1] - report.fitted_bounds_min[1],
        report.fitted_bounds_max[2] - report.fitted_bounds_min[2],
    ];
    size.into_iter().fold(0.0_f32, f32::max)
}
