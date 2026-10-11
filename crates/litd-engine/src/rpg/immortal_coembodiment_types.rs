use serde::{Deserialize, Serialize};

use super::ImmortalDecisionReadback;

pub const RPG_IMMORTAL_COEMBODIMENT_SCHEMA: &str = "litd.rpg.immortal_coembodiment.v1";
pub const ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID: &str =
    "server.ashen_reach.immortal_coembodiment.default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalCoembodimentPolicy {
    pub schema: String,
    pub policy_id: String,
    pub source_issue: u32,
    pub ai_driver_required: bool,
    pub human_driver_allowed: bool,
    pub human_override_authoritative: bool,
    pub player_visible_single_identity: bool,
    pub deterministic_lowering_required: bool,
    pub ai_hot_path_mutation_allowed: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmortalEmbodimentDriver {
    CalyxAi,
    HumanAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalEmbodiedDecision {
    pub sequence: u16,
    pub driver: ImmortalEmbodimentDriver,
    pub human_admin_id: Option<String>,
    pub override_ai_routine: bool,
    pub decision: ImmortalDecisionReadback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalCoembodimentSession {
    pub session_id: String,
    pub immortal_id: String,
    pub public_identity: String,
    pub embodied_decisions: Vec<ImmortalEmbodiedDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalCoembodimentReadback {
    pub schema: String,
    pub policy_id: String,
    pub source_issue: u32,
    pub session_id: String,
    pub immortal_id: String,
    pub display_name: String,
    pub religion_id: String,
    pub public_identity: String,
    pub player_visible_single_identity: bool,
    pub ai_driver_present: bool,
    pub human_driver_present: bool,
    pub human_override_authoritative: bool,
    pub handoff_count: usize,
    pub human_override_count: usize,
    pub accepted_decision_count: usize,
    pub lowered_effect_count: usize,
    pub deterministic_lowering_required: bool,
    pub ai_hot_path_mutation_allowed: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub decision_ledger_sha256s: Vec<String>,
    pub policy_sha256: String,
    pub session_ledger_sha256: String,
    pub result_sha256: String,
}
