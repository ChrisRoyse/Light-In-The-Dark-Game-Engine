use serde::{Deserialize, Serialize};

use crate::{math::FixedVec3, sim::SimBodyId, sim_order::SimOrder};

pub const AI_NPC_AGENT_ACTIVATION_SCHEMA: &str = "litd.ai.npc_agent_activation.v1";
pub const AI_NPC_AGENT_HOT_PATH_POLICY: &str = "lowered_orders_only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcAgentSignalKind {
    EnemyInRange,
    DialogueOpportunity,
    ScheduleTick,
    HelpRequest,
    WorldEvent,
}

impl NpcAgentSignalKind {
    pub const fn intent(self) -> NpcAgentIntent {
        match self {
            Self::EnemyInRange => NpcAgentIntent::RunController,
            Self::DialogueOpportunity => NpcAgentIntent::PromptDialogue,
            Self::ScheduleTick => NpcAgentIntent::PromptRoutine,
            Self::HelpRequest => NpcAgentIntent::PromptReaction,
            Self::WorldEvent => NpcAgentIntent::PromptEvent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcAgentIntent {
    RunController,
    PromptDialogue,
    PromptRoutine,
    PromptReaction,
    PromptEvent,
}

impl NpcAgentIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunController => "run_controller",
            Self::PromptDialogue => "prompt_dialogue",
            Self::PromptRoutine => "prompt_routine",
            Self::PromptReaction => "prompt_reaction",
            Self::PromptEvent => "prompt_event",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcAgentActivationGrade {
    Accepted,
    SignalTooWeak,
    LowRelevance,
    NoCandidate,
    WardBlocked,
    IntentNotAllowed,
    CooldownActive,
    BudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentRules {
    pub max_activations_per_tick: usize,
    pub min_strength_milli: u16,
    pub min_relevance_milli: u16,
    pub cooldown_ticks: u64,
}

impl NpcAgentRules {
    pub const fn living_world_defaults() -> Self {
        Self {
            max_activations_per_tick: 32,
            min_strength_milli: 500,
            min_relevance_milli: 500,
            cooldown_ticks: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentDefinition {
    pub npc_id: String,
    pub display_name: String,
    pub body_id: SimBodyId,
    pub default_agent_id: u32,
    pub prompt_profile_id: String,
    pub ward_tags: Vec<String>,
    pub activation_tags: Vec<String>,
    pub allowed_intents: Vec<NpcAgentIntent>,
    pub order_velocity: FixedVec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentRuntimeState {
    pub npc_id: String,
    pub spawned: bool,
    pub last_activation_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcWorldSignal {
    pub signal_id: String,
    pub world_id: String,
    pub tick: u64,
    pub kind: NpcAgentSignalKind,
    pub strength_milli: u16,
    pub relevance_milli: u16,
    pub tags: Vec<String>,
    pub summary: String,
    pub target_npc_id: Option<String>,
    pub target_body: Option<SimBodyId>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentActivationInput {
    pub runtime_id: String,
    pub catalog_id: String,
    pub rules: NpcAgentRules,
    pub npcs: Vec<NpcAgentDefinition>,
    pub states: Vec<NpcAgentRuntimeState>,
    pub signals: Vec<NpcWorldSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentActivationDecision {
    pub activation_id: String,
    pub signal_id: String,
    pub npc_id: Option<String>,
    pub default_agent_id: Option<u32>,
    pub prompt_profile_id: Option<String>,
    pub intent: NpcAgentIntent,
    pub accepted: bool,
    pub grade: NpcAgentActivationGrade,
    pub reason: String,
    pub relevance_score: u16,
    pub spawn_requested: bool,
    pub prompt_request_id: Option<String>,
    pub prompt: Option<String>,
    pub execute_tick: Option<u64>,
    pub lowered_order: Option<SimOrder>,
    pub order_fingerprint: Option<u64>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcAgentActivationReadback {
    pub schema: String,
    pub runtime_id: String,
    pub catalog_id: String,
    pub npc_count: usize,
    pub default_agent_count: usize,
    pub signal_count: usize,
    pub decision_count: usize,
    pub accepted_count: usize,
    pub skipped_count: usize,
    pub prompt_count: usize,
    pub order_count: usize,
    pub spawn_request_count: usize,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub lowering_boundary: String,
    pub decisions: Vec<NpcAgentActivationDecision>,
    pub orders: Vec<SimOrder>,
    pub fnv1a64: String,
}
