use serde::{Deserialize, Serialize};

use crate::sim::SimBodyId;

pub const AI_OBJECT_MICRO_AGENT_SCHEMA: &str = "litd.ai.object_micro_agent.v1";
pub const AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY: &str =
    "out_of_band_calyx_signal_lowered_visual_effects_only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectMicroAgentVisualIntent {
    Glow,
    Flicker,
    ParticlePulse,
    AwakenObject,
}

impl ObjectMicroAgentVisualIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Glow => "glow",
            Self::Flicker => "flicker",
            Self::ParticlePulse => "particle_pulse",
            Self::AwakenObject => "awaken_object",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectMicroAgentActivationGrade {
    Accepted,
    SignalTooWeak,
    NoCandidate,
    WardBlocked,
    CooldownActive,
    TickBudgetExceeded,
    ParticleBudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentBudget {
    pub max_agents: usize,
    pub max_activations_per_tick: usize,
    pub min_embedded_model_bytes: u64,
    pub max_embedded_model_bytes: u64,
    pub max_total_embedded_model_bytes: u64,
    pub max_particles_per_tick: u32,
    pub min_signal_strength_milli: u16,
    pub cooldown_ticks: u64,
}

impl ObjectMicroAgentBudget {
    pub const fn hundreds_spawnable_defaults() -> Self {
        Self {
            max_agents: 512,
            max_activations_per_tick: 256,
            min_embedded_model_bytes: 5 * 1024 * 1024,
            max_embedded_model_bytes: 20 * 1024 * 1024,
            max_total_embedded_model_bytes: 2 * 1024 * 1024 * 1024,
            max_particles_per_tick: 4096,
            min_signal_strength_milli: 500,
            cooldown_ticks: 6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentDefinition {
    pub agent_id: String,
    pub object_id: String,
    pub display_name: String,
    pub embedded_model_id: String,
    pub embedded_model_bytes: u64,
    pub embedded_model_sha256: String,
    pub target_body: SimBodyId,
    pub trigger_tags: Vec<String>,
    pub ward_tags: Vec<String>,
    pub visual_intent: ObjectMicroAgentVisualIntent,
    pub vfx_effect_id: String,
    pub material_variant: String,
    pub particle_budget: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentState {
    pub agent_id: String,
    pub spawned_instances: u16,
    pub last_activation_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxObjectVisualSignal {
    pub signal_id: String,
    pub world_id: String,
    pub tick: u64,
    pub strength_milli: u16,
    pub tags: Vec<String>,
    pub summary: String,
    pub target_object_id: Option<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentInput {
    pub run_id: String,
    pub world_id: String,
    pub budget: ObjectMicroAgentBudget,
    pub definitions: Vec<ObjectMicroAgentDefinition>,
    pub states: Vec<ObjectMicroAgentState>,
    pub signals: Vec<CalyxObjectVisualSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentLoweredEffect {
    pub effect_id: String,
    pub agent_id: String,
    pub object_id: String,
    pub target_body: SimBodyId,
    pub visual_intent: ObjectMicroAgentVisualIntent,
    pub vfx_effect_id: String,
    pub material_variant: String,
    pub intensity_milli: u16,
    pub particle_budget: u32,
    pub provenance_refs: Vec<String>,
    pub source: String,
    pub deterministic: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentDecision {
    pub decision_id: String,
    pub signal_id: String,
    pub agent_id: Option<String>,
    pub object_id: Option<String>,
    pub accepted: bool,
    pub grade: ObjectMicroAgentActivationGrade,
    pub reason: String,
    pub support_milli: u16,
    pub lowered_effect: Option<ObjectMicroAgentLoweredEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMicroAgentReadback {
    pub schema: String,
    pub run_id: String,
    pub world_id: String,
    pub agent_count: usize,
    pub signal_count: usize,
    pub decision_count: usize,
    pub accepted_count: usize,
    pub skipped_count: usize,
    pub embedded_model_count: usize,
    pub total_embedded_model_bytes: u64,
    pub visual_effect_count: usize,
    pub total_particles_reserved: u32,
    pub spawnable_object_count: usize,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub lowering_boundary: String,
    pub decisions: Vec<ObjectMicroAgentDecision>,
    pub lowered_effects: Vec<ObjectMicroAgentLoweredEffect>,
    pub readback_fingerprint: String,
}
