use crate::rpg::{
    ASHEN_REACH_SKILLED_REWARD_POLICY_ID, RPG_SKILLED_REWARD_SCHEMA, SkilledRewardPolicy,
};

pub(crate) fn ashen_reach_skilled_reward_policy() -> SkilledRewardPolicy {
    SkilledRewardPolicy {
        schema: RPG_SKILLED_REWARD_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_SKILLED_REWARD_POLICY_ID.to_string(),
        source_issue: 1047,
        min_skill_score_milli: 750,
        min_rp_quality_milli: 650,
        min_combined_score_milli: 800,
        skill_weight_milli: 600,
        rp_weight_milli: 400,
        max_reward_tier: 5,
        evidence_rows_required: 3,
        distinct_evidence_kinds_required: 3,
        rewards_revocable: true,
        immortal_or_system_approval_required: true,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}
