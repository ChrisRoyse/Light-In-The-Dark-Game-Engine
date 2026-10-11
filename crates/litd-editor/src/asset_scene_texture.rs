use litd_engine::{EngineError, render::editor_world::EditorWorldTextureData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PbrTextureRole {
    BaseColor,
    Normal,
    MetallicRoughness,
}

impl PbrTextureRole {
    fn field(self) -> &'static str {
        match self {
            Self::BaseColor => "base_color_texture",
            Self::Normal => "normal_texture",
            Self::MetallicRoughness => "metallic_roughness_texture",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::BaseColor => "gltf_base_color",
            Self::Normal => "gltf_normal",
            Self::MetallicRoughness => "gltf_metallic_roughness",
        }
    }

    fn human_name(self) -> &'static str {
        match self {
            Self::BaseColor => "baseColorTexture",
            Self::Normal => "normalTexture",
            Self::MetallicRoughness => "metallicRoughnessTexture",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GltfTextureRef {
    pub texture_index: Option<usize>,
    pub image_index: Option<usize>,
    pub source_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PbrTextureSetRef {
    pub base_color: GltfTextureRef,
    pub normal: GltfTextureRef,
    pub metallic_roughness: GltfTextureRef,
}

impl PbrTextureSetRef {
    pub fn uses_imported_textures(&self) -> bool {
        self.base_color.image_index.is_some()
            || self.normal.image_index.is_some()
            || self.metallic_roughness.image_index.is_some()
    }

    pub fn uses_imported_normal_texture(&self) -> bool {
        self.normal.image_index.is_some()
    }
}

pub(super) struct ImportedPbrTextureSet {
    pub base_color: EditorWorldTextureData,
    pub normal: EditorWorldTextureData,
    pub metallic_roughness: EditorWorldTextureData,
}

pub(super) fn resolve_pbr_texture_set_ref(
    material: &gltf::Material<'_>,
    scene_source_relative_path: &str,
) -> Result<PbrTextureSetRef, EngineError> {
    let pbr = material.pbr_metallic_roughness();
    let base_color = pbr
        .base_color_texture()
        .map(|texture| texture_ref(texture.texture(), scene_source_relative_path))
        .unwrap_or_else(|| implicit_texture_ref(PbrTextureRole::BaseColor));
    let normal = material
        .normal_texture()
        .map(|texture| texture_ref(texture.texture(), scene_source_relative_path))
        .unwrap_or_else(|| implicit_texture_ref(PbrTextureRole::Normal));
    let metallic_roughness = pbr
        .metallic_roughness_texture()
        .map(|texture| texture_ref(texture.texture(), scene_source_relative_path))
        .unwrap_or_else(|| implicit_texture_ref(PbrTextureRole::MetallicRoughness));
    Ok(PbrTextureSetRef {
        base_color,
        normal,
        metallic_roughness,
    })
}

pub(super) fn imported_pbr_textures(
    texture_set: &PbrTextureSetRef,
    images: &[gltf::image::Data],
) -> Result<ImportedPbrTextureSet, EngineError> {
    Ok(ImportedPbrTextureSet {
        base_color: imported_texture(PbrTextureRole::BaseColor, &texture_set.base_color, images)?,
        normal: imported_texture(PbrTextureRole::Normal, &texture_set.normal, images)?,
        metallic_roughness: imported_texture(
            PbrTextureRole::MetallicRoughness,
            &texture_set.metallic_roughness,
            images,
        )?,
    })
}

fn imported_texture(
    role: PbrTextureRole,
    texture_ref: &GltfTextureRef,
    images: &[gltf::image::Data],
) -> Result<EditorWorldTextureData, EngineError> {
    let Some(image_index) = texture_ref.image_index else {
        return implicit_texture(role, &texture_ref.source_relative_path);
    };
    let image = images
        .get(image_index)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}.image", role.field()),
            reason: format!(
                "{} references missing imported image {}",
                role.human_name(),
                image_index
            ),
        })?;
    let rgba8 = rgba8_pixels(role, image, &texture_ref.source_relative_path)?;
    EditorWorldTextureData::new(
        role.label(),
        texture_ref.source_relative_path.clone(),
        image.width,
        image.height,
        rgba8,
    )
    .map_err(|error| EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.gltf.{}", role.field()),
        reason: error.to_string(),
    })
}

