use serde::{Deserialize, Serialize};

use super::{AbilityCatalog, AttributeScores, RaceAttributeCaps};

pub const RPG_ADVANCEMENT_SCHEMA: &str = "litd.rpg.advancement.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementAttribute {
    Strength,
    Intelligence,
    Wisdom,
    Dexterity,
    Constitution,
    Charisma,
}

impl AdvancementAttribute {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Intelligence => "intelligence",
            Self::Wisdom => "wisdom",
            Self::Dexterity => "dexterity",
            Self::Constitution => "constitution",
            Self::Charisma => "charisma",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementVital {
    HitPoints,
    Mana,
    Movement,
}

impl AdvancementVital {
    pub const fn label(self) -> &'static str {
        match self {
            Self::HitPoints => "hit_points",
            Self::Mana => "mana",
            Self::Movement => "movement",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementRules {
    pub schema: String,
    pub max_level: u16,
    pub practice_bands: Vec<PracticeAwardBand>,
    pub train_sessions_per_level: u16,
    pub train_stat_increment: i16,
    pub train_hit_points: i32,
    pub train_mana: i32,
    pub train_movement: i32,
    pub practice_gain_base: u16,
    pub practice_intelligence_divisor: u16,
    pub practice_cap: u16,
    pub use_gain_per_success: u16,
    pub focused_skill_learn_bonus: u16,
    pub level_cap_base: u16,
    pub level_cap_per_level: u16,
    pub study_grants: Vec<StudyGrantRule>,
    pub study_costs: Vec<StudyAbilityCost>,
}

impl AdvancementRules {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_ADVANCEMENT_SCHEMA.to_string(),
            max_level: 51,
            practice_bands: vec![
                PracticeAwardBand::new(1, 1),
                PracticeAwardBand::new(10, 2),
                PracticeAwardBand::new(15, 3),
                PracticeAwardBand::new(18, 4),
                PracticeAwardBand::new(21, 5),
            ],
            train_sessions_per_level: 1,
            train_stat_increment: 1,
            train_hit_points: 10,
            train_mana: 12,
            train_movement: 15,
            practice_gain_base: 18,
            practice_intelligence_divisor: 2,
            practice_cap: 75,
            use_gain_per_success: 3,
            focused_skill_learn_bonus: 2,
            level_cap_base: 30,
            level_cap_per_level: 3,
            study_grants: vec![StudyGrantRule {
                class_id: "class.thief".to_string(),
                max_level: 41,
                points_per_level: 2,
                milestone_level: 42,
                milestone_points: 18,
            }],
            study_costs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PracticeAwardBand {
    pub min_wisdom: i16,
    pub practices_per_level: u16,
}

impl PracticeAwardBand {
    pub const fn new(min_wisdom: i16, practices_per_level: u16) -> Self {
        Self {
            min_wisdom,
            practices_per_level,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyGrantRule {
    pub class_id: String,
    pub max_level: u16,
    pub points_per_level: u16,
    pub milestone_level: u16,
    pub milestone_points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyAbilityCost {
    pub ability_id: String,
    pub points: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementInput {
    pub character_id: String,
    pub class_id: String,
    pub race_id: String,
    pub from_level: u16,
    pub to_level: u16,
    pub attributes: AttributeScores,
    pub race_caps: Vec<RaceAttributeCaps>,
    pub ability_catalog: AbilityCatalog,
    pub starting: AdvancementState,
    pub focused_ability_id: Option<String>,
    pub actions: Vec<AdvancementAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementState {
    pub level: u16,
    pub attributes: AttributeScores,
    pub pools: AdvancementPools,
    pub vitals: AdvancementVitals,
    pub abilities: Vec<AbilityLearningState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementPools {
    pub practices: u16,
    pub trains: u16,
    pub study_points: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementVitals {
    pub hit_points: i32,
    pub mana: i32,
    pub movement: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityLearningState {
    pub ability_id: String,
    pub proficiency: u16,
    pub studied: bool,
    pub practiced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementAction {
    Study {
        ability_id: String,
    },
    Practice {
        ability_id: String,
        sessions: u16,
    },
    TrainAttribute {
        attribute: AdvancementAttribute,
        sessions: u16,
    },
    TrainVital {
        vital: AdvancementVital,
        sessions: u16,
    },
    UseAbility {
        ability_id: String,
        successful_uses: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementAwardReadback {
    pub levels_gained: u16,
    pub practice_per_level: u16,
    pub practice_awarded: u16,
    pub trains_awarded: u16,
    pub study_points_awarded: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementActionReadback {
    pub action_index: usize,
    pub action_kind: String,
    pub target_id: String,
    pub before_value: i32,
    pub after_value: i32,
    pub delta: i32,
    pub before_pool: AdvancementPools,
    pub after_pool: AdvancementPools,
    pub cap_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancementReadback {
    pub schema: String,
    pub character_id: String,
    pub class_id: String,
    pub race_id: String,
    pub from_level: u16,
    pub to_level: u16,
    pub awards: AdvancementAwardReadback,
    pub level_proficiency_cap: u16,
    pub focused_ability_id: Option<String>,
    pub focused_skill_learn_bonus: u16,
    pub final_state: AdvancementState,
    pub actions: Vec<AdvancementActionReadback>,
    pub input_sha256: String,
    pub rules_sha256: String,
    pub catalog_sha256: String,
    pub result_sha256: String,
}
