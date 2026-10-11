use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    asset_products::{AssetProductKind, product_from_vault_record},
    asset_scene::load_editor_world_asset_scene,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    gltf_asset_import::import_gltf_asset_receipt,
};

const RENDERER_PATH: &str = "EditorWorldSceneBuffers";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelPreviewRequest {
    pub selected_animation: Option<String>,
    pub playback_time_ms: u32,
    pub camera: ModelPreviewCamera,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelPreviewCamera {
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelPreviewPlaybackState {
    Static,
    Playing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPreviewAnimationClip {
    pub clip_index: usize,
    pub name: String,
    pub duration_ms: u32,
    pub channel_count: usize,
    pub sampler_count: usize,
    pub target_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPreviewRuntime {
    pub previewer_id: String,
    pub renderer_path: String,
    pub uses_viewport_render_path: bool,
    pub product_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub verified_file_sha256: String,
    pub source_bytes: u64,
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub node_count: usize,
    pub skin_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub image_count: usize,
    pub gltf_import_verified: bool,
    pub unique_material_texture_set_count: usize,
    pub joint_count: usize,
    pub skinned_vertex_count: usize,
    pub weighted_vertex_count: usize,
    pub morph_target_count: usize,
    pub animation_channel_count: usize,
    pub animation_sampler_count: usize,
    pub animation_pose_applied_to_scene_buffers: bool,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub attachment_points: Vec<String>,
    pub animation_clips: Vec<ModelPreviewAnimationClip>,
    pub selected_animation: Option<String>,
    pub playback_state: ModelPreviewPlaybackState,
    pub playback_time_ms: u32,
    pub camera: ModelPreviewCamera,
}

impl Default for ModelPreviewCamera {
    fn default() -> Self {
        Self {
            yaw_degrees: 35.0,
            pitch_degrees: -18.0,
            distance: 128.0,
        }
    }
}

impl ModelPreviewCamera {
    pub fn validate(self) -> Result<(), EngineError> {
        if !self.yaw_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || !self.distance.is_finite()
            || self.pitch_degrees < -89.0
            || self.pitch_degrees > 89.0
            || self.distance <= 0.0
        {
            return invalid_previewer(
                "camera",
                "yaw, pitch -89..89, and positive distance must be finite",
            );
        }
        Ok(())
    }
}

impl ModelPreviewRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        self.camera.validate()
    }
}

pub fn load_model_previewer_from_default_catalog(
    relative_path: &str,
    request: ModelPreviewRequest,
) -> Result<ModelPreviewRuntime, EngineError> {
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let record = catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_previewer.record".to_string(),
            reason: format!("missing asset vault record {relative_path}"),
        })?;
    load_model_previewer_from_record(record, request)
}

pub fn load_model_previewer_from_record(
    record: &AssetVaultRecord,
    request: ModelPreviewRequest,
) -> Result<ModelPreviewRuntime, EngineError> {
    request.validate()?;
    let verified_file_sha256 = verify_record_hash(record)?;
    let product = product_from_vault_record(record)?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_previewer("product.kind", "model previewer requires a mesh product");
    }
    let extension = Path::new(&product.source_relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_previewer(
            "source.extension",
            "model previewer currently accepts only glTF/GLB products on the viewport render path",
        );
    }
    let import_receipt = import_gltf_asset_receipt(&product)?;
    let loaded_scene = load_editor_world_asset_scene(&product)?;
    let scene_report = loaded_scene.report;
    let metadata = read_gltf_preview_metadata(
        Path::new(&product.source_absolute_path),
        &product.source_relative_path,
    )?;
    let selected_animation = selected_animation(&metadata.animation_clips, &request)?;
    let playback_state = if selected_animation.is_some() {
        ModelPreviewPlaybackState::Playing
    } else {
        ModelPreviewPlaybackState::Static
    };
    Ok(ModelPreviewRuntime {
        previewer_id: format!("litd.model_previewer.{}", product.deterministic_sub_id),
        renderer_path: RENDERER_PATH.to_string(),
        uses_viewport_render_path: true,
        product_id: product.product_id,
        source_relative_path: product.source_relative_path,
        source_sha256: product.source_sha256.clone(),
        verified_file_sha256,
        source_bytes: product.source_bytes,
        mesh_count: metadata.mesh_count,
        primitive_count: scene_report.mesh_primitives as usize,
        node_count: metadata.node_count,
        skin_count: metadata.skin_count,
        material_count: scene_report.material_count,
        texture_count: scene_report.texture_count,
        image_count: scene_report.image_count,
        gltf_import_verified: import_receipt.verified_file_sha256 == product.source_sha256,
        unique_material_texture_set_count: import_receipt.unique_material_texture_set_count,
        joint_count: import_receipt.joint_count,
        skinned_vertex_count: import_receipt.skinned_vertex_count,
        weighted_vertex_count: import_receipt.weighted_vertex_count,
        morph_target_count: import_receipt.morph_target_count,
        animation_channel_count: import_receipt.animation_channel_count,
        animation_sampler_count: import_receipt.animation_sampler_count,
        animation_pose_applied_to_scene_buffers: false,
        vertex_count: scene_report.vertex_count,
        index_count: scene_report.index_count,
        triangle_count: scene_report.triangle_count,
        bounds_min: scene_report.fitted_bounds_min,
        bounds_max: scene_report.fitted_bounds_max,
        attachment_points: metadata.attachment_points,
        animation_clips: metadata.animation_clips,
        selected_animation,
        playback_state,
        playback_time_ms: request.playback_time_ms,
        camera: request.camera,
    })
}

