use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::math::{Quat, Vec2, Vec3};

use super::types::AnimationPose;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendLayer {
    pub clip_name: String,
    pub playback_time_ms: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendPoseRequest {
    pub skin_index: usize,
    pub layers: Vec<AnimationBlendLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationNormalizedBlendLayer {
    pub clip_name: String,
    pub playback_time_ms: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendPoseResult {
    pub schema: String,
    pub skin_index: usize,
    pub layer_count: usize,
    pub normalized_layers: Vec<AnimationNormalizedBlendLayer>,
    pub pose: AnimationPose,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpacePoint1d {
    pub position: f32,
    pub clip_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpace1d {
    pub name: String,
    pub parameter_name: String,
    pub points: Vec<AnimationBlendSpacePoint1d>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpace1dRequest {
    pub skin_index: usize,
    pub parameter_value: f32,
    pub playback_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpacePoint2d {
    pub position: Vec2,
    pub clip_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpace2d {
    pub name: String,
    pub parameter_x: String,
    pub parameter_y: String,
    pub points: Vec<AnimationBlendSpacePoint2d>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationBlendSpace2dRequest {
    pub skin_index: usize,
    pub parameter_value: Vec2,
    pub playback_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationGraphState {
    pub name: String,
    pub clip_name: String,
    pub looped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnimationTransitionCondition {
    Trigger { name: String },
    BoolEquals { name: String, value: bool },
    FloatGreaterEqual { name: String, threshold: f32 },
    FloatLessThan { name: String, threshold: f32 },
    ElapsedAtLeast { threshold_ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationStateTransition {
    pub from_state: String,
    pub to_state: String,
    pub condition: AnimationTransitionCondition,
    pub blend_duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationStateMachine {
    pub name: String,
    pub initial_state: String,
    pub states: Vec<AnimationGraphState>,
    pub transitions: Vec<AnimationStateTransition>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnimationStateMachineInputs {
    pub bools: BTreeMap<String, bool>,
    pub floats: BTreeMap<String, f32>,
    pub triggers: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationTransitionRuntime {
    pub from_state: String,
    pub to_state: String,
    pub elapsed_ms: u32,
    pub duration_ms: u32,
    pub from_state_time_ms: u32,
    pub to_state_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationStateMachineRuntime {
    pub active_state: String,
    pub state_time_ms: u32,
    pub transition: Option<AnimationTransitionRuntime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationStateMachineStep {
    pub schema: String,
    pub active_state: String,
    pub previous_state: Option<String>,
    pub transition_alpha: f32,
    pub triggered_transition: Option<AnimationStateTransition>,
    pub runtime: AnimationStateMachineRuntime,
    pub pose: AnimationPose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationRootMotionRequest {
    pub clip_name: String,
    pub skin_index: usize,
    pub root_node_index: Option<usize>,
    pub from_time_ms: u32,
    pub to_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationRootMotionDelta {
    pub schema: String,
    pub source_relative_path: String,
    pub clip_name: String,
    pub root_node_index: usize,
    pub from_time_ms: u32,
    pub to_time_ms: u32,
    pub from_global_translation: Vec3,
    pub to_global_translation: Vec3,
    pub translation_delta: Vec3,
    pub rotation_delta: Quat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationMontage {
    pub name: String,
    pub clip_name: String,
    pub start_time_ms: u32,
    pub end_time_ms: u32,
    pub blend_in_ms: u32,
    pub blend_out_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationMontageRuntime {
    pub elapsed_time_ms: u32,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationMontageStep {
    pub schema: String,
    pub montage_name: String,
    pub clip_name: String,
    pub playback_time_ms: u32,
    pub weight: f32,
    pub runtime: AnimationMontageRuntime,
    pub pose: AnimationPose,
}
