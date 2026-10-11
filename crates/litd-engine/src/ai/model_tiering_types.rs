use serde::{Deserialize, Serialize};

pub const AI_AGENT_MODEL_REGISTRY_SCHEMA: &str = "litd.ai.agent_model_registry.v1";
pub const AI_AGENT_MODEL_SELECTION_SCHEMA: &str = "litd.ai.agent_model_selection.v1";
pub const AI_AGENT_MODEL_HOT_PATH_POLICY: &str = "out_of_band_selection_lowered_orders_only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentModelRole {
    ObjectVisual,
    CreatureCombat,
    NpcDialogue,
    ImmortalDirector,
}

impl AgentModelRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObjectVisual => "object_visual",
            Self::CreatureCombat => "creature_combat",
            Self::NpcDialogue => "npc_dialogue",
            Self::ImmortalDirector => "immortal_director",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentModelBackend {
    LlamaCppGguf,
    TensorRtLlm,
    Vllm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentModelTier {
    TinyObject,
    SmallCreature,
    NpcDialogue,
    Director,
}

impl AgentModelTier {
    pub const fn rank(self) -> u8 {
        match self {
            Self::TinyObject => 1,
            Self::SmallCreature => 2,
            Self::NpcDialogue => 3,
            Self::Director => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelHardwareProfile {
    pub hardware_id: String,
    pub gpu_name: String,
    pub vram_budget_mib: u32,
    pub runtime_reserved_mib: u32,
    pub warm_pool_budget_mib: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelSpec {
    pub model_id: String,
    pub tier: AgentModelTier,
    pub role: AgentModelRole,
    pub backend: AgentModelBackend,
    pub quantization: String,
    pub parameter_class: String,
    pub artifact_ref: String,
    pub weight_mib: u32,
    pub kv_cache_mib: u32,
    pub max_context_tokens: u32,
    pub max_concurrent_requests: u16,
    pub warm_pool_slots: u16,
    pub warm_start_millis: u32,
    pub cold_start_millis: u32,
    pub deterministic_lowering_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelRegistry {
    pub schema: String,
    pub registry_id: String,
    pub hardware: AgentModelHardwareProfile,
    pub models: Vec<AgentModelSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelSelectionRequest {
    pub run_id: String,
    pub agent_id: String,
    pub role: AgentModelRole,
    pub required_context_tokens: u32,
    pub concurrent_requests: u16,
    pub existing_warm_pool_mib: u32,
    pub prefer_warm_pool: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelCandidateRejection {
    pub model_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModelSelectionReadback {
    pub schema: String,
    pub run_id: String,
    pub agent_id: String,
    pub role: AgentModelRole,
    pub selected_model_id: String,
    pub selected_tier: AgentModelTier,
    pub backend: AgentModelBackend,
    pub quantization: String,
    pub reserved_weight_mib: u32,
    pub reserved_kv_cache_mib: u32,
    pub reserved_total_mib: u32,
    pub warm_pool_slots_reserved: u16,
    pub existing_warm_pool_mib: u32,
    pub warm_pool_after_mib: u32,
    pub vram_budget_mib: u32,
    pub runtime_reserved_mib: u32,
    pub vram_headroom_mib: u32,
    pub candidate_count: usize,
    pub rejected_candidates: Vec<AgentModelCandidateRejection>,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub selection_reason: String,
    pub selection_fingerprint: String,
}
