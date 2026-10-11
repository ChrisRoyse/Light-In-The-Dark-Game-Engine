use serde::{Deserialize, Serialize};

use super::{CharacterThemeInput, CharacterThemeReadback, ThemeCatalog, ThemeRules};

pub const RPG_FACTION_LEADER_SCHEMA: &str = "litd.rpg.faction_leaders.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionOrganizationKind {
    Cabal,
    Guild,
    Religion,
    Faction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionLeaderActionKind {
    Recruit,
    Induct,
    SetObjective,
    WageWarfare,
    RewardLoyalty,
    AdvanceRank,
}

impl FactionLeaderActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recruit => "recruit",
            Self::Induct => "induct",
            Self::SetObjective => "set_objective",
            Self::WageWarfare => "wage_warfare",
            Self::RewardLoyalty => "reward_loyalty",
            Self::AdvanceRank => "advance_rank",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionLeaderSignalKind {
    Application,
    SponsorSupport,
    WorldThreat,
    ItemOfPowerThreat,
    LoyaltyReport,
    RankReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderRules {
    pub schema: String,
    pub min_signal_strength_milli: u16,
    pub min_rp_quality_milli: u16,
    pub min_loyalty_milli: u16,
    pub min_warfare_pressure_milli: u16,
    pub max_rank: u16,
}

impl FactionLeaderRules {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_FACTION_LEADER_SCHEMA.to_string(),
            min_signal_strength_milli: 500,
            min_rp_quality_milli: 700,
            min_loyalty_milli: 600,
            min_warfare_pressure_milli: 650,
            max_rank: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderCatalog {
    pub schema: String,
    pub catalog_id: String,
    pub organizations: Vec<FactionOrganizationDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionOrganizationDefinition {
    pub organization_id: String,
    pub display_name: String,
    pub organization_kind: FactionOrganizationKind,
    pub leader_agent_id: u32,
    pub leader_display_name: String,
    pub religion_id: Option<String>,
    pub home_region_id: String,
    pub item_of_power_id: Option<String>,
    pub allowed_actions: Vec<FactionLeaderActionKind>,
    pub ward_tags: Vec<String>,
    pub required_theme_tags: Vec<String>,
    pub forbidden_theme_tags: Vec<String>,
    pub ranks: Vec<FactionRankDefinition>,
    pub enemy_organization_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionRankDefinition {
    pub rank: u16,
    pub title: String,
    pub min_rp_quality_milli: u16,
    pub min_loyalty_milli: u16,
    pub power_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderSignal {
    pub signal_id: String,
    pub organization_id: String,
    pub tick: u64,
    pub kind: FactionLeaderSignalKind,
    pub requested_action: FactionLeaderActionKind,
    pub strength_milli: u16,
    pub rp_quality_milli: u16,
    pub loyalty_milli: u16,
    pub source_cx_id: String,
    pub target_character_id: String,
    pub target_organization_id: Option<String>,
    pub target_item_id: Option<String>,
    pub current_rank: Option<u16>,
    pub requested_rank: Option<u16>,
    pub tags: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderEvaluationInput {
    pub catalog: FactionLeaderCatalog,
    pub theme_catalog: ThemeCatalog,
    pub theme_rules: ThemeRules,
    pub theme_input: CharacterThemeInput,
    pub signal: FactionLeaderSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderDecisionReadback {
    pub schema: String,
    pub decision_id: String,
    pub organization_id: String,
    pub display_name: String,
    pub organization_kind: FactionOrganizationKind,
    pub leader_agent_id: u32,
    pub leader_display_name: String,
    pub signal_id: String,
    pub source_cx_id: String,
    pub target_character_id: String,
    pub action_kind: FactionLeaderActionKind,
    pub accepted: bool,
    pub grade: String,
    pub reason: String,
    pub signal_strength_milli: u16,
    pub rp_quality_milli: u16,
    pub loyalty_milli: u16,
    pub target_rank: Option<u16>,
    pub theme_match: CharacterThemeReadback,
    pub organization_theme_fit: FactionThemeFitReadback,
    pub lowered_effects: Vec<FactionLeaderLoweredEffect>,
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
pub struct FactionThemeFitReadback {
    pub required_theme_tags: Vec<String>,
    pub missing_required_tags: Vec<String>,
    pub forbidden_tags_present: Vec<String>,
    pub religion_required: Option<String>,
    pub religion_match_accepted: bool,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderLoweredEffect {
    pub effect_id: String,
    pub kind: FactionLeaderActionKind,
    pub subject_character_id: String,
    pub ledger_entry: String,
    pub recruit_invitation_id: Option<String>,
    pub membership_status: Option<String>,
    pub objective_id: Option<String>,
    pub raid_order_id: Option<String>,
    pub reward_id: Option<String>,
    pub rank_after: Option<u16>,
    pub unlocked_power_ids: Vec<String>,
    pub leader_order: FactionLeaderLoweredOrder,
    pub effect_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionLeaderOrderSourceKind {
    LoweredAgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionLeaderLoweredOrder {
    pub order_id: String,
    pub tick: u64,
    pub source_kind: FactionLeaderOrderSourceKind,
    pub lowered_agent_id: u32,
    pub organization_id: String,
    pub action_kind: FactionLeaderActionKind,
    pub subject_character_id: String,
    pub target_organization_id: Option<String>,
    pub target_item_id: Option<String>,
    pub rank_after: Option<u16>,
}
