use std::path::Path;

use crate::{EngineError, math::Vec3};

use super::EditorWorldSceneMetadata;

pub const DEFAULT_VISUAL_PRESET_ID: &str = "litd.visual_preset.ed11_cinematic_default.v1";
pub const DEFAULT_CAMERA_PROFILE_ID: &str = "ed11_gothic_showcase_authoring_v3";

// Composition/quality references ONLY — never a HUD/behavior spec. ED12's
// minimap/quest-tracker/player-marker surfaces are OBSOLETE per ADR-944 /
// #944 / #1510 and must not be implemented in packaged player runtime.
const DEFAULT_REFERENCE_IMAGES: &[&str] = &[
    "docs/readme/ED11-world-view.png",
    "docs/readme/ED06-material-assets.png",
    "docs/readme/ED12-play-topdown.png",
    "docs/readme/GP01-town.png",
];
const DEFAULT_MATERIAL_CONTRACT: &str = "glTF-metallic-roughness;base_color=required;normal=required;metallic_roughness=required;emissive_bloom_hints=required";
const DEFAULT_LIGHTING_CONTRACT: &str = "directional=-0.45,-0.72,-0.32;power=1.25;warm_cool_emissive_pools=required;fog_rift_layers=required";
const DEFAULT_POST_CONTRACT: &str =
    "hdr=Rgba16Float;tonemap=aces;bloom=2_pass;exposure=1.02;color_grade=ed11_cinematic_veil";
const DEFAULT_CAMERA_CONTRACT: &str =
    "target=0.0,0.0,-24.0;distance=150.0;yaw=38.0;pitch=-42.0;fov=55";
const MIN_DEFAULT_SCENE_MATERIALS: u32 = 4;
const MIN_DEFAULT_SCENE_IMAGES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorWorldVisualPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub camera_profile_id: &'static str,
    pub camera_target_world: Vec3,
    pub camera_distance: f32,
    pub camera_yaw_degrees: f32,
    pub camera_pitch_degrees: f32,
    pub material_contract: &'static str,
    pub lighting_contract: &'static str,
    pub post_contract: &'static str,
    pub reference_images: &'static [&'static str],
}

impl EditorWorldVisualPreset {
    pub fn validate(self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() {
            return visual_preset_error("id", "preset id must be non-empty");
        }
        if self.camera_profile_id.trim().is_empty() {
            return visual_preset_error("camera_profile_id", "camera profile id must be non-empty");
        }
        if !self.camera_target_world.is_finite()
            || !self.camera_distance.is_finite()
            || !self.camera_yaw_degrees.is_finite()
            || !self.camera_pitch_degrees.is_finite()
            || self.camera_distance <= 0.0
            || self.camera_pitch_degrees < -89.0
            || self.camera_pitch_degrees > 89.0
        {
            return visual_preset_error(
                "camera",
                "camera target, positive distance, yaw, and pitch -89..89 must be finite",
            );
        }
        for field in [
            ("material_contract", self.material_contract),
            ("lighting_contract", self.lighting_contract),
            ("post_contract", self.post_contract),
        ] {
            if field.1.trim().is_empty() {
                return visual_preset_error(field.0, "contract string must be non-empty");
            }
        }
        if self.reference_images.len() < 4 {
            return visual_preset_error(
                "reference_images",
                "default preset must bind ED11, material, play, and gameplay references",
            );
        }
        for image in self.reference_images {
            if !image.starts_with("docs/readme/") || !image.ends_with(".png") {
                return visual_preset_error(
                    "reference_images",
                    format!("reference image '{image}' must be a docs/readme PNG"),
                );
            }
        }
        Ok(())
    }

    pub fn validate_scene_metadata(
        self,
        metadata: &EditorWorldSceneMetadata,
    ) -> Result<(), EngineError> {
        self.validate()?;
        require_texture(
            "base_color_texture",
            &metadata.base_color_texture_source_path,
            metadata.base_color_texture_width,
            metadata.base_color_texture_height,
            metadata.base_color_texture_bytes,
        )?;
        require_texture(
            "normal_texture",
            &metadata.normal_texture_source_path,
            metadata.normal_texture_width,
            metadata.normal_texture_height,
            metadata.normal_texture_bytes,
        )?;
        require_texture(
            "metallic_roughness_texture",
            &metadata.metallic_roughness_texture_source_path,
            metadata.metallic_roughness_texture_width,
            metadata.metallic_roughness_texture_height,
            metadata.metallic_roughness_texture_bytes,
        )?;
        if metadata.source_id != "litd.editor.blank_world.authoring_v1"
            && (metadata.material_count < MIN_DEFAULT_SCENE_MATERIALS
                || metadata.image_count < MIN_DEFAULT_SCENE_IMAGES)
        {
            return visual_preset_error(
                "scene_metadata",
                format!(
                    "default visual preset requires at least {MIN_DEFAULT_SCENE_MATERIALS} materials and {MIN_DEFAULT_SCENE_IMAGES} images, got materials={} images={}",
                    metadata.material_count, metadata.image_count
                ),
            );
        }
        Ok(())
    }

    pub fn validate_reference_files(self, repo_root: &Path) -> Result<(), EngineError> {
        self.validate()?;
        for image in self.reference_images {
            let path = repo_root.join(image);
            let metadata = std::fs::metadata(&path).map_err(|error| {
                visual_preset_error_value(
                    "reference_images",
                    format!("failed to read {}: {error}", path.display()),
                )
            })?;
            if !metadata.is_file() || metadata.len() == 0 {
                return visual_preset_error(
                    "reference_images",
                    format!("{} is not a non-empty file", path.display()),
                );
            }
        }
        Ok(())
    }

    pub fn reference_image_list(self) -> String {
        self.reference_images.join("|")
    }

    pub fn contract_summary(self) -> String {
        format!(
            "id={};camera_profile={};camera={};materials={};lighting={};post={};references={}",
            self.id,
            self.camera_profile_id,
            DEFAULT_CAMERA_CONTRACT,
            self.material_contract,
            self.lighting_contract,
            self.post_contract,
            self.reference_image_list()
        )
    }
}

pub fn default_editor_world_visual_preset() -> EditorWorldVisualPreset {
    EditorWorldVisualPreset {
        id: DEFAULT_VISUAL_PRESET_ID,
        label: "ED11 Cinematic Warcraft Authoring",
        camera_profile_id: DEFAULT_CAMERA_PROFILE_ID,
        camera_target_world: Vec3::new(0.0, 0.0, -24.0),
        camera_distance: 150.0,
        camera_yaw_degrees: 38.0,
        camera_pitch_degrees: -42.0,
        material_contract: DEFAULT_MATERIAL_CONTRACT,
        lighting_contract: DEFAULT_LIGHTING_CONTRACT,
        post_contract: DEFAULT_POST_CONTRACT,
        reference_images: DEFAULT_REFERENCE_IMAGES,
    }
}

fn require_texture(
    field: &'static str,
    source_path: &str,
    width: u32,
    height: u32,
    bytes: u32,
) -> Result<(), EngineError> {
    if source_path.trim().is_empty() || width == 0 || height == 0 || bytes == 0 {
        return visual_preset_error(
            field,
            format!(
                "default visual preset requires non-empty source path and texture bytes, got path='{source_path}' size={width}x{height} bytes={bytes}"
            ),
        );
    }
    Ok(())
}

fn visual_preset_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(visual_preset_error_value(field, reason))
}

fn visual_preset_error_value(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::EditorWorldRendererInvalidConfig {
        field: format!("visual_preset.{}", field.into()),
        reason: reason.into(),
    }
}
