use serde::{Deserialize, Serialize};

use super::{MortalityAgingReadback, MortalityCharacterState, MortalityEventReadback};

pub const RPG_UNDEATH_SCHEMA: &str = "litd.rpg.undeath.v1";
pub const ASHEN_REACH_UNDEATH_POLICY_ID: &str = "server.ashen_reach.undeath.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndeadForm {
    Lich,
    Mummy,
    Wraith,
}

impl UndeadForm {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lich => "lich",
            Self::Mummy => "mummy",
            Self::Wraith => "wraith",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeathPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub minimum_caster_level: u16,
    pub fixed_constitution_after_withering: i16,
    pub aging_death_immune: bool,
    pub hunger_thirst_immune: bool,
    pub physical_kill_only: bool,
    pub irreversible: bool,
    pub normal_resurrection_forbidden: bool,
    pub allowed_dark_caster_class_ids: Vec<String>,
    pub required_discovery_ref_count: usize,
    pub required_components: Vec<String>,
    pub forms: Vec<UndeadFormSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeadFormSpec {
    pub form: UndeadForm,
    pub required_focus_id: String,
    pub ritual: UndeadRitualAbility,
}

impl UndeadFormSpec {
    pub fn new(
        form: UndeadForm,
        required_focus_id: &str,
        ability_id: &str,
        display_name: &str,
        description: &str,
    ) -> Self {
        Self {
            form,
            required_focus_id: required_focus_id.to_string(),
            ritual: UndeadRitualAbility {
                ability_id: ability_id.to_string(),
                display_name: display_name.to_string(),
                description: description.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeadRitualAbility {
    pub ability_id: String,
    pub display_name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringRequest {
    pub ritual_id: String,
    pub chosen_form: UndeadForm,
    pub caster_class_id: String,
    pub caster_level: u16,
    pub discovery_refs: Vec<String>,
    pub ritual_components: Vec<String>,
    pub prepared_focus_id: String,
    pub accepted_irreversible_cost: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeadCharacterState {
    pub schema: String,
    pub character_id: String,
    pub character_name: String,
    pub original_race_id: String,
    pub form: UndeadForm,
    pub transformed_at_age_years: u16,
    pub mortality_state: MortalityCharacterState,
    pub focus_id: String,
    pub unlocked_rituals: Vec<UndeadRitualAbility>,
    pub aging_death_immune: bool,
    pub hunger_thirst_immune: bool,
    pub physical_kill_only: bool,
    pub irreversible: bool,
    pub normal_resurrection_forbidden: bool,
    pub source_issue: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeadAgingReadback {
    pub mortal_aging: MortalityAgingReadback,
    pub aging_death_immune: bool,
    pub old_age_death_suppressed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringReadback {
    pub schema: String,
    pub ritual_id: String,
    pub character_id: String,
    pub chosen_form: UndeadForm,
    pub caster_class_id: String,
    pub caster_level: u16,
    pub constitution_before: i16,
    pub constitution_after: i16,
    pub discovery_ref_count: usize,
    pub component_count: usize,
    pub focus_id: String,
    pub unlocked_rituals: Vec<UndeadRitualAbility>,
    pub undead_state: UndeadCharacterState,
    pub before_mortality_sha256: String,
    pub undead_state_sha256: String,
    pub policy_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndeadMortalityReadback {
    pub schema: String,
    pub undead_character_id: String,
    pub form: UndeadForm,
    pub aging: UndeadAgingReadback,
    pub physical_kill_only: bool,
    pub mortality: MortalityEventReadback,
    pub undead_state_after: UndeadCharacterState,
    pub undead_before_sha256: String,
    pub undead_after_sha256: String,
    pub result_sha256: String,
}
