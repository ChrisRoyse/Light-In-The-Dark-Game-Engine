use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBinding {
    pub binding_id: String,
    pub agent_id: String,
    pub object_id: String,
    pub display_name: String,
    pub deterministic_order_sink: String,
    pub activation: AgentActivationPolicy,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivationPolicy {
    pub mode: String,
    pub signal_refs: Vec<String>,
    pub cooldown_ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPromptModel {
    pub agent_id: String,
    pub prompt: String,
    pub model_id: String,
    pub personality: String,
    pub memory_scope_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentWardEnvelope {
    pub agent_id: String,
    pub physics: WardPhysicsEnvelope,
    pub visual: WardVisualEnvelope,
    pub behavior: WardBehaviorEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardPhysicsEnvelope {
    pub max_move_units_per_tick: u32,
    pub allowed_region_ids: Vec<String>,
    pub allow_teleport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardVisualEnvelope {
    pub allowed_effect_ids: Vec<String>,
    pub max_scale_milli: u32,
    pub max_emissive_milli: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardBehaviorEnvelope {
    pub allowed_intents: Vec<String>,
    pub forbidden_intents: Vec<String>,
    pub grounding_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAuthoringVisualBinding {
    pub character_ref: String,
    pub visual_ref: String,
    pub particle_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorAgentArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub agent_id: String,
    pub object_id: String,
    pub display_name: String,
    pub prompt: String,
    pub model_tier: String,
    pub behavior_preset: String,
    pub personality: String,
    pub region_id: String,
    pub activation: AgentActivationPolicy,
    pub visual: AgentAuthoringVisualBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindAgentArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub binding: AgentBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetPromptModelArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub prompt_model: AgentPromptModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetWardEnvelopeArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub envelope: AgentWardEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadAgentsArgs {
    pub state_path: PathBuf,
}
