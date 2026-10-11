use serde::{Deserialize, Serialize};

use crate::animation::AnimationInterpolation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphTargetAnimationAsset {
    pub schema: String,
    pub source_relative_path: String,
    pub mesh_count: usize,
    pub node_count: usize,
    pub clip_count: usize,
    pub meshes: Vec<MorphMesh>,
    pub clips: Vec<MorphWeightClip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphMesh {
    pub mesh_index: usize,
    pub name: Option<String>,
    pub target_count: usize,
    pub default_weights: Vec<f32>,
    pub primitives: Vec<MorphPrimitiveSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphPrimitiveSummary {
    pub primitive_index: usize,
    pub vertex_count: usize,
    pub target_count: usize,
    pub position_delta_count: usize,
    pub normal_delta_count: usize,
    pub tangent_delta_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphWeightClip {
    pub clip_index: usize,
    pub name: String,
    pub duration_secs: f32,
    pub duration_ms: u32,
    pub frame_count: u32,
    pub non_morph_channel_count: usize,
    pub channels: Vec<MorphWeightChannel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphWeightChannel {
    pub target_node_index: usize,
    pub mesh_index: usize,
    pub target_count: usize,
    pub interpolation: AnimationInterpolation,
    pub times: Vec<f32>,
    pub weights: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphSampleRequest {
    pub clip_name: String,
    pub target_node_index: usize,
    pub playback_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphWeightSample {
    pub source_relative_path: String,
    pub clip_name: String,
    pub target_node_index: usize,
    pub mesh_index: usize,
    pub playback_time_ms: u32,
    pub weights: Vec<f32>,
}
