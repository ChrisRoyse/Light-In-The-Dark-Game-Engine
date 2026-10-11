use serde::{Deserialize, Serialize};

use super::AbilityCatalogReadback;

pub const RPG_UNHALLOWED_KNIGHT_SCHEMA: &str = "litd.rpg.unhallowed_knight.v1";
pub const ASHEN_REACH_UNHALLOWED_KNIGHT_POLICY_ID: &str =
    "server.ashen_reach.unhallowed_knight.default";
pub const UNHALLOWED_KNIGHT_CLASS_ID: &str = "class.unhallowed_knight";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnhallowedKnightPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub class_id: String,
    pub display_name: String,
    pub prime_stat: String,
    pub required_alignment: String,
    pub minimum_strength: i16,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub soul_weapon: SoulWeaponRules,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulWeaponRules {
    pub binding_rite_id: String,
    pub transfer_rite_id: String,
    pub soul_resource_id: String,
    pub every_life_claimed_strengthens: bool,
    pub accumulated_power_transferable: bool,
    pub destroying_weapon_wounds_master: bool,
    pub initial_power_milli: u16,
    pub power_per_soul_milli: u16,
    pub maximum_power_milli: u16,
    pub transfer_efficiency_milli: u16,
    pub destruction_wound_milli: u16,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnhallowedKnightCharacter {
    pub character_id: String,
    pub character_name: String,
    pub class_id: String,
    pub alignment: String,
    pub strength: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulWeaponBindRequest {
    pub weapon_id: String,
    pub weapon_display_name: String,
    pub rite_id: String,
    pub accepted_irreversible_binding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulVictimKind {
    PlayerCharacter,
    Npc,
    SummonedFiend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulKillEvent {
    pub event_id: String,
    pub victim_id: String,
    pub victim_kind: SoulVictimKind,
    pub soul_value: u16,
    pub witnessed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulWeaponTransferRequest {
    pub transfer_id: String,
    pub new_weapon_id: String,
    pub new_weapon_display_name: String,
    pub rite_id: String,
    pub accepted_soul_scarring_cost: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulWeaponTransferRecord {
    pub transfer_id: String,
    pub from_weapon_id: String,
    pub to_weapon_id: String,
    pub power_before_milli: u16,
    pub power_after_milli: u16,
    pub efficiency_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulBoundWeaponState {
    pub schema: String,
    pub character_id: String,
    pub character_name: String,
    pub class_id: String,
    pub alignment: String,
    pub strength: i16,
    pub bound_weapon_id: String,
    pub weapon_display_name: String,
    pub souls_siphoned: u32,
    pub power_rating_milli: u16,
    pub master_wound_milli: u16,
    pub destroyed: bool,
    pub kill_event_ids: Vec<String>,
    pub transfer_history: Vec<SoulWeaponTransferRecord>,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulWeaponActionKind {
    Bind,
    KillSiphon,
    Transfer,
    Destroy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoulWeaponActionReadback {
    pub schema: String,
    pub action: SoulWeaponActionKind,
    pub character_id: String,
    pub weapon_id_before: Option<String>,
    pub weapon_id_after: Option<String>,
    pub souls_before: u32,
    pub souls_after: u32,
    pub power_before_milli: u16,
    pub power_after_milli: u16,
    pub master_wound_before_milli: u16,
    pub master_wound_after_milli: u16,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub state_sha256: String,
    pub result_sha256: String,
    pub after_state: SoulBoundWeaponState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnhallowedKnightReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub class_id: String,
    pub display_name: String,
    pub prime_stat: String,
    pub required_alignment: String,
    pub minimum_strength: i16,
    pub ability_count: usize,
    pub ability_readback: AbilityCatalogReadback,
    pub every_life_claimed_strengthens: bool,
    pub accumulated_power_transferable: bool,
    pub destroying_weapon_wounds_master: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub policy_sha256: String,
    pub ability_catalog_sha256: String,
    pub result_sha256: String,
}
