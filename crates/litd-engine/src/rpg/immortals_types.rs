use serde::{Deserialize, Serialize};

use crate::{math::FixedVec3, sim_order::SimOrder};

use super::{CharacterThemeInput, CharacterThemeReadback, ThemeCatalog, ThemeRules};

pub const RPG_IMMORTAL_SCHEMA: &str = "litd.rpg.immortals.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmortalActionKind {
    RunEvent,
    GrantEmpowerment,
    IssueQuest,
    EnforceRoleplay,
    PossessMob,
}

impl ImmortalActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunEvent => "run_event",
            Self::GrantEmpowerment => "grant_empowerment",
            Self::IssueQuest => "issue_quest",
            Self::EnforceRoleplay => "enforce_roleplay",
            Self::PossessMob => "possess_mob",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmortalSignalKind {
    Prayer,
    ShrineVisit,
    RoleplayScene,
    QuestOpportunity,
    RuleBreak,
    EventOpportunity,
    PossessionRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalRules {
    pub schema: String,
    pub min_signal_strength_milli: u16,
    pub min_rp_quality_milli: u16,
    pub max_empowerment_tier: u16,
}

impl ImmortalRules {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_IMMORTAL_SCHEMA.to_string(),
            min_signal_strength_milli: 500,
            min_rp_quality_milli: 700,
            max_empowerment_tier: 51,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalCatalog {
    pub schema: String,
    pub catalog_id: String,
    pub immortals: Vec<ImmortalDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalDefinition {
    pub immortal_id: String,
    pub display_name: String,
    pub religion_id: String,
    pub allowed_actions: Vec<ImmortalActionKind>,
    pub ward_tags: Vec<String>,
    pub lowered_agent_id: u32,
    pub max_empowerment_tier: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalSignal {
    pub signal_id: String,
    pub immortal_id: String,
    pub tick: u64,
    pub kind: ImmortalSignalKind,
    pub requested_action: ImmortalActionKind,
    pub strength_milli: u16,
    pub rp_quality_milli: u16,
    pub source_cx_id: String,
    pub target_character_id: String,
    pub target_body_id: Option<u32>,
    pub possession_velocity: Option<FixedVec3>,
    pub tags: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalEvaluationInput {
    pub catalog: ImmortalCatalog,
    pub theme_catalog: ThemeCatalog,
    pub theme_rules: ThemeRules,
    pub theme_input: CharacterThemeInput,
    pub signal: ImmortalSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalDecisionReadback {
    pub schema: String,
    pub decision_id: String,
    pub immortal_id: String,
    pub display_name: String,
    pub religion_id: String,
    pub signal_id: String,
    pub source_cx_id: String,
    pub target_character_id: String,
    pub action_kind: ImmortalActionKind,
    pub accepted: bool,
    pub grade: String,
    pub reason: String,
    pub signal_strength_milli: u16,
    pub rp_quality_milli: u16,
    pub theme_match: CharacterThemeReadback,
    pub lowered_effects: Vec<ImmortalLoweredEffect>,
    pub effect_count: usize,
    pub evidence_refs: Vec<String>,
    pub catalog_sha256: String,
    pub signal_sha256: String,
    pub rules_sha256: String,
    pub theme_result_sha256: String,
    pub decision_ledger_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmortalLoweredEffect {
    pub effect_id: String,
    pub kind: ImmortalActionKind,
    pub subject_character_id: String,
    pub ledger_entry: String,
    pub empowerment_tier: Option<u16>,
    pub quest_id: Option<String>,
    pub event_id: Option<String>,
    pub enforcement_severity: Option<String>,
    pub sim_order: Option<SimOrder>,
    pub effect_sha256: String,
}
