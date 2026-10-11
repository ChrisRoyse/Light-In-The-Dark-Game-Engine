use serde::{Deserialize, Serialize};

use crate::{sim::SimBodyId, sim_order::SimOrder};

use super::types::AnimationPose;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationDrivenState {
    Idle,
    Walk,
    Attack,
    Cast,
    Death,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationDrivenClipBinding {
    pub clip_name: String,
    pub looped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationDrivenClipMap {
    pub idle: AnimationDrivenClipBinding,
    pub walk: AnimationDrivenClipBinding,
    pub attack: AnimationDrivenClipBinding,
    pub cast: AnimationDrivenClipBinding,
    pub death: AnimationDrivenClipBinding,
    pub movement_threshold_raw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationPresentationSignalKind {
    Cast,
    Death,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationPresentationSignal {
    pub body_id: SimBodyId,
    pub kind: AnimationPresentationSignalKind,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationDriveRequest {
    pub body_id: SimBodyId,
    pub skin_index: usize,
    pub playback_time_ms: u32,
    pub pending_orders: Vec<SimOrder>,
    pub presentation_signals: Vec<AnimationPresentationSignal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationDrivenPose {
    pub schema: String,
    pub presentation_only: bool,
    pub body_id: SimBodyId,
    pub source_tick: u64,
    pub source_state_hash: u64,
    pub selected_state: AnimationDrivenState,
    pub selected_clip_name: String,
    pub selection_reason: String,
    pub playback_time_ms: u32,
    pub pose: AnimationPose,
}
