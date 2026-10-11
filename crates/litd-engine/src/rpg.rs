//! RPG gameplay data contracts shared by editor authoring and runtime lowering.

#[path = "rpg/abilities.rs"]
mod abilities;
#[path = "rpg/abilities_content.rs"]
mod abilities_content;
#[path = "rpg/abilities_readback.rs"]
mod abilities_readback;
#[path = "rpg/abilities_stealth.rs"]
mod abilities_stealth;
#[path = "rpg/abilities_stealth_extra.rs"]
mod abilities_stealth_extra;
#[path = "rpg/abilities_stealth_kits.rs"]
mod abilities_stealth_kits;
#[path = "rpg/abilities_support.rs"]
mod abilities_support;
#[path = "rpg/abilities_types.rs"]
mod abilities_types;
#[path = "rpg/abilities_validate.rs"]
mod abilities_validate;
#[path = "rpg/advancement.rs"]
mod advancement;
#[path = "rpg/advancement_apply.rs"]
mod advancement_apply;
#[path = "rpg/advancement_content.rs"]
mod advancement_content;
#[path = "rpg/advancement_readback.rs"]
mod advancement_readback;
#[path = "rpg/advancement_types.rs"]
mod advancement_types;
#[path = "rpg/advancement_state_store.rs"]
mod advancement_state_store;
#[path = "rpg/advancement_validate.rs"]
mod advancement_validate;
#[path = "rpg/area_registry.rs"]
mod area_registry;
#[path = "rpg/area_reset_repop.rs"]
mod area_reset_repop;
#[path = "rpg/area_reset_repop_content.rs"]
mod area_reset_repop_content;
#[path = "rpg/area_reset_repop_table.rs"]
mod area_reset_repop_table;
#[path = "rpg/area_reset_repop_types.rs"]
mod area_reset_repop_types;
#[path = "rpg/area_specs.rs"]
mod area_specs;
#[path = "rpg/area_specs_content.rs"]
mod area_specs_content;
#[path = "rpg/area_specs_types.rs"]
mod area_specs_types;
#[path = "rpg/area_terrain_acoustics.rs"]
mod area_terrain_acoustics;
#[path = "rpg/area_terrain_acoustics_content.rs"]
mod area_terrain_acoustics_content;
#[path = "rpg/area_terrain_acoustics_table.rs"]
mod area_terrain_acoustics_table;
#[path = "rpg/area_terrain_acoustics_types.rs"]
mod area_terrain_acoustics_types;
#[path = "rpg/attributes.rs"]
mod attributes;
#[path = "rpg/attributes_content.rs"]
mod attributes_content;
#[path = "rpg/attributes_types.rs"]
mod attributes_types;
#[path = "rpg/attributes_validate.rs"]
mod attributes_validate;
#[path = "rpg/authored_content.rs"]
mod authored_content;
#[path = "rpg/class_registry.rs"]
mod class_registry;
#[path = "rpg/class_eligibility.rs"]
mod class_eligibility;
#[path = "rpg/class_specs.rs"]
mod class_specs;
#[path = "rpg/class_specs_content.rs"]
mod class_specs_content;
#[path = "rpg/class_specs_primitives.rs"]
mod class_specs_primitives;
#[path = "rpg/class_specs_types.rs"]
mod class_specs_types;
#[path = "rpg/class_specs_validate_v2.rs"]
mod class_specs_validate_v2;
#[path = "rpg/combat.rs"]
mod combat;
#[path = "rpg/combat_content.rs"]
mod combat_content;
#[path = "rpg/combat_readback.rs"]
mod combat_readback;
#[path = "rpg/combat_resolve.rs"]
mod combat_resolve;
#[path = "rpg/combat_types.rs"]
mod combat_types;
#[path = "rpg/combat_validate.rs"]
mod combat_validate;
#[path = "rpg/faction_leaders.rs"]
mod faction_leaders;
#[path = "rpg/faction_roster.rs"]
mod faction_roster;
#[path = "rpg/faction_leaders_content.rs"]
mod faction_leaders_content;
#[path = "rpg/faction_leaders_readback.rs"]
mod faction_leaders_readback;
#[path = "rpg/faction_leaders_types.rs"]
mod faction_leaders_types;
#[path = "rpg/faction_leaders_validate.rs"]
mod faction_leaders_validate;
#[path = "rpg/hall_of_remembered.rs"]
mod hall_of_remembered;
#[path = "rpg/hall_of_remembered_content.rs"]
mod hall_of_remembered_content;
#[path = "rpg/hall_of_remembered_types.rs"]
mod hall_of_remembered_types;
#[path = "rpg/hall_of_remembered_validate.rs"]
mod hall_of_remembered_validate;
#[path = "rpg/human_immortal_avatar.rs"]
mod human_immortal_avatar;
#[path = "rpg/human_immortal_avatar_content.rs"]
mod human_immortal_avatar_content;
#[path = "rpg/human_immortal_avatar_support.rs"]
mod human_immortal_avatar_support;
#[path = "rpg/human_immortal_avatar_types.rs"]
mod human_immortal_avatar_types;
#[path = "rpg/human_immortal_avatar_validate.rs"]
mod human_immortal_avatar_validate;
#[path = "rpg/immortal_coembodiment.rs"]
mod immortal_coembodiment;
#[path = "rpg/immortal_coembodiment_content.rs"]
mod immortal_coembodiment_content;
#[path = "rpg/immortal_coembodiment_types.rs"]
mod immortal_coembodiment_types;
#[path = "rpg/immortals.rs"]
mod immortals;
#[path = "rpg/immortals_content.rs"]
mod immortals_content;
#[path = "rpg/immortals_types.rs"]
mod immortals_types;
#[path = "rpg/immortals_validate.rs"]
mod immortals_validate;
#[path = "rpg/infernal_access.rs"]
mod infernal_access;
#[path = "rpg/infernal_access_content.rs"]
mod infernal_access_content;
#[path = "rpg/infernal_access_types.rs"]
mod infernal_access_types;
#[path = "rpg/infernal_access_validate.rs"]
mod infernal_access_validate;
#[path = "rpg/infernal_attrition.rs"]
mod infernal_attrition;
#[path = "rpg/infernal_attrition_content.rs"]
mod infernal_attrition_content;
#[path = "rpg/infernal_attrition_types.rs"]
mod infernal_attrition_types;
#[path = "rpg/infernal_boss_agents.rs"]
mod infernal_boss_agents;
#[path = "rpg/infernal_boss_agents_content.rs"]
mod infernal_boss_agents_content;
#[path = "rpg/infernal_boss_agents_runtime.rs"]
mod infernal_boss_agents_runtime;
#[path = "rpg/infernal_boss_agents_types.rs"]
mod infernal_boss_agents_types;
#[path = "rpg/infernal_deeps.rs"]
mod infernal_deeps;
#[path = "rpg/infernal_deeps_content.rs"]
mod infernal_deeps_content;
#[path = "rpg/infernal_deeps_runtime.rs"]
mod infernal_deeps_runtime;
#[path = "rpg/infernal_deeps_types.rs"]
mod infernal_deeps_types;
#[path = "rpg/infernal_relic_vaults.rs"]
mod infernal_relic_vaults;
#[path = "rpg/infernal_relic_vaults_content.rs"]
mod infernal_relic_vaults_content;
#[path = "rpg/infernal_relic_vaults_runtime.rs"]
mod infernal_relic_vaults_runtime;
#[path = "rpg/infernal_relic_vaults_types.rs"]
mod infernal_relic_vaults_types;
#[path = "rpg/infernal_zones.rs"]
mod infernal_zones;
#[path = "rpg/infernal_zones_content.rs"]
mod infernal_zones_content;
#[path = "rpg/infernal_zones_types.rs"]
mod infernal_zones_types;
#[path = "rpg/item_affects.rs"]
mod item_affects;
#[path = "rpg/item_flags.rs"]
mod item_flags;
#[path = "rpg/item_ingest.rs"]
mod item_ingest;
#[path = "rpg/item_ingest_parse.rs"]
mod item_ingest_parse;
#[path = "rpg/item_ingest_types.rs"]
mod item_ingest_types;
#[path = "rpg/item_kinds.rs"]
mod item_kinds;
#[path = "rpg/item_limited.rs"]
mod item_limited;
#[path = "rpg/item_limited_content.rs"]
mod item_limited_content;
#[path = "rpg/item_materials.rs"]
mod item_materials;
#[path = "rpg/item_specs.rs"]
mod item_specs;
#[path = "rpg/item_specs_content.rs"]
mod item_specs_content;
#[path = "rpg/item_specs_types.rs"]
mod item_specs_types;
#[path = "rpg/item_weapons.rs"]
mod item_weapons;
#[path = "rpg/item_wear_slots.rs"]
mod item_wear_slots;
#[path = "rpg/map_access.rs"]
mod map_access;
#[path = "rpg/map_access_store.rs"]
mod map_access_store;
#[path = "rpg/map_access_types.rs"]
mod map_access_types;
#[path = "rpg/mortality.rs"]
mod mortality;
#[path = "rpg/mortality_content.rs"]
mod mortality_content;
#[path = "rpg/mortality_types.rs"]
mod mortality_types;
#[path = "rpg/outfit_kit.rs"]
mod outfit_kit;
#[path = "rpg/outfit_kit_content.rs"]
mod outfit_kit_content;
#[path = "rpg/outfit_kit_types.rs"]
mod outfit_kit_types;
#[path = "rpg/overworld_topology.rs"]
mod overworld_topology;
#[path = "rpg/overworld_topology_content.rs"]
mod overworld_topology_content;
#[path = "rpg/overworld_topology_doc.rs"]
mod overworld_topology_doc;
#[path = "rpg/overworld_topology_types.rs"]
mod overworld_topology_types;
#[path = "rpg/race_registry.rs"]
mod race_registry;
#[path = "rpg/race_specs.rs"]
mod race_specs;
#[path = "rpg/race_specs_content.rs"]
mod race_specs_content;
#[path = "rpg/race_specs_types.rs"]
mod race_specs_types;
#[path = "rpg/racial_stealth.rs"]
mod racial_stealth;
#[path = "rpg/racial_stealth_content.rs"]
mod racial_stealth_content;
#[path = "rpg/racial_stealth_types.rs"]
mod racial_stealth_types;
#[path = "rpg/racial_stealth_validate.rs"]
mod racial_stealth_validate;
#[path = "rpg/road_network.rs"]
mod road_network;
#[path = "rpg/road_network_content.rs"]
mod road_network_content;
#[path = "rpg/road_network_types.rs"]
mod road_network_types;
#[path = "rpg/shop_economy.rs"]
mod shop_economy;
#[path = "rpg/shop_economy_content.rs"]
mod shop_economy_content;
#[path = "rpg/shop_economy_types.rs"]
mod shop_economy_types;
#[path = "rpg/skilled_rewards.rs"]
mod skilled_rewards;
#[path = "rpg/skilled_rewards_content.rs"]
mod skilled_rewards_content;
#[path = "rpg/skilled_rewards_types.rs"]
mod skilled_rewards_types;
#[path = "rpg/stealth_acquisition.rs"]
mod stealth_acquisition;
#[path = "rpg/stealth_acquisition_content.rs"]
mod stealth_acquisition_content;
#[path = "rpg/stealth_acquisition_types.rs"]
mod stealth_acquisition_types;
#[path = "rpg/stealth_acquisition_validate.rs"]
mod stealth_acquisition_validate;
#[path = "rpg/themes.rs"]
mod themes;
#[path = "rpg/themes_content.rs"]
mod themes_content;
#[path = "rpg/themes_readback.rs"]
mod themes_readback;
#[path = "rpg/themes_types.rs"]
mod themes_types;
#[path = "rpg/themes_validate.rs"]
mod themes_validate;
#[path = "rpg/undeath.rs"]
mod undeath;
#[path = "rpg/undeath_content.rs"]
mod undeath_content;
#[path = "rpg/undeath_types.rs"]
mod undeath_types;
#[path = "rpg/unhallowed_knight.rs"]
mod unhallowed_knight;
#[path = "rpg/unhallowed_knight_abilities.rs"]
mod unhallowed_knight_abilities;
#[path = "rpg/unhallowed_knight_content.rs"]
mod unhallowed_knight_content;
#[path = "rpg/unhallowed_knight_types.rs"]
mod unhallowed_knight_types;
#[path = "rpg/unhallowed_knight_validate.rs"]
mod unhallowed_knight_validate;
#[path = "rpg/withering_raid_path.rs"]
mod withering_raid_path;
#[path = "rpg/withering_raid_path_content.rs"]
mod withering_raid_path_content;
#[path = "rpg/withering_raid_path_runtime.rs"]
mod withering_raid_path_runtime;
#[path = "rpg/withering_raid_path_types.rs"]
mod withering_raid_path_types;

