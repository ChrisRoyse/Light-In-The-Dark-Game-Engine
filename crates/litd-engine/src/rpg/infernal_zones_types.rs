use serde::{Deserialize, Serialize};

use super::{InfernalCircleId, InfernalHazardKind};

pub const RPG_INFERNAL_ZONES_SCHEMA: &str = "litd.rpg.infernal_zones.v1";
pub const ASHEN_REACH_INFERNAL_ZONES_POLICY_ID: &str = "server.ashen_reach.infernal_zones.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalZoneExitKind {
    Descend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalZonePolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub entry_circle: InfernalCircleId,
    pub terminal_circle: InfernalCircleId,
    pub zones: Vec<InfernalZoneNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalZoneNode {
    pub circle_id: InfernalCircleId,
    pub zone_id: String,
    pub display_name: String,
    pub depth: u8,
    pub hazard_profile_id: String,
    pub hazard_kind: InfernalHazardKind,
    pub mob_population_ids: Vec<String>,
    pub ruler_ref: String,
    pub reset_profile: InfernalResetProfile,
    pub repop_profile: InfernalRepopProfile,
    pub exits: Vec<InfernalZoneExit>,
    pub withering_reagent_source: bool,
    pub relic_vault_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalResetProfile {
    pub reset_id: String,
    pub interval_minutes: u16,
    pub clears_unlooted_corpses: bool,
    pub resets_hazard_emitters: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRepopProfile {
    pub repop_id: String,
    pub baseline_mob_budget: u16,
    pub repop_interval_minutes: u16,
    pub max_pressure_tier: u8,
    pub ruler_respawns: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalZoneExit {
    pub exit_id: String,
    pub kind: InfernalZoneExitKind,
    pub to_circle_id: InfernalCircleId,
    pub locked_by_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalZoneTopologyReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub zone_count: usize,
    pub built_in_map_required: bool,
    pub entry_circle: InfernalCircleId,
    pub terminal_circle: InfernalCircleId,
    pub chain: Vec<InfernalZoneChainStep>,
    pub wailing_tombs_reagent_source: bool,
    pub throne_road_relic_vault_source: bool,
    pub policy_sha256: String,
    pub attrition_policy_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalZoneChainStep {
    pub depth: u8,
    pub circle_id: InfernalCircleId,
    pub zone_id: String,
    pub display_name: String,
    pub hazard_profile_id: String,
    pub hazard_kind: InfernalHazardKind,
    pub attrition_health_damage_per_tick: i32,
    pub attrition_constitution_strain_per_tick: i32,
    pub mob_population_count: usize,
    pub ruler_ref: String,
    pub reset_id: String,
    pub repop_id: String,
    pub descends_to: Option<InfernalCircleId>,
}
