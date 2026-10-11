use serde::{Deserialize, Serialize};

use super::AttributeScores;

pub const RPG_MORTALITY_SCHEMA: &str = "litd.rpg.mortality.v1";
pub const ASHEN_REACH_MORTALITY_POLICY_ID: &str = "server.ashen_reach.mortality.default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub deaths_per_constitution_loss: u16,
    pub permanent_death_constitution_threshold: i16,
    pub constitution_hit_points: i32,
    pub constitution_regen_per_tick: i32,
    pub death_threshold_base: i32,
    pub death_threshold_per_constitution: i32,
    pub mourning_wall_required: bool,
    pub race_lifespans: Vec<RaceLifespan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceLifespan {
    pub race_id: String,
    pub display_name: String,
    pub adulthood_years: u16,
    pub expected_lifespan_years: u16,
    pub maximum_lifespan_years: u16,
}

impl RaceLifespan {
    pub fn new(
        race_id: &str,
        display_name: &str,
        adulthood_years: u16,
        expected_lifespan_years: u16,
        maximum_lifespan_years: u16,
    ) -> Self {
        Self {
            race_id: race_id.to_string(),
            display_name: display_name.to_string(),
            adulthood_years,
            expected_lifespan_years,
            maximum_lifespan_years,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityCharacterState {
    pub schema: String,
    pub character_id: String,
    pub character_name: String,
    pub race_id: String,
    pub level: u16,
    pub age_years: u16,
    pub base_attributes: AttributeScores,
    pub current_constitution: i16,
    pub deaths_since_constitution_loss: u16,
    pub total_deaths: u32,
    pub permanently_dead: bool,
    pub mourning_wall_entry_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MortalityEventKind {
    CombatDeath,
    ExposureDeath,
    HollowAttrition,
    OldAge,
    RitualBacklash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityEvent {
    pub event_id: String,
    pub kind: MortalityEventKind,
    pub witness_id: Option<String>,
    pub epitaph: String,
}

impl MortalityEvent {
    pub fn new(event_id: &str, kind: MortalityEventKind, epitaph: &str) -> Self {
        Self {
            event_id: event_id.to_string(),
            kind,
            witness_id: None,
            epitaph: epitaph.to_string(),
        }
    }

    pub fn witnessed_by(mut self, witness_id: &str) -> Self {
        self.witness_id = Some(witness_id.to_string());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MortalityAgeBand {
    Youth,
    Adult,
    Elder,
    OldAgeDue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityDerivedStats {
    pub constitution: i16,
    pub constitution_hit_points: i32,
    pub regen_per_tick: i32,
    pub death_threshold_hit_points: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityAgingReadback {
    pub race_id: String,
    pub age_years: u16,
    pub adulthood_years: u16,
    pub expected_lifespan_years: u16,
    pub maximum_lifespan_years: u16,
    pub age_band: MortalityAgeBand,
    pub old_age_death_due: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MourningWallEntry {
    pub entry_id: String,
    pub carved_name: String,
    pub character_id: String,
    pub race_id: String,
    pub age_years: u16,
    pub final_constitution: i16,
    pub total_deaths: u32,
    pub cause: MortalityEventKind,
    pub witness_id: Option<String>,
    pub epitaph: String,
    pub source_issue: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MortalityEventReadback {
    pub schema: String,
    pub event_id: String,
    pub character_id: String,
    pub race_id: String,
    pub event_kind: MortalityEventKind,
    pub constitution_before: i16,
    pub constitution_after: i16,
    pub deaths_since_loss_before: u16,
    pub deaths_since_loss_after: u16,
    pub total_deaths_before: u32,
    pub total_deaths_after: u32,
    pub constitution_lost_this_event: bool,
    pub permanently_dead: bool,
    pub permanent_death_reason: Option<String>,
    pub aging: MortalityAgingReadback,
    pub derived_before: MortalityDerivedStats,
    pub derived_after: MortalityDerivedStats,
    pub mourning_wall_entry: Option<MourningWallEntry>,
    pub before_state_sha256: String,
    pub after_state_sha256: String,
    pub policy_sha256: String,
    pub result_sha256: String,
    pub after_state: MortalityCharacterState,
}