pub use abilities::*;
pub use abilities_content::*;
pub use abilities_stealth::*;
pub use abilities_types::*;
pub use advancement::*;
pub use advancement_content::*;
pub use advancement_types::*;
pub use advancement_state_store::*;
pub use area_registry::*;
pub use area_reset_repop::*;
pub use area_reset_repop_content::*;
pub use area_reset_repop_types::*;
pub use area_specs::*;
pub use area_specs_content::*;
pub use area_specs_types::*;
pub use area_terrain_acoustics::*;
pub use area_terrain_acoustics_content::*;
pub use area_terrain_acoustics_types::*;
pub use attributes::*;
pub use attributes_content::*;
pub use attributes_types::*;
pub use authored_content::*;
pub use class_registry::*;
pub use class_eligibility::*;
pub use class_specs::*;
pub use class_specs_content::*;
pub use class_specs_types::*;
pub use combat::*;
pub use combat_content::*;
pub use combat_types::*;
pub use faction_leaders::*;
pub use faction_leaders_content::*;
pub use faction_leaders_types::*;
pub use faction_roster::*;
pub use hall_of_remembered::*;
pub use hall_of_remembered_content::*;
pub use hall_of_remembered_types::*;
pub use hall_of_remembered_validate::{
    validate_hall_memorial_readback, validate_hall_memorial_record,
    validate_hall_of_remembered_policy,
};
pub use human_immortal_avatar::*;
pub use human_immortal_avatar_content::*;
pub use human_immortal_avatar_types::*;
pub use human_immortal_avatar_validate::{
    human_immortal_avatar_policy_hash, validate_human_immortal_avatar_policy,
    validate_human_immortal_avatar_readback,
};
pub use immortal_coembodiment::*;
pub use immortal_coembodiment_content::*;
pub use immortal_coembodiment_types::*;
pub use immortals::*;
pub use immortals_content::*;
pub use immortals_types::*;
pub use infernal_access::*;
pub use infernal_access_content::*;
pub use infernal_access_types::*;
pub use infernal_access_validate::{
    validate_infernal_access_policy, validate_infernal_access_state,
    validate_infernal_entry_request, validate_infernal_escape_attempt,
};
pub use infernal_attrition::*;
pub use infernal_attrition_content::*;
pub use infernal_attrition_types::*;
pub use infernal_boss_agents::*;
pub use infernal_boss_agents_content::*;
pub use infernal_boss_agents_runtime::*;
pub use infernal_boss_agents_types::*;
pub use infernal_deeps::*;
pub use infernal_deeps_content::*;
pub use infernal_deeps_runtime::*;
pub use infernal_deeps_types::*;
pub use infernal_relic_vaults::*;
pub use infernal_relic_vaults_content::*;
pub use infernal_relic_vaults_runtime::*;
pub use infernal_relic_vaults_types::*;
pub use infernal_zones::*;
pub use infernal_zones_content::*;
pub use infernal_zones_types::*;
pub use item_affects::*;
pub use item_flags::*;
pub use item_ingest::*;
pub use item_ingest_types::*;
pub use item_kinds::*;
pub use item_limited::*;
pub use item_limited_content::*;
pub use item_materials::*;
pub use item_specs::*;
pub use item_specs_content::*;
pub use item_specs_types::*;
pub use item_weapons::*;
pub use item_wear_slots::*;
pub use map_access::*;
pub use map_access_store::*;
pub use map_access_types::*;
pub use mortality::*;
pub use mortality_content::*;
pub use mortality_types::*;
pub use outfit_kit::*;
pub use outfit_kit_content::*;
pub use outfit_kit_types::*;
pub use overworld_topology::*;
pub use overworld_topology_content::*;
pub use overworld_topology_doc::*;
pub use overworld_topology_types::*;
pub use race_registry::*;
pub use race_specs::*;
pub use race_specs_content::*;
pub use race_specs_types::*;
pub use racial_stealth::*;
pub use racial_stealth_content::*;
pub use racial_stealth_types::*;
pub use racial_stealth_validate::*;
pub use road_network::*;
pub use road_network_content::*;
pub use road_network_types::*;
pub use shop_economy::*;
pub use shop_economy_content::*;
pub use shop_economy_types::*;
pub use skilled_rewards::*;
pub use skilled_rewards_content::*;
pub use skilled_rewards_types::*;
pub use stealth_acquisition::*;
pub use stealth_acquisition_content::*;
pub use stealth_acquisition_types::*;
pub use stealth_acquisition_validate::*;
pub use themes::*;
pub use themes_content::*;
pub use themes_types::*;
pub use undeath::*;
pub use undeath_content::*;
pub use undeath_types::*;
pub use unhallowed_knight::*;
pub use unhallowed_knight_abilities::*;
pub use unhallowed_knight_content::*;
pub use unhallowed_knight_types::*;
pub use unhallowed_knight_validate::*;
pub use withering_raid_path::*;
pub use withering_raid_path_content::*;
pub use withering_raid_path_runtime::*;
pub use withering_raid_path_types::*;

/// Canonical SHA-256 over a value's JSON serialization. Shared by all rpg
/// content/readback modules (#1491 — replaces 43 byte-identical private copies).
/// The error context is the concrete type name via `std::any::type_name`, which
/// is more precise for diagnosis than the prior hand-written per-module strings.
/// The success path is byte-for-byte identical to the removed copies
/// (`serde_json::to_vec` → `litd_hash::sha256_hex`), so all existing hashes are
/// preserved.
pub(crate) fn serialize_sha256<T: serde::Serialize>(
    value: &T,
) -> Result<String, crate::EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| crate::EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize {}: {error}", std::any::type_name::<T>()),
        })
}
