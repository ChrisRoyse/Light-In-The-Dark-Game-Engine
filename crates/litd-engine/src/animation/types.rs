use serde::{Deserialize, Serialize};

use crate::math::{Mat4, Quat, Transform, Vec3};

pub const MAX_SKINNING_JOINTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkeletalAnimationAsset {
    pub schema: String,
    pub source_relative_path: String,
    pub node_count: usize,
    pub skin_count: usize,
    pub clip_count: usize,
    pub nodes: Vec<AnimationNode>,
    pub skins: Vec<AnimationSkin>,
    pub clips: Vec<AnimationClip>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimationNode {
    pub node_index: usize,
    pub parent_index: Option<usize>,
    pub base_transform: Transform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationSkin {
    pub skin_index: usize,
    pub name: Option<String>,
    pub skeleton_root_index: Option<usize>,
    pub joint_node_indices: Vec<usize>,
    pub joint_names: Vec<String>,
    pub inverse_bind_matrices: Vec<Mat4>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClip {
    pub clip_index: usize,
    pub name: String,
    pub duration_secs: f32,
    pub duration_ms: u32,
    pub frame_count: u32,
    pub channels: Vec<AnimationChannel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationChannel {
    pub target_node_index: usize,
    pub property: AnimationProperty,
    pub interpolation: AnimationInterpolation,
    pub times: Vec<f32>,
    pub values: AnimationChannelValues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnimationProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationInterpolation {
    Linear,
    Step,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnimationChannelValues {
    Translations(Vec<Vec3>),
    Rotations(Vec<Quat>),
    Scales(Vec<Vec3>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationSampleRequest {
    pub clip_name: String,
    pub skin_index: usize,
    pub playback_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationPose {
    pub source_relative_path: String,
    pub clip_name: String,
    pub skin_index: usize,
    pub playback_time_ms: u32,
    pub node_local_transforms: Vec<Transform>,
    pub node_global_matrices: Vec<Mat4>,
    pub joint_matrices: Vec<Mat4>,
}
