use crate::rpg::{
    ASHEN_REACH_UNHALLOWED_KNIGHT_POLICY_ID, RPG_UNHALLOWED_KNIGHT_SCHEMA, SoulWeaponRules,
    UNHALLOWED_KNIGHT_CLASS_ID, UnhallowedKnightPolicy,
};

pub(crate) fn ashen_reach_unhallowed_knight_policy() -> UnhallowedKnightPolicy {
    UnhallowedKnightPolicy {
        schema: RPG_UNHALLOWED_KNIGHT_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_UNHALLOWED_KNIGHT_POLICY_ID.to_string(),
        source_issue: 1062,
        class_id: UNHALLOWED_KNIGHT_CLASS_ID.to_string(),
        display_name: "Unhallowed Knight".to_string(),
        prime_stat: "strength".to_string(),
        required_alignment: "evil".to_string(),
        minimum_strength: 16,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        soul_weapon: SoulWeaponRules {
            binding_rite_id: "rite.unhallowed.black_oath_binding".to_string(),
            transfer_rite_id: "rite.unhallowed.iron_to_iron_transfer".to_string(),
            soul_resource_id: "resource.siphoned_souls".to_string(),
            every_life_claimed_strengthens: true,
            accumulated_power_transferable: true,
            destroying_weapon_wounds_master: true,
            initial_power_milli: 350,
            power_per_soul_milli: 25,
            maximum_power_milli: 1000,
            transfer_efficiency_milli: 900,
            destruction_wound_milli: 300,
            grants_map_or_minimap: false,
            grants_gps_markers: false,
        },
    }
}
