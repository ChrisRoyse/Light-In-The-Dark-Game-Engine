use serde::{Deserialize, Serialize};

use super::{MortalityCharacterState, MortalityEventReadback, MourningWallEntry};

pub const RPG_INFERNAL_ATTRITION_SCHEMA: &str = "litd.rpg.infernal_attrition.v1";
pub const ASHEN_REACH_INFERNAL_ATTRITION_POLICY_ID: &str =
    "server.ashen_reach.infernal_attrition.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalCircleId {
    Threshold,
    EmberWastes,
    IronBastion,
    HungeringDark,
    WailingTombs,
    MoltenThroneRoad,
    ObsidianThrone,
}

impl InfernalCircleId {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Threshold => "threshold",
            Self::EmberWastes => "ember_wastes",
            Self::IronBastion => "iron_bastion",
            Self::HungeringDark => "hungering_dark",
            Self::WailingTombs => "wailing_tombs",
            Self::MoltenThroneRoad => "molten_throne_road",
            Self::ObsidianThrone => "obsidian_throne",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalHazardKind {
    Heat,
    DarkPressure,
    SoulDrain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAttritionPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub constitution_strain_per_loss: i32,
    pub max_step_ticks: u16,
    pub max_mitigation_percent: u8,
    pub circles: Vec<InfernalCircleProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalCircleProfile {
    pub circle_id: InfernalCircleId,
    pub hazard: InfernalHazardKind,
    pub health_damage_per_tick: i32,
    pub constitution_strain_per_tick: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAttritionState {
    pub schema: String,
    pub run_id: String,
    pub party_member_id: String,
    pub circle_id: InfernalCircleId,
    pub hit_points: i32,
    pub max_hit_points: i32,
    pub constitution_strain: i32,
    pub total_exposure_ticks: u32,
    pub mortality_state: MortalityCharacterState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAttritionStep {
    pub step_id: String,
    pub circle_id: InfernalCircleId,
    pub tick_count: u16,
    pub mitigation_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAttritionDeathReadback {
    pub permanent: bool,
    pub reason: String,
    pub mortality_event: Option<MortalityEventReadback>,
    pub mourning_wall_entry: Option<MourningWallEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAttritionReadback {
    pub schema: String,
    pub step_id: String,
    pub run_id: String,
    pub party_member_id: String,
    pub circle_id: InfernalCircleId,
    pub hazard: InfernalHazardKind,
    pub tick_count: u16,
    pub mitigation_percent: u8,
    pub health_before: i32,
    pub health_after: i32,
    pub constitution_before: i16,
    pub constitution_after: i16,
    pub strain_before: i32,
    pub strain_after: i32,
    pub strain_gained: i32,
    pub constitution_lost_to_strain: i16,
    pub death_threshold_hit_points: i32,
    pub death: Option<InfernalAttritionDeathReadback>,
    pub state_after: InfernalAttritionState,
    pub state_before_sha256: String,
    pub state_after_sha256: String,
    pub policy_sha256: String,
    pub result_sha256: String,
}
