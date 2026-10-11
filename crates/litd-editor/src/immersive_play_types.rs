use litd_engine::{math::Vec3, render::ui::HudLayoutPreset};
use serde::{Deserialize, Serialize};

use crate::play::{PlayCameraRig, PlaySessionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmersiveEntryKind {
    Cursor,
    ChosenSpawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmersiveDisplayMode {
    Windowed,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmersiveInputAction {
    MoveForward,
    Confirm,
    Orbit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersivePlayRequest {
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub player_object_id: String,
    pub camera_rig: PlayCameraRig,
    pub entry: ImmersiveEntryRequest,
    pub viewport: ImmersiveViewport,
    pub screenshot: ImmersiveScreenshotInput,
    pub input_commands: Vec<ImmersiveInputCommand>,
    pub editor_before: ImmersiveEditorStateSnapshot,
    pub editor_after_exit: ImmersiveEditorStateSnapshot,
    pub display_toggle: ImmersiveDisplayToggleProof,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveEntryRequest {
    pub kind: ImmersiveEntryKind,
    pub spawn_object_id: Option<String>,
    pub cursor_world: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveViewport {
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveScreenshotInput {
    pub path: String,
    pub expected_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveInputCommand {
    pub command_id: String,
    pub action: ImmersiveInputAction,
    pub pressed: bool,
    pub timestamp_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersiveEditorStateSnapshot {
    pub document_id: String,
    pub revision_id: String,
    pub document_fingerprint: String,
    pub selected_object_ids: Vec<String>,
    pub active_panel: String,
    pub viewport_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersiveDisplayState {
    pub mode: ImmersiveDisplayMode,
    pub monitor_index: u32,
    pub monitor_count: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub scale_factor_milli: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersiveDisplayToggleProof {
    pub before: ImmersiveDisplayState,
    pub entered_fullscreen: ImmersiveDisplayState,
    pub restored: ImmersiveDisplayState,
    pub stable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveEntryResolved {
    pub kind: ImmersiveEntryKind,
    pub player_object_id: String,
    pub source_object_id: Option<String>,
    pub world_position: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveRenderSnapshot {
    pub play_session_state: PlaySessionState,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub camera_rig: PlayCameraRig,
    pub camera_player_id: String,
    pub camera_target_world: Vec3,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub presentation_instances: usize,
    pub spawned_bodies: usize,
    pub embodied_camera: bool,
    pub full_fidelity_render_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveHudSnapshot {
    pub preset: HudLayoutPreset,
    pub viewport_width_px: u32,
    pub viewport_height_px: u32,
    pub region_count: usize,
    pub command_slot_count: u32,
    pub minimap_present: bool,
    pub command_bar_present: bool,
    pub status_strip_present: bool,
    pub reference_images: Vec<String>,
    pub hud_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersiveScreenshotEvidence {
    pub path: String,
    pub sha256: String,
    pub byte_len: usize,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveInputEvent {
    pub command_id: String,
    pub action: ImmersiveInputAction,
    pub engine_action: String,
    pub pressed: bool,
    pub timestamp_ms: f64,
    pub frame_index: u64,
    pub events_seen: u64,
    pub active_binding_count: usize,
    pub action_down: bool,
    pub just_pressed: bool,
    pub just_released: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersiveInputLog {
    pub events: Vec<ImmersiveInputEvent>,
    pub input_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersivePauseProof {
    pub paused: bool,
    pub simulation_fingerprint_before_pause: String,
    pub simulation_fingerprint_after_pause: String,
    pub stable_while_paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmersiveExitProof {
    pub before: ImmersiveEditorStateSnapshot,
    pub after: ImmersiveEditorStateSnapshot,
    pub restored_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersivePlayReadback {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub camera_rig: PlayCameraRig,
    pub entry_world_position: Vec3,
    pub render_active: bool,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub hud_active: bool,
    pub hud_regions: usize,
    pub screenshot_verified: bool,
    pub screenshot_sha256: String,
    pub input_active: bool,
    pub input_event_count: usize,
    pub pause_verified: bool,
    pub fullscreen_toggle_stable: bool,
    pub editor_restored_exact: bool,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmersivePlayLedger {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub entry: ImmersiveEntryResolved,
    pub render: ImmersiveRenderSnapshot,
    pub hud: ImmersiveHudSnapshot,
    pub screenshot: ImmersiveScreenshotEvidence,
    pub input_log: ImmersiveInputLog,
    pub pause: ImmersivePauseProof,
    pub display_toggle: ImmersiveDisplayToggleProof,
    pub exit: ImmersiveExitProof,
    pub readback: ImmersivePlayReadback,
}
