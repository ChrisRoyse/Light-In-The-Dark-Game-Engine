use litd_engine::{sim_order::SimOrder, sim_replay::SimReplayRecord};
use serde::{Deserialize, Serialize};

use crate::{history::EditorDocument, play::PlayCameraRig};

#[path = "play_session_capture_store.rs"]
mod play_session_capture_store;

#[path = "play_session_capture_capture.rs"]
mod play_session_capture_capture;

#[path = "play_session_capture_build.rs"]
mod play_session_capture_build;

#[path = "play_session_capture_validate.rs"]
mod play_session_capture_validate;

pub use play_session_capture_capture::{
    capture_play_session, restart_session_again, toggle_ab_variant,
    validate_play_session_capture_ledger,
};
pub use play_session_capture_store::{
    PlaySessionCaptureAsterReadback, parse_play_session_capture_ledger,
    read_play_session_capture_ledger, write_play_session_capture_ledger,
};

pub(crate) use play_session_capture_build::*;
pub(crate) use play_session_capture_validate::*;

pub const PLAY_SESSION_CAPTURE_SCHEMA: &str = "litd.play_session.capture.v1";
pub const PLAY_SESSION_CAPTURE_LEDGER_SCHEMA: &str = "litd.play_session.capture_ledger.v1";
pub const PLAY_SESSION_CAPTURE_GUI_ROUTE: &str = "egui://editor.project/session_capture";
pub const PLAY_SESSION_CAPTURE_TOOL: &str = "litd.editor.play.capture_session";
pub const PLAY_SESSION_RESTART_TOOL: &str = "litd.editor.play.restart_again";
pub const PLAY_SESSION_VARIANT_TOOL: &str = "litd.editor.play.toggle_ab_variant";
pub const PLAY_SESSION_CAPTURE_READBACK_TOOL: &str = "litd.editor.play.read_session_capture";
pub const PLAY_SESSION_LOWERING_BOUNDARY: &str = "litd.sim_order.lowering_v1";
pub const PLAY_SESSION_RESTART_COMMAND: &str = "again";
pub const MAX_CAPTURE_BODIES: usize = 16;
pub const MAX_CAPTURE_ORDERS: usize = 32;
pub const MAX_CAPTURE_TICKS: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaySessionCaptureAction {
    Captured,
    RestartedAgain,
    VariantToggled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySessionVariantInput {
    pub variant_id: String,
    pub label: String,
    pub document: EditorDocument,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySessionCaptureRequest {
    pub ledger_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub active_variant_id: String,
    pub camera_rig: PlayCameraRig,
    pub restart_command: String,
    pub run_ticks: u64,
    pub checkpoint_tick: u64,
    pub orders: Vec<SimOrder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySessionVariantSnapshot {
    pub variant_id: String,
    pub label: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub initial_state_hash: u64,
    pub source_revision_id: String,
    pub spawned_bodies: usize,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub document: EditorDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionReplayProof {
    pub replay_schema_version: u32,
    pub lowering_boundary: String,
    pub record_fingerprint: String,
    pub initial_hash: u64,
    pub expected_final_hash: u64,
    pub replayed_final_hash: u64,
    pub final_tick: u64,
    pub steps: u64,
    pub orders_applied: u64,
    pub deterministic_match: bool,
    pub record: SimReplayRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionCheckpoint {
    pub checkpoint_id: String,
    pub checkpoint_tick: u64,
    pub checkpoint_hash: u64,
    pub remaining_ticks: u64,
    pub remaining_order_count: usize,
    pub restart_record_fingerprint: String,
    pub restart_record: SimReplayRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionRestartReadback {
    pub command: String,
    pub checkpoint_id: String,
    pub checkpoint_tick: u64,
    pub restart_final_hash: u64,
    pub full_replay_final_hash: u64,
    pub final_tick: u64,
    pub orders_applied: u64,
    pub matches_full_replay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionCaptureEvent {
    pub event_id: String,
    pub sequence: usize,
    pub action: PlaySessionCaptureAction,
    pub actor_id: String,
    pub variant_id: String,
    pub code: String,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaySessionCaptureReadback {
    pub schema: String,
    pub session_id: String,
    pub active_variant_id: String,
    pub active_document_fingerprint: String,
    pub variant_count: usize,
    pub variant_catalog_fingerprint: String,
    pub a_b_isolated: bool,
    pub run_ticks: u64,
    pub checkpoint_tick: u64,
    pub full_replay_final_hash: u64,
    pub replay_final_hash_matches: bool,
    pub restart_final_hash: u64,
    pub restart_matches_full_replay: bool,
    pub lowering_boundary: String,
    pub lowered_order_count: usize,
    pub human_order_count: usize,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySessionCaptureLedger {
    pub schema: String,
    pub ledger_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub restart_tool_name: String,
    pub variant_tool_name: String,
    pub readback_tool_name: String,
    pub active_variant_id: String,
    pub variants: Vec<PlaySessionVariantSnapshot>,
    pub replay: PlaySessionReplayProof,
    pub checkpoint: PlaySessionCheckpoint,
    pub restart: PlaySessionRestartReadback,
    pub event_count: usize,
    pub events: Vec<PlaySessionCaptureEvent>,
    pub readback: PlaySessionCaptureReadback,
}

pub struct PlaySessionCaptureRequestArgs<
    T0: Into<String>,
    T1: Into<String>,
    T2: Into<String>,
    T3: Into<String>,
> {
    pub ledger_id: T0,
    pub session_id: T1,
    pub actor_id: T2,
    pub active_variant_id: T3,
    pub camera_rig: PlayCameraRig,
    pub run_ticks: u64,
    pub checkpoint_tick: u64,
    pub orders: Vec<SimOrder>,
}

impl PlaySessionCaptureRequest {
    pub fn new(
        args: PlaySessionCaptureRequestArgs<
            impl Into<String>,
            impl Into<String>,
            impl Into<String>,
            impl Into<String>,
        >,
    ) -> Self {
        let PlaySessionCaptureRequestArgs {
            ledger_id,
            session_id,
            actor_id,
            active_variant_id,
            camera_rig,
            run_ticks,
            checkpoint_tick,
            orders,
        } = args;
        Self {
            ledger_id: ledger_id.into(),
            session_id: session_id.into(),
            actor_id: actor_id.into(),
            active_variant_id: active_variant_id.into(),
            camera_rig,
            restart_command: PLAY_SESSION_RESTART_COMMAND.to_string(),
            run_ticks,
            checkpoint_tick,
            orders,
        }
    }
}

#[cfg(test)]
#[path = "play_session_capture_tests.rs"]
mod tests;
