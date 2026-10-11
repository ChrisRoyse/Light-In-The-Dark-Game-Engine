use litd_engine::EngineError;

use super::{GltfMaterialImport, support::invalid_import};

pub(super) fn import_material(
    scene_source_relative_path: &str,
    material: gltf::Material<'_>,
) -> Result<GltfMaterialImport, EngineError> {
    let pbr = material.pbr_metallic_roughness();
    let base_color_factor = pbr.base_color_factor();
    let metallic_factor = pbr.metallic_factor();
    let roughness_factor = pbr.roughness_factor();
    if base_color_factor.iter().any(|value| !value.is_finite())
        || !metallic_factor.is_finite()
        || !roughness_factor.is_finite()
    {
        return invalid_import("material.factor", "PBR material factor is not finite");
    }
    Ok(GltfMaterialImport {
        material_index: material
            .index()
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.gltf_import.material.index".to_string(),
                reason: "explicit material import received the default material".to_string(),
            })?,
        name: material.name().map(str::to_string),
        alpha_mode: format!("{:?}", material.alpha_mode()),
        double_sided: material.double_sided(),
        base_color_factor,
        metallic_factor,
        roughness_factor,
        base_color_texture_source_path: pbr
            .base_color_texture()
            .map(|texture| texture_source_path(scene_source_relative_path, texture.texture())),
        normal_texture_source_path: material
            .normal_texture()
            .map(|texture| texture_source_path(scene_source_relative_path, texture.texture())),
        metallic_roughness_texture_source_path: pbr
            .metallic_roughness_texture()
            .map(|texture| texture_source_path(scene_source_relative_path, texture.texture())),
        emissive_texture_source_path: material
            .emissive_texture()
            .map(|texture| texture_source_path(scene_source_relative_path, texture.texture())),
    })
}

pub(super) fn material_texture_key(
    scene_source_relative_path: &str,
    material: &gltf::Material<'_>,
) -> String {
    let pbr = material.pbr_metallic_roughness();
    let base = pbr
        .base_color_texture()
        .map(|texture| texture_source_path(scene_source_relative_path, texture.texture()))
        .unwrap_or_else(|| "generated://editor/gltf/base_color_texture".to_string());
    let normal = material
        .normal_texture()
        .map(|texture| texture_source_path(scene_source_relative_path, texture.texture()))
        .unwrap_or_else(|| "generated://editor/gltf/normal_texture".to_string());
    let mr = pbr
        .metallic_roughness_texture()
        .map(|texture| texture_source_path(scene_source_relative_path, texture.texture()))
        .unwrap_or_else(|| "generated://editor/gltf/metallic_roughness_texture".to_string());
    format!("base={base};normal={normal};mr={mr}")
}

fn texture_source_path(scene_source_relative_path: &str, texture: gltf::Texture<'_>) -> String {
    let image = texture.source();
    match image.source() {
        gltf::image::Source::Uri { uri, .. } => {
            if uri.starts_with("data:") {
                format!("{}#image[{}]", scene_source_relative_path, image.index())
            } else if let Some((parent, _)) = scene_source_relative_path.rsplit_once('/') {
                format!("{parent}/{uri}").replace('\\', "/")
            } else {
                uri.replace('\\', "/")
            }
        }
        gltf::image::Source::View { .. } => {
            format!("{}#image[{}]", scene_source_relative_path, image.index())
        }
    }
}
