use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const RPG_ABILITY_SCHEMA: &str = "litd.rpg.abilities.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityKind {
    Skill,
    Spell,
    Supplication,
    Song,
    CabalPower,
    RacialInnate,
}

impl AbilityKind {
    pub const ALL: [Self; 6] = [
        Self::Skill,
        Self::Spell,
        Self::Supplication,
        Self::Song,
        Self::CabalPower,
        Self::RacialInnate,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Spell => "spell",
            Self::Supplication => "supplication",
            Self::Song => "song",
            Self::CabalPower => "cabal_power",
            Self::RacialInnate => "racial_innate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityActivation {
    Active,
    Passive,
    Automatic,
}

impl AbilityActivation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Passive => "passive",
            Self::Automatic => "automatic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityTargetMode {
    None,
    SelfOnly,
    Ally,
    Enemy,
    Area,
    Room,
    Object,
    Direction,
}

impl AbilityTargetMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SelfOnly => "self_only",
            Self::Ally => "ally",
            Self::Enemy => "enemy",
            Self::Area => "area",
            Self::Room => "room",
            Self::Object => "object",
            Self::Direction => "direction",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityEffectKind {
    Damage,
    Heal,
    ApplyAffect,
    RemoveAffect,
    ResourceDelta,
    Summon,
    Dispel,
    Movement,
    StateTag,
    ScriptEvent,
}

impl AbilityEffectKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Damage => "damage",
            Self::Heal => "heal",
            Self::ApplyAffect => "apply_affect",
            Self::RemoveAffect => "remove_affect",
            Self::ResourceDelta => "resource_delta",
            Self::Summon => "summon",
            Self::Dispel => "dispel",
            Self::Movement => "movement",
            Self::StateTag => "state_tag",
            Self::ScriptEvent => "script_event",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityStackingPolicy {
    None,
    Refresh,
    Extend,
    StackUpToCap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityCatalog {
    pub schema: String,
    pub catalog_id: String,
    pub abilities: Vec<AbilityDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDefinition {
    pub ability_id: String,
    pub display_name: String,
    pub description: String,
    pub kind: AbilityKind,
    pub activation: AbilityActivation,
    pub category: AbilityCategory,
    pub requirements: AbilityRequirements,
    pub costs: Vec<AbilityCost>,
    pub timing: AbilityTiming,
    pub targeting: AbilityTargeting,
    pub effects: Vec<AbilityEffect>,
    pub tags: Vec<String>,
    pub grants_tags: Vec<String>,
    pub blocks_tags: Vec<String>,
    pub trigger_tags: Vec<String>,
    pub ui_slot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityCategory {
    pub family_id: String,
    pub school_id: Option<String>,
    pub path_id: Option<String>,
    pub branch_id: Option<String>,
    pub rank: u16,
    pub max_rank: u16,
    pub capstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityRequirements {
    pub min_level: u16,
    pub class_ids: Vec<String>,
    pub race_ids: Vec<String>,
    pub cabal_ids: Vec<String>,
    pub alignments: Vec<String>,
    pub ethos: Vec<String>,
    pub deity_domains: Vec<String>,
    pub prerequisites: Vec<AbilityPrerequisite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityPrerequisite {
    pub ability_id: String,
    pub min_rank: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityCost {
    pub resource_id: String,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityTiming {
    pub cast_ticks: u32,
    pub round_ticks: u32,
    pub lag_ticks: u32,
    pub cooldown_ticks: u32,
    pub cooldown_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityTargeting {
    pub mode: AbilityTargetMode,
    pub max_targets: u16,
    pub range_tiles: u16,
    pub requires_line_of_sight: bool,
    pub allow_friendly_fire: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityEffect {
    pub effect_id: String,
    pub kind: AbilityEffectKind,
    pub channel: String,
    pub magnitude_min: i32,
    pub magnitude_max: i32,
    pub duration_ticks: u32,
    pub tick_interval: u32,
    pub stacking_policy: AbilityStackingPolicy,
    pub stack_cap: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCostTotal {
    pub resource_id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityCatalogReadback {
    pub schema: String,
    pub catalog_id: String,
    pub ability_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
    pub activation_counts: BTreeMap<String, usize>,
    pub target_mode_counts: BTreeMap<String, usize>,
    pub effect_kind_counts: BTreeMap<String, usize>,
    pub cost_resource_totals: Vec<ResourceCostTotal>,
    pub prerequisite_edge_count: usize,
    pub max_prerequisite_depth: usize,
    pub school_count: usize,
    pub branch_count: usize,
    pub capstone_count: usize,
    pub ui_slot_count: usize,
    pub trigger_tag_count: usize,
    pub ability_ids: Vec<String>,
    pub input_sha256: String,
    pub result_sha256: String,
}
