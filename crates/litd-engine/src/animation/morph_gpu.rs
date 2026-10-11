use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::{
    EngineError,
    animation::{MorphSampleRequest, MorphTargetAnimationAsset, sample_morph_target_weights},
};

pub const MORPH_WEIGHT_BIND_GROUP: u32 = 4;
pub const MORPH_WEIGHT_BINDING: u32 = 0;
pub const MORPH_DELTA_BINDING: u32 = 1;
pub const MORPH_SHADER_ID: &str = "litd.morphing.vertex";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphWeightUpload {
    pub schema: String,
    pub source_relative_path: String,
    pub clip_name: String,
    pub target_node_index: usize,
    pub mesh_index: usize,
    pub playback_time_ms: u32,
    pub target_count: usize,
    pub byte_count: usize,
    pub bind_group: u32,
    pub binding: u32,
    pub shader_id: String,
    pub weights: Vec<f32>,
}

impl MorphWeightUpload {
    pub fn bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.weights)
    }

    pub fn weight_hash(&self) -> String {
        litd_hash::fnv1a64_hex(self.bytes())
    }

    pub fn buffer_label(&self) -> String {
        format!(
            "litd-morph-weights-{}-node{}-{}ms",
            self.clip_name, self.target_node_index, self.playback_time_ms
        )
    }
}

pub fn build_morph_weight_upload(
    asset: &MorphTargetAnimationAsset,
    request: &MorphSampleRequest,
) -> Result<MorphWeightUpload, EngineError> {
    let sample = sample_morph_target_weights(asset, request.clone())?;
    if sample.weights.is_empty() || sample.weights.iter().any(|weight| !weight.is_finite()) {
        return Err(EngineError::AnimationInvalidConfig {
            field: "animation.morph.weights".to_string(),
            reason: "morph weight upload requires non-empty finite weights".to_string(),
        });
    }
    Ok(MorphWeightUpload {
        schema: "litd.morph_weight_upload.v1".to_string(),
        source_relative_path: sample.source_relative_path,
        clip_name: sample.clip_name,
        target_node_index: sample.target_node_index,
        mesh_index: sample.mesh_index,
        playback_time_ms: sample.playback_time_ms,
        target_count: sample.weights.len(),
        byte_count: sample.weights.len() * std::mem::size_of::<f32>(),
        bind_group: MORPH_WEIGHT_BIND_GROUP,
        binding: MORPH_WEIGHT_BINDING,
        shader_id: MORPH_SHADER_ID.to_string(),
        weights: sample.weights,
    })
}

pub fn create_morph_weight_buffer(
    device: &wgpu::Device,
    upload: &MorphWeightUpload,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&upload.buffer_label()),
        contents: upload.bytes(),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}