fn rgba8_pixels(
    role: PbrTextureRole,
    image: &gltf::image::Data,
    source_relative_path: &str,
) -> Result<Vec<u8>, EngineError> {
    let pixel_count = (image.width as usize)
        .checked_mul(image.height as usize)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}.size", role.field()),
            reason: format!("{source_relative_path} dimensions overflow pixel count"),
        })?;
    match image.format {
        gltf::image::Format::R8G8B8A8 => {
            let expected = rgba8_len(role, source_relative_path, pixel_count)?;
            if image.pixels.len() != expected {
                return invalid_texture_bytes(
                    role,
                    source_relative_path,
                    image.pixels.len(),
                    expected,
                );
            }
            Ok(image.pixels.clone())
        }
        gltf::image::Format::R8G8B8 => {
            let expected = rgb8_len(role, source_relative_path, pixel_count)?;
            if image.pixels.len() != expected {
                return invalid_texture_bytes(
                    role,
                    source_relative_path,
                    image.pixels.len(),
                    expected,
                );
            }
            let mut rgba8 = Vec::with_capacity(pixel_count * 4);
            for rgb in image.pixels.chunks_exact(3) {
                rgba8.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            Ok(rgba8)
        }
        format => Err(EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}.format", role.field()),
            reason: format!(
                "{source_relative_path} uses unsupported {} format {format:?}",
                role.human_name()
            ),
        }),
    }
}

fn rgba8_len(
    role: PbrTextureRole,
    source_relative_path: &str,
    pixel_count: usize,
) -> Result<usize, EngineError> {
    pixel_count
        .checked_mul(4)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}.size", role.field()),
            reason: format!("{source_relative_path} dimensions overflow rgba8 byte count"),
        })
}

fn rgb8_len(
    role: PbrTextureRole,
    source_relative_path: &str,
    pixel_count: usize,
) -> Result<usize, EngineError> {
    pixel_count
        .checked_mul(3)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}.size", role.field()),
            reason: format!("{source_relative_path} dimensions overflow rgb8 byte count"),
        })
}

fn invalid_texture_bytes<T>(
    role: PbrTextureRole,
    source_relative_path: &str,
    actual: usize,
    expected: usize,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.gltf.{}.bytes", role.field()),
        reason: format!(
            "{source_relative_path} decoded byte count {actual} != expected {expected}"
        ),
    })
}

fn implicit_texture_ref(role: PbrTextureRole) -> GltfTextureRef {
    GltfTextureRef {
        texture_index: None,
        image_index: None,
        source_relative_path: format!("generated://editor/gltf/{}", role.field()),
    }
}

fn implicit_texture(
    role: PbrTextureRole,
    source_relative_path: &str,
) -> Result<EditorWorldTextureData, EngineError> {
    let rgba8 = match role {
        PbrTextureRole::BaseColor => vec![255, 255, 255, 255],
        PbrTextureRole::Normal => vec![128, 128, 255, 255],
        PbrTextureRole::MetallicRoughness => vec![255, 255, 255, 255],
    };
    EditorWorldTextureData::new(role.label(), source_relative_path.to_string(), 1, 1, rgba8)
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.gltf.{}", role.field()),
            reason: error.to_string(),
        })
}

fn texture_ref(texture: gltf::Texture<'_>, scene_source_relative_path: &str) -> GltfTextureRef {
    let image = texture.source();
    GltfTextureRef {
        texture_index: Some(texture.index()),
        image_index: Some(image.index()),
        source_relative_path: image_source_relative_path(scene_source_relative_path, &image),
    }
}

fn image_source_relative_path(scene_source_relative_path: &str, image: &gltf::Image<'_>) -> String {
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
