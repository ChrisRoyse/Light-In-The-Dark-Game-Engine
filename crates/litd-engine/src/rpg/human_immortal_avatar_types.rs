use serde::{Deserialize, Serialize};

use crate::{math::FixedVec3, sim_order::SimOrder};

pub const RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA: &str = "litd.rpg.human_immortal_avatar.v1";
pub const ASHEN_REACH_HUMAN_IMMORTAL_AVATAR_POLICY_ID: &str =
    "server.ashen_reach.human_immortal_avatar.default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanImmortalAvatarPolicy {
    pub schema: String,
    pub policy_id: String,
    pub source_issue: u32,
    pub admin_auth_required: bool,
    pub coembodiment_required: bool,
    pub human_override_authoritative: bool,
    pub deterministic_lowering_required: bool,
    pub audit_ledger_required: bool,
    pub revocable_grants_required: bool,
    pub player_visible_single_identity_required: bool,
    pub ai_hot_path_mutation_allowed: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanImmortalAvatarCommandKind {
    PossessNpc,
    PossessBoss,
    RunLiveEvent,
    GrantBoon,
    GrantTattoo,
    GrantTitle,
    GrantSuperPower,
    PromoteRank,
    MakeRuling,
}

impl HumanImmortalAvatarCommandKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PossessNpc => "possess_npc",
            Self::PossessBoss => "possess_boss",
            Self::RunLiveEvent => "run_live_event",
            Self::GrantBoon => "grant_boon",
            Self::GrantTattoo => "grant_tattoo",
            Self::GrantTitle => "grant_title",
            Self::GrantSuperPower => "grant_super_power",
            Self::PromoteRank => "promote_rank",
            Self::MakeRuling => "make_ruling",
        }
    }

    pub const fn is_grant(self) -> bool {
        matches!(
            self,
            Self::GrantBoon | Self::GrantTattoo | Self::GrantTitle | Self::GrantSuperPower
        )
    }

    pub const fn is_possession(self) -> bool {
        matches!(self, Self::PossessNpc | Self::PossessBoss)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanImmortalAvatarCommand {
    pub command_id: String,
    pub kind: HumanImmortalAvatarCommandKind,
    pub subject_ref: String,
    pub target_body_id: Option<u32>,
    pub possession_velocity: Option<FixedVec3>,
    pub artifact_ref: Option<String>,
    pub summary: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanImmortalAvatarSession {
    pub session_id: String,
    pub admin_id: String,
    pub admin_role: String,
    pub immortal_id: String,
    pub public_identity: String,
    pub coembodiment_session_id: String,
    pub admin_authenticated: bool,
    pub commands: Vec<HumanImmortalAvatarCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanImmortalAvatarEffect {
    pub effect_id: String,
    pub command_id: String,
    pub kind: HumanImmortalAvatarCommandKind,
    pub subject_ref: String,
    pub artifact_ref: Option<String>,
    pub ledger_entry: String,
    pub audit_actor_id: String,
    pub public_identity: String,
    pub revocable: bool,
    pub sim_order: Option<SimOrder>,
    pub evidence_refs: Vec<String>,
    pub effect_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanImmortalAvatarReadback {
    pub schema: String,
    pub policy_id: String,
    pub source_issue: u32,
    pub session_id: String,
    pub admin_id: String,
    pub admin_role: String,
    pub immortal_id: String,
    pub public_identity: String,
    pub coembodiment_session_id: String,
    pub coembodiment_result_sha256: String,
    pub admin_authenticated: bool,
    pub player_visible_single_identity: bool,
    pub human_override_authoritative: bool,
    pub command_count: usize,
    pub possession_order_count: usize,
    pub npc_possession_count: usize,
    pub boss_possession_count: usize,
    pub live_event_count: usize,
    pub grant_count: usize,
    pub promotion_count: usize,
    pub ruling_count: usize,
    pub revocable_grant_count: usize,
    pub lowered_effect_count: usize,
    pub deterministic_lowering_required: bool,
    pub audit_ledger_required: bool,
    pub ai_hot_path_mutation_allowed: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub lowered_effects: Vec<HumanImmortalAvatarEffect>,
    pub policy_sha256: String,
    pub session_sha256: String,
    pub audit_ledger_sha256: String,
    pub result_sha256: String,
}
