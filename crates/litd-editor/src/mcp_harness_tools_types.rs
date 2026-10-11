use litd_engine::sim_replay::SimSnapshot;
use serde::{Deserialize, Serialize};

use super::mcp_harness_tools_oracle_types::{HarnessOracleKind, HarnessOracleRunRecord};
use crate::play::PlayCameraRig;
use crate::{
    authoring::{AssayBalanceProfile, GroundedAbilityRequest, WardPolicy, WorldCorpus},
    world_memory::WorldMemorySearchCalibration,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessMutationContext {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub aster_dir: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunHeadlessHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub run_id: String,
    pub project_path: String,
    pub camera_rig: PlayCameraRig,
    pub ticks: u64,
    #[serde(default)]
    pub until_condition: Option<HarnessRunUntilCondition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotBranchHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub run_id: String,
    pub snapshot_id: String,
    pub branch_id: String,
    pub branch_ticks: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub verdict_id: String,
    pub run_id: String,
    pub assertions: Vec<HarnessAssertion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOraclesHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub oracle_run_id: String,
    pub run_id: String,
    pub oracles: Vec<HarnessOracleKind>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotDiffHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub diff_id: String,
    pub baseline_path: String,
    pub candidate_path: String,
    pub max_changed_bytes: u64,
    #[serde(default)]
    pub max_mean_abs_error: f32,
    #[serde(default)]
    pub render_reference_scene: bool,
    #[serde(default)]
    pub render_width: Option<u32>,
    #[serde(default)]
    pub render_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxSignalsHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub signal_batch_id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub authored_content: Vec<HarnessCalyxAuthoredContent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitFeedbackHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub feedback_id: String,
    pub feedback: Vec<HarnessFeedbackInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessFeedbackInput {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub remediation: String,
    #[serde(default)]
    pub actual: String,
    #[serde(default)]
    pub expected: String,
    #[serde(default)]
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPevHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub pev_id: String,
    pub run_id: String,
    pub project_path: String,
    pub camera_rig: PlayCameraRig,
    pub ticks: u64,
    pub assertions: Vec<HarnessAssertion>,
    pub oracles: Vec<HarnessOracleKind>,
    pub transcript_id: Option<String>,
    pub intent: Option<String>,
    pub acceptance_criteria: Option<Vec<String>>,
    pub max_turns: Option<u32>,
    pub turns: Option<Vec<HarnessPevLoopTurnInput>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadHarnessArgs {
    pub state_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadHarnessEventsArgs {
    pub state_path: String,
    pub after_event_index: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessPevLoopTurnInput {
    pub turn_id: String,
    pub author_delta: String,
    pub changed_paths: Vec<String>,
    pub ticks: u64,
    pub assertions: Vec<HarnessAssertion>,
    pub oracles: Vec<HarnessOracleKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessRunRecord {
    pub run_id: String,
    pub project_id: String,
    pub project_path: String,
    pub camera_rig: PlayCameraRig,
    #[serde(default = "default_run_mode")]
    pub run_mode: HarnessRunMode,
    #[serde(default = "default_render_mode")]
    pub render_mode: HarnessRunRenderMode,
    #[serde(default)]
    pub until_condition: Option<HarnessRunUntilCondition>,
    #[serde(default)]
    pub termination_reason: String,
    pub ticks_requested: u64,
    pub start_tick: u64,
    pub final_tick: u64,
    pub steps: u64,
    pub bodies_updated: u64,
    pub initial_state_hash: u64,
    pub final_state_hash: u64,
    pub spawned_bodies: usize,
    pub visible_instance_count: usize,
    pub draw_batches: usize,
    pub verdict: String,
    pub initial_snapshot: SimSnapshot,
    pub final_snapshot: SimSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessRunMode {
    FixedTicks,
    UntilCondition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessRunRenderMode {
    HeadlessNoRender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessRunUntilCondition {
    TickAtLeast { tick: u64 },
    BodyCountAtLeast { body_count: usize },
    StateHashEquals { state_hash: u64 },
}

fn default_run_mode() -> HarnessRunMode {
    HarnessRunMode::FixedTicks
}

fn default_render_mode() -> HarnessRunRenderMode {
    HarnessRunRenderMode::HeadlessNoRender
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessSnapshotBranchRecord {
    pub snapshot_id: String,
    pub branch_id: String,
    pub source_run_id: String,
    pub restored_tick: u64,
    pub restored_state_hash: u64,
    pub branch_ticks: u64,
    pub branch_final_tick: u64,
    pub branch_bodies_updated: u64,
    pub branch_final_state_hash: u64,
    pub branch_final_snapshot: SimSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessAssertionKind {
    FinalTickEquals,
    FinalHashEquals,
    BodiesUpdatedAtLeast,
    SpawnedBodiesEquals,
    VerdictEquals,
    BodyExists,
    BodyScalarInRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessAssertionMetric {
    Position,
    Velocity,
    Acceleration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessAssertionAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessAssertion {
    pub assertion_id: String,
    pub kind: HarnessAssertionKind,
    pub expected_u64: Option<u64>,
    pub expected_string: Option<String>,
    pub min_u64: Option<u64>,
    pub body_id: Option<u32>,
    pub metric: Option<HarnessAssertionMetric>,
    pub axis: Option<HarnessAssertionAxis>,
    pub min_milli: Option<i64>,
    pub max_milli: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessAssertionCheck {
    pub assertion_id: String,
    pub kind: HarnessAssertionKind,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
    pub reason: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub source_of_truth: String,
    #[serde(default)]
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessAssertionVerdict {
    pub verdict_id: String,
    pub run_id: String,
    pub check_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub verdict: String,
    pub checks: Vec<HarnessAssertionCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessScreenshotDiffRecord {
    pub diff_id: String,
    pub baseline_path: String,
    pub candidate_path: String,
    pub source_of_truth: String,
    pub baseline_sha256: String,
    pub candidate_sha256: String,
    pub baseline_bytes: u64,
    pub candidate_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub pixel_count: u64,
    pub changed_bytes: u64,
    pub changed_pixels: u64,
    pub max_changed_bytes: u64,
    pub mean_abs_error: f32,
    pub mean_luma_error: f32,
    pub rms_error: f32,
    pub max_channel_delta: u8,
    pub max_mean_abs_error: f32,
    pub render_reference_scene: bool,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCalyxPanelReadback {
    pub template_name: String,
    pub template_status: String,
    pub content_lens_count: usize,
    pub association_family_count: usize,
    pub lens_names: Vec<String>,
    pub source_ref: String,
    pub source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessAuthoredAbilityCandidate {
    pub ability_id: String,
    pub display_name: String,
    pub damage_type: String,
    pub damage: u32,
    pub cooldown_seconds: u32,
    pub mana_cost: u32,
    pub summary: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCalyxOracleGate {
    pub min_grounded_bits_milli: u32,
    pub min_honesty_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCalyxAuthoredContent {
    pub content_id: String,
    pub request: GroundedAbilityRequest,
    pub corpus: WorldCorpus,
    pub assay: AssayBalanceProfile,
    pub ward: WardPolicy,
    pub candidate: HarnessAuthoredAbilityCandidate,
    pub oracle: HarnessCalyxOracleGate,
    pub panel: HarnessCalyxPanelReadback,
    pub search_calibration: WorldMemorySearchCalibration,
    pub lodestar_question: String,
    pub sextant_query: String,
    pub similar_to_content_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessCalyxSignalSubsystem {
    Assay,
    Oracle,
    Ward,
    Lodestar,
    Sextant,
    Loom,
    RunState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessCalyxSignalStatus {
    Admitted,
    Refused,
    Insufficient,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCalyxSignalVerdict {
    pub content_id: String,
    pub subsystem: HarnessCalyxSignalSubsystem,
    pub status: HarnessCalyxSignalStatus,
    pub code: String,
    pub message: String,
    pub evidence: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessCalyxSignalsRecord {
    pub signal_batch_id: String,
    pub run_id: Option<String>,
    pub source_of_truth: String,
    pub panel: Option<HarnessCalyxPanelReadback>,
    pub request_count: usize,
    pub signal_count: usize,
    pub signals: Vec<HarnessCalyxSignalVerdict>,
    pub admitted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessPevRunRecord {
    pub pev_id: String,
    pub run_id: String,
    pub run_record: HarnessRunRecord,
    pub assertion_verdict: HarnessAssertionVerdict,
    pub oracle_verdict: HarnessOracleRunRecord,
    pub calyx_signals: HarnessCalyxSignalsRecord,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessPevTranscriptRecord {
    pub issue: u32,
    pub transcript_id: String,
    pub pev_id: String,
    pub intent: String,
    pub acceptance_criteria: Vec<String>,
    pub max_turns: u32,
    pub turn_count: usize,
    pub final_verdict: String,
    pub halt_reason: Option<String>,
    pub accepted_turn_id: Option<String>,
    pub reverted_to_turn_id: Option<String>,
    pub final_turn_hash: String,
    pub turns: Vec<HarnessPevTranscriptTurn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessPevTranscriptTurn {
    pub turn_index: u32,
    pub turn_id: String,
    pub author_delta: String,
    pub changed_paths: Vec<String>,
    pub rerun_scope: String,
    pub proposal_fingerprint: String,
    pub run_id: Option<String>,
    pub run_record: Option<HarnessRunRecord>,
    pub assertion_verdict: Option<HarnessAssertionVerdict>,
    pub oracle_verdict: Option<HarnessOracleRunRecord>,
    pub calyx_signals: Option<HarnessCalyxSignalsRecord>,
    pub passed: bool,
    pub no_op: bool,
    pub regression: bool,
    pub applied_to_state: bool,
    pub feedback_code: String,
    pub feedback_message: String,
    pub remediation: String,
    pub previous_turn_hash: String,
    pub turn_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessFeedbackRecord {
    pub feedback_id: String,
    pub stream_index: u64,
    pub source: String,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub remediation: String,
    pub actual: String,
    pub expected: String,
    pub location: String,
    pub source_of_truth: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessEventRecord {
    pub event_index: u64,
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub summary: String,
    pub affected_ids: Vec<String>,
    pub state_update_count: u64,
    #[serde(default)]
    pub feedback: Vec<HarnessFeedbackRecord>,
}
