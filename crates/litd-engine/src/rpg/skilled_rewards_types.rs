use serde::{Deserialize, Serialize};

pub const RPG_SKILLED_REWARD_SCHEMA: &str = "litd.rpg.skilled_rewards.v1";
pub const ASHEN_REACH_SKILLED_REWARD_POLICY_ID: &str = "server.ashen_reach.skilled_rewards.default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkilledRewardPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub min_skill_score_milli: u16,
    pub min_rp_quality_milli: u16,
    pub min_combined_score_milli: u16,
    pub skill_weight_milli: u16,
    pub rp_weight_milli: u16,
    pub max_reward_tier: u16,
    pub evidence_rows_required: usize,
    pub distinct_evidence_kinds_required: usize,
    pub rewards_revocable: bool,
    pub immortal_or_system_approval_required: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkilledPlayEvidenceKind {
    CombatProwess,
    PvpVictory,
    AbilityTiming,
    ObjectivePlay,
    BossMechanic,
    Achievement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkilledRewardKind {
    ImmortalBoon,
    Title,
    Tattoo,
    AdvancementCredit,
    FactionRenown,
}

impl SkilledRewardKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ImmortalBoon => "immortal_boon",
            Self::Title => "title",
            Self::Tattoo => "tattoo",
            Self::AdvancementCredit => "advancement_credit",
            Self::FactionRenown => "faction_renown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkilledPlayEvidence {
    pub evidence_id: String,
    pub kind: SkilledPlayEvidenceKind,
    pub metric_milli: u16,
    pub difficulty_milli: u16,
    pub witnessed_by_system: bool,
    pub source_ref: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkilledRewardCandidate {
    pub candidate_id: String,
    pub character_id: String,
    pub character_name: String,
    pub class_id: String,
    pub rp_quality_milli: u16,
    pub requested_reward_kind: SkilledRewardKind,
    pub immortal_id: Option<String>,
    pub evidence: Vec<SkilledPlayEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkilledRewardLoweredEffect {
    pub reward_id: String,
    pub reward_kind: SkilledRewardKind,
    pub subject_character_id: String,
    pub tier: u16,
    pub ledger_entry: String,
    pub revocable: bool,
    pub approval_actor_id: String,
    pub effect_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkilledRewardReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub candidate_id: String,
    pub character_id: String,
    pub requested_reward_kind: SkilledRewardKind,
    pub accepted: bool,
    pub grade: String,
    pub reason: String,
    pub skill_score_milli: u16,
    pub rp_quality_milli: u16,
    pub combined_score_milli: u16,
    pub evidence_count: usize,
    pub distinct_evidence_kind_count: usize,
    pub effect_count: usize,
    pub lowered_effects: Vec<SkilledRewardLoweredEffect>,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub candidate_sha256: String,
    pub policy_sha256: String,
    pub decision_ledger_sha256: String,
    pub result_sha256: String,
}
