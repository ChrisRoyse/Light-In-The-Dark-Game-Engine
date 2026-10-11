//! Serde type definitions for the edit-from-play bridge (#1466: extracted from
//! `edit_from_play.rs` to keep the parent module under the 500-line doctrine).
//!
//! These are the wire/ledger types exchanged by the pause/resume/readback tools;
//! they are re-exported from the parent so external paths are unchanged.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditFromPlayAction {
    PausedAndFocused,
    ResumedAfterEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditFromPlayState {
    Paused,
    Resumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditFromPlayRequest {
    pub ledger_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub target_object_id: String,
    pub camera_rig: PlayCameraRig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayResumeRequest {
    pub actor_id: String,
    pub edit_id: String,
    pub target_object_id: String,
    pub edited_document: EditorDocument,
    pub edit_elapsed_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayTargetReadback {
    pub target_object_id: String,
    pub body_id: u32,
    pub object_kind: String,
    pub camera_rig: PlayCameraRig,
    pub play_state: PlaySessionState,
    pub paused_translation: Vec3,
    pub paused_snapshot_hash: u64,
    pub source_fingerprint: String,
    pub play_build_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayFocusReadback {
    pub focused_object_id: String,
    pub selected_object_ids: Vec<String>,
    pub camera: WorldViewCameraState,
    pub camera_target_matches_object: bool,
    pub source_document_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayResumeReadback {
    pub edit_id: String,
    pub edit_elapsed_ticks: u64,
    pub before_document_fingerprint: String,
    pub after_document_fingerprint: String,
    pub resumed_play_build_fingerprint: String,
    pub target_before_translation: Vec3,
    pub target_after_translation: Vec3,
    pub target_changed: bool,
    pub live_change_visible: bool,
    pub preserved_paused_snapshot_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditFromPlayEvent {
    pub event_id: String,
    pub sequence: usize,
    pub action: EditFromPlayAction,
    pub actor_id: String,
    pub target_object_id: String,
    pub code: String,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayReadback {
    pub schema: String,
    pub session_id: String,
    pub state: EditFromPlayState,
    pub target_object_id: String,
    pub editor_focus_matches_play_target: bool,
    pub selected_object_ids: Vec<String>,
    pub source_document_fingerprint: String,
    pub current_document_fingerprint: String,
    pub paused_snapshot_hash: u64,
    pub resumed_play_build_fingerprint: Option<String>,
    pub live_change_visible: bool,
    pub round_trip_preserves_play_state: bool,
    pub edit_elapsed_ticks: Option<u64>,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditFromPlayLedger {
    pub schema: String,
    pub ledger_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub pause_tool_name: String,
    pub resume_tool_name: String,
    pub readback_tool_name: String,
    pub state: EditFromPlayState,
    pub target: EditFromPlayTargetReadback,
    pub focus: EditFromPlayFocusReadback,
    pub source_document: EditorDocument,
    pub current_document: EditorDocument,
    pub resume: Option<EditFromPlayResumeReadback>,
    pub event_count: usize,
    pub events: Vec<EditFromPlayEvent>,
    pub readback: EditFromPlayReadback,
}
