use serde::{Deserialize, Serialize};

use super::InfernalCircleId;

pub const RPG_INFERNAL_DEEPS_SCHEMA: &str = "litd.rpg.infernal_deeps.v1";
pub const ASHEN_REACH_INFERNAL_DEEPS_PACKAGE_ID: &str = "server.ashen_reach.infernal_deeps.package";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalDeepsPackagePolicy {
    pub schema: String,
    pub package_id: String,
    pub source_issue: u32,
    pub display_name: String,
    pub ordered_hell_not_outer_dark: bool,
    pub group_required: bool,
    pub no_recall_inside: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub required_child_issues: Vec<u32>,
}

impl InfernalDeepsPackagePolicy {
    pub fn ashen_reach_defaults() -> Self {
        Self {
            schema: RPG_INFERNAL_DEEPS_SCHEMA.to_string(),
            package_id: ASHEN_REACH_INFERNAL_DEEPS_PACKAGE_ID.to_string(),
            source_issue: 1063,
            display_name: "The Infernal Deeps".to_string(),
            ordered_hell_not_outer_dark: true,
            group_required: true,
            no_recall_inside: true,
            built_in_map_required: false,
            grants_map_or_minimap: false,
            grants_gps_markers: false,
            required_child_issues: vec![1066, 1070, 1067, 1068, 1069, 1064],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalDeepsChildHashes {
    pub access_policy_sha256: String,
    pub attrition_policy_sha256: String,
    pub zone_topology_sha256: String,
    pub boss_hierarchy_sha256: String,
    pub relic_vaults_sha256: String,
    pub withering_raid_path_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalDeepsPackageReadback {
    pub schema: String,
    pub package_id: String,
    pub source_issue: u32,
    pub display_name: String,
    pub ordered_hell_not_outer_dark: bool,
    pub group_required: bool,
    pub no_recall_inside: bool,
    pub entry_circle: InfernalCircleId,
    pub terminal_circle: InfernalCircleId,
    pub access_path_count: usize,
    pub denied_escape_kind_count: usize,
    pub zone_count: usize,
    pub hazard_kind_count: usize,
    pub boss_count: usize,
    pub apex_agent_id: String,
    pub relic_vault_count: usize,
    pub hellforged_item_count: usize,
    pub world_finite_item_total: u16,
    pub highest_power_rating_milli: u16,
    pub withering_raid_phase_count: usize,
    pub withering_minimum_party_size: u8,
    pub withering_minimum_character_level: u16,
    pub wailing_tombs_reagent_source: bool,
    pub throne_road_relic_vault_source: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub child_hashes: InfernalDeepsChildHashes,
    pub policy_sha256: String,
    pub result_sha256: String,
}
