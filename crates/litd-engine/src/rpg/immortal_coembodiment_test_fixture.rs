use crate::rpg::{
    ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID, ImmortalCoembodimentPolicy,
    RPG_IMMORTAL_COEMBODIMENT_SCHEMA,
};

pub(crate) fn ashen_reach_immortal_coembodiment_policy() -> ImmortalCoembodimentPolicy {
    ImmortalCoembodimentPolicy {
        schema: RPG_IMMORTAL_COEMBODIMENT_SCHEMA.to_string(),
        policy_id: ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID.to_string(),
        source_issue: 1046,
        ai_driver_required: true,
        human_driver_allowed: true,
        human_override_authoritative: true,
        player_visible_single_identity: true,
        deterministic_lowering_required: true,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}
