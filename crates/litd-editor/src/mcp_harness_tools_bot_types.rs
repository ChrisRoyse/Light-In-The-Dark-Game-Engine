use litd_engine::{sim_order::SimOrder, sim_replay::SimReplayRecord};
use serde::{Deserialize, Serialize};

use super::HarnessMutationContext;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriveBotHarnessArgs {
    #[serde(flatten)]
    pub context: HarnessMutationContext,
    pub bot_run_id: String,
    pub run_id: String,
    pub bot_id: String,
    pub driver_kind: HarnessBotDriverKind,
    pub objective: HarnessBotObjective,
    pub expected_outcome: HarnessBotOutcomeStatus,
    pub target_body_id: Option<u32>,
    pub ticks: u64,
    pub sample_interval_ticks: u64,
    pub velocity_x_milli: i64,
    pub velocity_z_milli: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessBotDriverKind {
    Heuristic,
    BehaviorTree,
    FiniteStateMachine,
    CalyxAgentLowered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessBotOutcomeStatus {
    Completed,
    Unwinnable,
    Timeout,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessBotObjective {
    ReachPositionXAtLeast { min_x_milli: i64 },
    SurviveTicks { min_ticks: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessBotDriveRecord {
    pub bot_run_id: String,
    pub run_id: String,
    pub bot_id: String,
    pub driver_kind: HarnessBotDriverKind,
    pub objective: HarnessBotObjective,
    pub expected_outcome: HarnessBotOutcomeStatus,
    pub status: HarnessBotOutcomeStatus,
    pub verdict: String,
    pub termination_reason: String,
    pub source_of_truth: String,
    pub target_body_id: u32,
    pub tick_budget: u64,
    pub sample_interval_ticks: u64,
    pub initial_state_hash: u64,
    pub final_state_hash: u64,
    pub replay_final_state_hash: u64,
    pub replay_verified: bool,
    pub final_tick: u64,
    pub ticks_elapsed: u64,
    pub bodies_updated: u64,
    pub order_count: u64,
    pub orders: Vec<SimOrder>,
    pub replay_record: SimReplayRecord,
    pub metrics: HarnessBotMetrics,
    pub trajectory_sample_count: usize,
    pub trajectory_hash: String,
    pub trajectory: Vec<HarnessBotTrajectorySample>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessBotMetrics {
    pub start_x_milli: i64,
    pub final_x_milli: i64,
    pub displacement_x_milli: i64,
    pub displacement_z_milli: i64,
    pub objective_progress_milli: i64,
    pub max_objective_progress_milli: i64,
    pub distance_xz_milli: i64,
    pub body_updates: u64,
    pub orders_applied: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessBotTrajectorySample {
    pub sample_index: usize,
    pub tick: u64,
    pub body_id: u32,
    pub position_x_milli: i64,
    pub position_z_milli: i64,
    pub velocity_x_milli: i64,
    pub velocity_z_milli: i64,
    pub state_hash: u64,
    pub orders_applied_total: u64,
    pub objective_progress_milli: i64,
}
