use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use crate::{
    asset_products::{AssetProduct, AssetProductKind},
    gltf_validation::validate_gltf_source,
};

#[path = "gltf_asset_import_animations.rs"]
mod animations;
#[path = "gltf_asset_import_materials.rs"]
mod materials;
#[path = "gltf_asset_import_primitives.rs"]
mod primitives;
#[path = "gltf_asset_import_skins.rs"]
mod skins;
#[path = "gltf_asset_import_support.rs"]
mod support;
#[path = "gltf_asset_import_types.rs"]
mod types;

use animations::import_animation;
use materials::{import_material, material_texture_key};
use primitives::{import_primitive, validate_mesh_morph_weights};
use skins::import_skin;
use support::{invalid_import, verify_product_hash};
pub use types::{
    GltfAnimationImport, GltfAssetImportReceipt, GltfMaterialImport, GltfPrimitiveImport,
    GltfSkinImport,
};

pub fn import_gltf_asset_receipt(
    product: &AssetProduct,
) -> Result<GltfAssetImportReceipt, EngineError> {
    product.validate()?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_import("product.kind", "glTF import requires a mesh product");
    }
    let extension = Path::new(&product.source_relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.gltf_import.source.extension".to_string(),
            reason: "glTF source has no extension".to_string(),
        })?;
    if !matches!(extension.as_str(), "gltf" | "glb") {
        return invalid_import("source.extension", "expected .gltf or .glb source");
    }

    let source_path = Path::new(&product.source_absolute_path);
    let verified_file_sha256 = verify_product_hash(source_path, product)?;
    let validation = validate_gltf_source(source_path, &product.source_relative_path)?;
    let (document, buffers, images) =
        gltf::import(source_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.gltf_import.import".to_string(),
            reason: format!(
                "{} failed semantic glTF import: {error}",
                source_path.display()
            ),
        })?;

    let materials = document
        .materials()
        .map(|material| import_material(&product.source_relative_path, material))
        .collect::<Result<Vec<_>, _>>()?;
    let skins = document
        .skins()
        .map(|skin| import_skin(skin, &buffers))
        .collect::<Result<Vec<_>, _>>()?;
    let animations = document
        .animations()
        .map(|animation| import_animation(animation, &buffers, &product.source_relative_path))
        .collect::<Result<Vec<_>, _>>()?;

    let mut material_texture_sets = BTreeSet::new();
    let mut primitives = Vec::new();
    for mesh in document.meshes() {
        validate_mesh_morph_weights(&mesh)?;
        for primitive in mesh.primitives() {
            material_texture_sets.insert(material_texture_key(
                &product.source_relative_path,
                &primitive.material(),
            ));
            primitives.push(import_primitive(mesh.index(), primitive, &buffers)?);
        }
    }

    let vertex_count = primitives
        .iter()
        .map(|primitive| primitive.vertex_count)
        .sum();
    let index_count = primitives
        .iter()
        .map(|primitive| primitive.index_count)
        .sum();
    let primitive_count = primitives.len();
    if validation.index_count != index_count || validation.primitive_count != primitive_count {
        return invalid_import(
            "buffer_counts",
            format!(
                "validated counts primitives={} indices={} do not match semantic import primitives={} indices={}",
                validation.primitive_count, validation.index_count, primitive_count, index_count
            ),
        );
    }

    Ok(GltfAssetImportReceipt {
        schema: "litd.gltf_asset_import_receipt.v1".to_string(),
        source_relative_path: product.source_relative_path.clone(),
        source_absolute_path: product.source_absolute_path.clone(),
        source_sha256: product.source_sha256.clone(),
        verified_file_sha256,
        source_bytes: product.source_bytes,
        extension,
        dependency_count: validation.dependencies.len(),
        dependencies: validation.dependencies,
        scene_count: document.scenes().count(),
        default_scene_index: document.default_scene().map(|scene| scene.index()),
        node_count: document.nodes().count(),
        mesh_count: document.meshes().count(),
        primitive_count,
        material_count: document.materials().count(),
        texture_count: document.textures().count(),
        sampler_count: document.samplers().count(),
        image_count: images.len(),
        unique_material_texture_set_count: material_texture_sets.len(),
        vertex_count,
        index_count,
        triangle_count: index_count / 3,
        skin_count: skins.len(),
        joint_count: skins.iter().map(|skin| skin.joint_count).sum(),
        skinned_node_count: document
            .nodes()
            .filter(|node| node.skin().is_some())
            .count(),
        skinned_vertex_count: primitives
            .iter()
            .map(|primitive| primitive.skinned_vertex_count)
            .sum(),
        weighted_vertex_count: primitives
            .iter()
            .map(|primitive| primitive.weighted_vertex_count)
            .sum(),
        inverse_bind_matrix_count: skins
            .iter()
            .map(|skin| skin.inverse_bind_matrix_count)
            .sum(),
        animation_count: animations.len(),
        animation_channel_count: animations
            .iter()
            .map(|animation| animation.channel_count)
            .sum(),
        animation_sampler_count: animations
            .iter()
            .map(|animation| animation.sampler_count)
            .sum(),
        morph_target_count: primitives
            .iter()
            .map(|primitive| primitive.morph_target_count)
            .sum(),
        morph_position_count: primitives
            .iter()
            .map(|primitive| primitive.morph_position_count)
            .sum(),
        morph_normal_count: primitives
            .iter()
            .map(|primitive| primitive.morph_normal_count)
            .sum(),
        morph_tangent_count: primitives
            .iter()
            .map(|primitive| primitive.morph_tangent_count)
            .sum(),
        materials,
        primitives,
        skins,
        animations,
    })
}

#[cfg(test)]
#[path = "gltf_asset_import_tests.rs"]
mod tests;
