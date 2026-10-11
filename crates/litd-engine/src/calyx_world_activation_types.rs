use serde::{Deserialize, Serialize};

use crate::{
    calyx_persistence::{EngineCalyxDatabaseReadback, EngineCalyxPersistenceError},
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::SimOrder,
};

pub const ENGINE_CALYX_WORLD_ACTIVATION_SCHEMA: &str = "litd.engine.calyx_world_activation.v1";
pub const ENGINE_CALYX_WORLD_ACTIVATION_READBACK_SCHEMA: &str =
    "litd.engine.calyx_world_activation_readback.v1";
pub const ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY: &str = "lowered_orders_only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWorldFactKind {
    LogLine,
    Event,
    Measure,
    Bit,
}

impl EngineWorldFactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LogLine => "log_line",
            Self::Event => "event",
            Self::Measure => "measure",
            Self::Bit => "bit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWorldActivationAgentKind {
    Npc,
    Object,
    FactionLeader,
    Immortal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWorldActivationIntent {
    RunController,
    PromptDialogue,
    PromptRoutine,
    PromptReaction,
    PromptEvent,
    SpawnAgent,
}

impl EngineWorldActivationIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunController => "run_controller",
            Self::PromptDialogue => "prompt_dialogue",
            Self::PromptRoutine => "prompt_routine",
            Self::PromptReaction => "prompt_reaction",
            Self::PromptEvent => "prompt_event",
            Self::SpawnAgent => "spawn_agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWorldActivationGrade {
    Accepted,
    NoAssociation,
    InsufficientSupport,
    WardBlocked,
    CooldownActive,
    BudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldActivationPolicy {
    pub policy_id: String,
    pub max_activations_per_tick: usize,
    pub min_support_milli: u16,
    pub cooldown_ticks: u64,
}

impl EngineWorldActivationPolicy {
    pub fn living_world_defaults(policy_id: impl Into<String>) -> Self {
        Self {
            policy_id: policy_id.into(),
            max_activations_per_tick: 32,
            min_support_milli: 500,
            cooldown_ticks: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldAgentProfile {
    pub agent_id: String,
    pub display_name: String,
    pub kind: EngineWorldActivationAgentKind,
    pub prompt_profile_id: String,
    pub lowered_agent_id: u32,
    pub target_body: SimBodyId,
    pub trigger_tags: Vec<String>,
    pub ward_tags: Vec<String>,
    pub intent: EngineWorldActivationIntent,
    pub order_velocity: FixedVec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldAgentState {
    pub agent_id: String,
    pub spawned: bool,
    pub last_activation_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldFact {
    pub fact_id: String,
    pub world_id: String,
    pub tick: u64,
    pub kind: EngineWorldFactKind,
    pub tags: Vec<String>,
    pub summary: String,
    pub weight_milli: u16,
    pub measure_milli: Option<u16>,
    pub target_agent_id: Option<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldActivationInput {
    pub run_id: String,
    pub world_id: String,
    pub policy: EngineWorldActivationPolicy,
    pub agents: Vec<EngineWorldAgentProfile>,
    pub states: Vec<EngineWorldAgentState>,
    pub facts: Vec<EngineWorldFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldAssociation {
    pub association_id: String,
    pub agent_id: String,
    pub matched_fact_ids: Vec<String>,
    pub matched_tags: Vec<String>,
    pub all_fact_tags: Vec<String>,
    pub support_milli: u16,
    pub max_tick: u64,
    pub evidence_summary: String,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldActivationDecision {
    pub decision_id: String,
    pub agent_id: String,
    pub prompt_profile_id: String,
    pub intent: EngineWorldActivationIntent,
    pub accepted: bool,
    pub grade: EngineWorldActivationGrade,
    pub reason: String,
    pub support_milli: u16,
    pub spawn_requested: bool,
    pub association: Option<EngineWorldAssociation>,
    pub prompt_request_id: Option<String>,
    pub prompt: Option<String>,
    pub execute_tick: Option<u64>,
    pub lowered_order: Option<SimOrder>,
    pub order_fingerprint: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldActivationReadback {
    pub schema: String,
    pub run_id: String,
    pub world_id: String,
    pub policy: EngineWorldActivationPolicy,
    pub fact_count: usize,
    pub fact_kind_count: usize,
    pub agent_count: usize,
    pub association_count: usize,
    pub decision_count: usize,
    pub accepted_count: usize,
    pub skipped_count: usize,
    pub prompt_count: usize,
    pub order_count: usize,
    pub spawn_request_count: usize,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub lowering_boundary: String,
    pub associations: Vec<EngineWorldAssociation>,
    pub decisions: Vec<EngineWorldActivationDecision>,
    pub orders: Vec<SimOrder>,
    pub activation_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineWorldActivationStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub activation: EngineWorldActivationReadback,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineWorldActivationError {
    #[error("engine Calyx world activation field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx world activation persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx world activation codec failed: {0}")]
    Codec(String),
    #[error(
        "engine Calyx world activation readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },
}

impl EngineWorldActivationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "LITD-CALYX-WORLD-ACTIVATION-INVALID-INPUT",
            Self::Persistence(_) => "LITD-CALYX-WORLD-ACTIVATION-PERSISTENCE",
            Self::Codec(_) => "LITD-CALYX-WORLD-ACTIVATION-CODEC",
            Self::ReadbackMismatch { .. } => "LITD-CALYX-WORLD-ACTIVATION-READBACK-MISMATCH",
        }
    }
}
