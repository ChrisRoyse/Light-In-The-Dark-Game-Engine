use crate::rpg::{
    ASHEN_REACH_HALL_OF_REMEMBERED_POLICY_ID, HallOfRememberedPolicy, RPG_HALL_OF_REMEMBERED_SCHEMA,
};

pub(crate) fn ashen_reach_hall_of_remembered_policy() -> HallOfRememberedPolicy {
    HallOfRememberedPolicy {
        schema: RPG_HALL_OF_REMEMBERED_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_HALL_OF_REMEMBERED_POLICY_ID.to_string(),
        source_issue: 1065,
        built_in_map_required: false,
        persistent_across_server: true,
        auto_populate_from_character_record: true,
        opt_in_player_tributes_only: true,
        browsable_in_galdmere: true,
        browsable_online: true,
        galdmere_surface_id: "surface.galdmere.mourning_wall".to_string(),
        online_surface_id: "surface.online.hall_of_remembered".to_string(),
        minimum_deed_count: 3,
        maximum_tribute_count: 32,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}
