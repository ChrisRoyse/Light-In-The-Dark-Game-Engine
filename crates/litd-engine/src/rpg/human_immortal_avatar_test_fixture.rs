use crate::rpg::{
    ASHEN_REACH_HUMAN_IMMORTAL_AVATAR_POLICY_ID, HumanImmortalAvatarPolicy,
    RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA,
};

pub(crate) fn ashen_reach_human_immortal_avatar_policy() -> HumanImmortalAvatarPolicy {
    HumanImmortalAvatarPolicy {
        schema: RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA.to_string(),
        policy_id: ASHEN_REACH_HUMAN_IMMORTAL_AVATAR_POLICY_ID.to_string(),
        source_issue: 1045,
        admin_auth_required: true,
        coembodiment_required: true,
        human_override_authoritative: true,
        deterministic_lowering_required: true,
        audit_ledger_required: true,
        revocable_grants_required: true,
        player_visible_single_identity_required: true,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}
