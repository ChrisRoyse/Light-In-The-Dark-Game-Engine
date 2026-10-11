use serde::{Deserialize, Serialize};

use super::InfernalCircleId;

pub const RPG_INFERNAL_ACCESS_SCHEMA: &str = "litd.rpg.infernal_access.v1";
pub const ASHEN_REACH_INFERNAL_ACCESS_POLICY_ID: &str =
    "server.ashen_reach.infernal_access.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalEntryPath {
    LongDescentEarnedCindermark,
    LongDescentDevilBrandedCindermark,
    LesserDevilPact,
    HollowPactSeam,
    DraggedUnderAccident,
}

impl InfernalEntryPath {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LongDescentEarnedCindermark => "long_descent_earned_cindermark",
            Self::LongDescentDevilBrandedCindermark => "long_descent_devil_branded_cindermark",
            Self::LesserDevilPact => "lesser_devil_pact",
            Self::HollowPactSeam => "hollow_pact_seam",
            Self::DraggedUnderAccident => "dragged_under_accident",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CindermarkSource {
    EarnedQuestline,
    DevilBrand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalAccessCostKind {
    CindermarkTrialScar,
    DevilBrandDebt,
    PactDebt,
    FactionSeamDebt,
    DraggedUnderTrauma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalEscapeKind {
    Recall,
    Teleport,
    GatePortal,
    HearthRite,
    ExtractionRitual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAccessPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub destination_circle: InfernalCircleId,
    pub no_recall_inside: bool,
    pub required_gate_id: String,
    pub required_long_descent_ref: String,
    pub required_earned_cindermark_ref: String,
    pub required_hollow_pact_faction_id: String,
    pub required_hollow_pact_power_id: String,
    pub max_access_history: usize,
    pub entry_rules: Vec<InfernalEntryRule>,
    pub denied_escape_kinds: Vec<InfernalEscapeKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalEntryRule {
    pub path: InfernalEntryPath,
    pub cost_kind: InfernalAccessCostKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAccessState {
    pub schema: String,
    pub character_id: String,
    pub party_id: String,
    pub location_id: String,
    pub faction_id: Option<String>,
    pub inside_infernal_deeps: bool,
    pub current_circle: Option<InfernalCircleId>,
    pub no_recall_lock_active: bool,
    pub infernal_debt_count: u16,
    pub access_history: Vec<InfernalEntryPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalEntryRequest {
    pub schema: String,
    pub request_id: String,
    pub character_id: String,
    pub path: InfernalEntryPath,
    pub at_gate_id: Option<String>,
    pub completed_long_descent_ref: Option<String>,
    pub cindermark_ref: Option<String>,
    pub cindermark_source: Option<CindermarkSource>,
    pub lesser_devil_id: Option<String>,
    pub pact_contract_id: Option<String>,
    pub accepted_hidden_cost: bool,
    pub faction_power_id: Option<String>,
    pub thin_place_death_id: Option<String>,
    pub current_location_thin_place: bool,
    pub accident_not_player_chosen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalAccessReadback {
    pub schema: String,
    pub request_id: String,
    pub character_id: String,
    pub path: InfernalEntryPath,
    pub granted: bool,
    pub destination_circle: InfernalCircleId,
    pub no_recall_lock_active: bool,
    pub cost_kind: InfernalAccessCostKind,
    pub infernal_debt_before: u16,
    pub infernal_debt_after: u16,
    pub state_before_sha256: String,
    pub state_after_sha256: String,
    pub policy_sha256: String,
    pub request_sha256: String,
    pub result_sha256: String,
    pub state_after: InfernalAccessState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalEscapeAttempt {
    pub schema: String,
    pub attempt_id: String,
    pub character_id: String,
    pub kind: InfernalEscapeKind,
    pub declared_destination_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalEscapeReadback {
    pub schema: String,
    pub attempt_id: String,
    pub character_id: String,
    pub kind: InfernalEscapeKind,
    pub denied: bool,
    pub denial_reason: String,
    pub state_unchanged: bool,
    pub state_before_sha256: String,
    pub state_after_sha256: String,
    pub policy_sha256: String,
    pub attempt_sha256: String,
    pub result_sha256: String,
}