fn selected_animation(
    clips: &[ModelPreviewAnimationClip],
    request: &ModelPreviewRequest,
) -> Result<Option<String>, EngineError> {
    if clips.is_empty() {
        if let Some(name) = &request.selected_animation {
            return invalid_previewer(
                "animation.selected",
                format!("cannot select animation '{name}' because the model has no animations"),
            );
        }
        if request.playback_time_ms != 0 {
            return invalid_previewer(
                "animation.playback_time_ms",
                "static model preview playback time must be 0",
            );
        }
        return Ok(None);
    }
    let Some(selected) = request.selected_animation.clone() else {
        if request.playback_time_ms != 0 {
            return invalid_previewer(
                "animation.playback_time_ms",
                "playback time requires a selected animation and implemented pose application",
            );
        }
        return Ok(None);
    };
    let Some(clip) = clips.iter().find(|clip| clip.name == selected) else {
        return invalid_previewer(
            "animation.selected",
            format!("model does not contain requested animation '{selected}'"),
        );
    };
    if request.playback_time_ms > clip.duration_ms {
        return invalid_previewer(
            "animation.playback_time_ms",
            format!(
                "playback time {}ms exceeds selected clip '{}' duration {}ms",
                request.playback_time_ms, clip.name, clip.duration_ms
            ),
        );
    }
    invalid_previewer(
        "animation.playback",
        format!(
            "glTF animation clip '{selected}' is imported, but viewport pose application is not implemented"
        ),
    )
}

struct GltfPreviewMetadata {
    mesh_count: usize,
    node_count: usize,
    skin_count: usize,
    attachment_points: Vec<String>,
    animation_clips: Vec<ModelPreviewAnimationClip>,
}

fn read_gltf_preview_metadata(
    source_path: &Path,
    source_relative_path: &str,
) -> Result<GltfPreviewMetadata, EngineError> {
    let (document, buffers, _images) =
        gltf::import(source_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_previewer.gltf.import".to_string(),
            reason: format!(
                "{} failed preview metadata import: {error}",
                source_path.display()
            ),
        })?;
    Ok(GltfPreviewMetadata {
        mesh_count: document.meshes().count(),
        node_count: document.nodes().count(),
        skin_count: document.skins().count(),
        attachment_points: collect_attachment_points(&document),
        animation_clips: collect_animation_clips(&document, &buffers, source_relative_path)?,
    })
}

fn collect_animation_clips(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    source_relative_path: &str,
) -> Result<Vec<ModelPreviewAnimationClip>, EngineError> {
    let mut clips = Vec::new();
    for (clip_index, animation) in document.animations().enumerate() {
        let name = animation
            .name()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("animation_{clip_index}"));
        let mut duration_secs = 0.0_f32;
        let mut targets = Vec::new();
        let channel_count = animation.channels().count();
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
            let inputs =
                reader
                    .read_inputs()
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_previewer.gltf.animation.inputs".to_string(),
                        reason: format!(
                            "{source_relative_path} animation '{name}' has no time input"
                        ),
                    })?;
            for time in inputs {
                if !time.is_finite() || time < 0.0 {
                    return invalid_previewer(
                        "animation.time",
                        format!("{source_relative_path} animation '{name}' has invalid time"),
                    );
                }
                duration_secs = duration_secs.max(time);
            }
            targets.push(format!(
                "node{}:{:?}",
                channel.target().node().index(),
                channel.target().property()
            ));
        }
        clips.push(ModelPreviewAnimationClip {
            clip_index,
            name,
            duration_ms: (duration_secs * 1000.0).round() as u32,
            channel_count,
            sampler_count: animation.samplers().count(),
            target_summary: targets.join("|"),
        });
    }
    Ok(clips)
}

fn collect_attachment_points(document: &gltf::Document) -> Vec<String> {
    let mut points = Vec::new();
    for node in document.nodes() {
        let Some(name) = node.name() else {
            continue;
        };
        let key = name.to_ascii_lowercase();
        if key.contains("attachment") || key.contains("attach") || key.contains("socket") {
            points.push(format!("node{}:{name}", node.index()));
        }
    }
    points
}

fn verify_record_hash(record: &AssetVaultRecord) -> Result<String, EngineError> {
    let bytes =
        fs::read(&record.absolute_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_previewer.source.read".to_string(),
            reason: format!("failed to read {}: {error}", record.absolute_path.display()),
        })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());
    if hash != record.sha256 {
        return invalid_previewer(
            "source.sha256",
            format!(
                "{} manifest sha256 {} != actual {}",
                record.relative_path, record.sha256, hash
            ),
        );
    }
    Ok(hash)
}

fn invalid_previewer<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_previewer.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "asset_previewer_tests.rs"]
mod tests;
