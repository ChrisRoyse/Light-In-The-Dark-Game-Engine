use serde::{Deserialize, Serialize};

use litd_engine::math::{Transform, Vec3};

use crate::{
    history::EditorDocument,
    play::{PlayCameraRig, PlaySessionState},
    semantic_authoring_ops::SemanticAuthoringReport,
    world_memory::{WorldMemoryConstellation, WorldMemorySearchHit},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextFeedbackAction {
    SessionStarted,
    FeedbackCaptured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackSessionRequest {
    pub ledger_id: String,
    pub session_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub correlation_id: String,
    pub traceparent: String,
    pub camera_rig: PlayCameraRig,
    pub max_feedback_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackMarker {
    pub target_object_id: String,
    pub label: String,
    pub world_position: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackScreenshotInput {
    pub path: String,
    pub expected_sha256: String,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackCaptureRequest {
    pub feedback_id: String,
    pub actor_id: String,
    pub note: String,
    pub severity: String,
    pub marker: ContextFeedbackMarker,
    pub screenshot: ContextFeedbackScreenshotInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackSessionSnapshot {
    pub play_session_state: PlaySessionState,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub source_fingerprint: String,
    pub generation: u64,
    pub camera_rig: PlayCameraRig,
    pub sim_seed: u64,
    pub spawned_bodies: usize,
    pub visible_instances: usize,
    pub draw_batches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackBody {
    pub purpose: String,
    pub value: String,
    pub format: String,
    pub creator: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackSelector {
    pub selector_type: String,
    pub marker_object_id: String,
    pub marker_label: String,
    pub coordinate_space: String,
    pub world_position: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackTargetState {
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackTarget {
    pub source: String,
    pub selector: ContextFeedbackSelector,
    pub state: ContextFeedbackTargetState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackAnnotation {
    pub annotation_id: String,
    pub motivation: String,
    pub body: ContextFeedbackBody,
    pub target: ContextFeedbackTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackScreenshotEvidence {
    pub source_path: String,
    pub media_type: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub blob: ContextFeedbackScreenshotBlobRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFeedbackScreenshotBlobRef {
    pub blob_id: String,
    pub chunk_row_prefix: String,
    pub chunk_count: usize,
    pub chunk_size_bytes: usize,
    pub total_bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackWorldState {
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub target_object_id: String,
    pub target_object_kind: String,
    pub target_transform: Transform,
    pub target_object_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackRepro {
    pub camera_rig: PlayCameraRig,
    pub sim_seed: u64,
    pub generation: u64,
    pub play_session_state: PlaySessionState,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub spawned_bodies: usize,
    pub visible_instances: usize,
    pub draw_batches: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackSextantReadback {
    pub query: String,
    pub world_memory_hit_count: usize,
    pub self_constellation_id: String,
    pub self_hit_rank: usize,
    pub self_found: bool,
    pub definition: SemanticAuthoringReport,
    pub neighbors: SemanticAuthoringReport,
    pub hits: Vec<WorldMemorySearchHit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackLoomAssociation {
    pub association_id: String,
    pub source_feedback_id: String,
    pub duplicate_feedback_ids: Vec<String>,
    pub highest_overlap: f32,
    pub matched_target_object_id: String,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackRecord {
    pub feedback_id: String,
    pub actor_id: String,
    pub severity: String,
    pub annotation: ContextFeedbackAnnotation,
    pub screenshot: ContextFeedbackScreenshotEvidence,
    pub world_state: ContextFeedbackWorldState,
    pub repro: ContextFeedbackRepro,
    pub constellation: WorldMemoryConstellation,
    pub sextant: ContextFeedbackSextantReadback,
    pub loom: ContextFeedbackLoomAssociation,
    pub editor_route: String,
    pub agent_route: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackEvent {
    pub event_id: String,
    pub sequence: usize,
    pub action: ContextFeedbackAction,
    pub actor_id: String,
    pub code: String,
    pub details: String,
    pub feedback: Option<ContextFeedbackRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackReadback {
    pub schema: String,
    pub session_id: String,
    pub project_id: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub feedback_count: usize,
    pub screenshot_count: usize,
    pub marker_count: usize,
    pub latest_feedback_id: Option<String>,
    pub latest_constellation_id: Option<String>,
    pub latest_sextant_self_found: bool,
    pub latest_loom_duplicate_count: usize,
    pub world_state_attached: bool,
    pub repro_attached: bool,
    pub screenshot_blob_retrievable: bool,
    pub gui_route: String,
    pub agent_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFeedbackLedger {
    pub schema: String,
    pub ledger_id: String,
    pub session: ContextFeedbackSessionRequest,
    pub source_document: EditorDocument,
    pub session_snapshot: ContextFeedbackSessionSnapshot,
    pub gui_route: String,
    pub agent_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub event_count: usize,
    pub events: Vec<ContextFeedbackEvent>,
    pub readback: ContextFeedbackReadback,
}

impl ContextFeedbackSessionRequest {
    pub fn new(
        ledger_id: impl Into<String>,
        session_id: impl Into<String>,
        project_id: impl Into<String>,
        actor_id: impl Into<String>,
        camera_rig: PlayCameraRig,
    ) -> Self {
        Self {
            ledger_id: ledger_id.into(),
            session_id: session_id.into(),
            project_id: project_id.into(),
            actor_id: actor_id.into(),
            correlation_id: "issue1184-context-feedback-correlation".to_string(),
            traceparent: "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
            camera_rig,
            max_feedback_count: 64,
        }
    }
}
