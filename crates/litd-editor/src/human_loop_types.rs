use serde::{Deserialize, Serialize};

use crate::play::{PlayCameraRig, PlaySessionState};

use super::{
    HUMAN_LOOP_FEEDBACK_TOOL, HUMAN_LOOP_GUI_ROUTE, HUMAN_LOOP_LAUNCH_TOOL,
    HUMAN_LOOP_READBACK_TOOL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanLoopEntryPoint {
    Gui,
    McpTool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanLoopActorKind {
    Human,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanLoopAction {
    Configured,
    Launched,
    FeedbackCaptured,
    AgentVerdictRecorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanLoopSessionState {
    Configured,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopPermission {
    pub actor_id: String,
    pub actor_kind: HumanLoopActorKind,
    pub can_launch: bool,
    pub can_submit_feedback: bool,
    pub can_record_agent_verdict: bool,
    pub can_inspect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopSessionConfig {
    pub session_id: String,
    pub project_id: String,
    pub owner_actor_id: String,
    pub correlation_id: String,
    pub traceparent: String,
    pub max_human_feedback: usize,
    pub allow_concurrent_agent_verdicts: bool,
    pub gui_route: String,
    pub mcp_launch_tool_name: String,
    pub mcp_feedback_tool_name: String,
    pub readback_tool_name: String,
    pub permissions: Vec<HumanLoopPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopLaunchRequest {
    pub ledger_id: String,
    pub entry_point: HumanLoopEntryPoint,
    pub actor_id: String,
    pub config: HumanLoopSessionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopPlaySnapshot {
    pub play_session_state: PlaySessionState,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub source_fingerprint: String,
    pub generation: u64,
    pub camera_rig: PlayCameraRig,
    pub camera_player_id: String,
    pub spawned_bodies: usize,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub sim_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopFeedback {
    pub feedback_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub summary: String,
    pub severity: String,
    pub referenced_object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopAgentVerdict {
    pub verdict_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub verdict: String,
    pub confidence_basis: String,
    pub artifact_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopEvent {
    pub event_id: String,
    pub sequence: usize,
    pub action: HumanLoopAction,
    pub entry_point: HumanLoopEntryPoint,
    pub actor_id: String,
    pub code: String,
    pub details: String,
    pub play_snapshot: Option<HumanLoopPlaySnapshot>,
    pub feedback: Option<HumanLoopFeedback>,
    pub agent_verdict: Option<HumanLoopAgentVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopReadback {
    pub schema: String,
    pub session_id: String,
    pub project_id: String,
    pub owner_actor_id: String,
    pub state: HumanLoopSessionState,
    pub correlation_id: String,
    pub traceparent: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_fingerprint: String,
    pub play_session_fingerprint: String,
    pub play_build_fingerprint: String,
    pub camera_rig: PlayCameraRig,
    pub spawned_bodies: usize,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub human_feedback_count: usize,
    pub agent_verdict_count: usize,
    pub participant_count: usize,
    pub permission_fingerprint: String,
    pub last_entry_point: HumanLoopEntryPoint,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopLedger {
    pub schema: String,
    pub ledger_id: String,
    pub config: HumanLoopSessionConfig,
    pub event_count: usize,
    pub events: Vec<HumanLoopEvent>,
    pub readback: HumanLoopReadback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanLoopParityReport {
    pub schema: String,
    pub gui_session_id: String,
    pub tool_session_id: String,
    pub parity: bool,
    pub matching_fields: Vec<String>,
    pub mismatched_fields: Vec<String>,
}

impl HumanLoopSessionConfig {
    pub fn new(
        session_id: impl Into<String>,
        project_id: impl Into<String>,
        owner_actor_id: impl Into<String>,
        correlation_id: impl Into<String>,
        traceparent: impl Into<String>,
        permissions: Vec<HumanLoopPermission>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            project_id: project_id.into(),
            owner_actor_id: owner_actor_id.into(),
            correlation_id: correlation_id.into(),
            traceparent: traceparent.into(),
            max_human_feedback: 32,
            allow_concurrent_agent_verdicts: true,
            gui_route: HUMAN_LOOP_GUI_ROUTE.to_string(),
            mcp_launch_tool_name: HUMAN_LOOP_LAUNCH_TOOL.to_string(),
            mcp_feedback_tool_name: HUMAN_LOOP_FEEDBACK_TOOL.to_string(),
            readback_tool_name: HUMAN_LOOP_READBACK_TOOL.to_string(),
            permissions,
        }
    }
}
