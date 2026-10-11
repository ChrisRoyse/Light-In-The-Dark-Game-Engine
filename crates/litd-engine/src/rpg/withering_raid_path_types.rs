use serde::{Deserialize, Serialize};

use super::{InfernalCircleId, UndeadForm};

pub const RPG_WITHERING_RAID_PATH_SCHEMA: &str = "litd.rpg.withering_raid_path.v1";
pub const ASHEN_REACH_WITHERING_RAID_PATH_POLICY_ID: &str =
    "server.ashen_reach.withering_raid_path.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitheringRaidPhaseKind {
    CipherTrail,
    OuterGuardian,
    InnerGuardian,
    ItemOfPowerSeizure,
    WailingTombsHarvest,
    WitheringRite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringRaidPathPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub raid: WitheringFactionRaid,
    pub questline: WitheringQuestline,
}

impl WitheringRaidPathPolicy {
    pub fn ashen_reach_defaults() -> Self {
        Self {
            schema: RPG_WITHERING_RAID_PATH_SCHEMA.to_string(),
            server_policy_id: ASHEN_REACH_WITHERING_RAID_PATH_POLICY_ID.to_string(),
            source_issue: 1064,
            built_in_map_required: false,
            raid: WitheringFactionRaid {
                raid_id: "raid.withering.black_reliquary".to_string(),
                display_name: "Black Reliquary of the Rift-Shard".to_string(),
                attacking_faction_id: "organization.vesper_court".to_string(),
                target_faction_id: "organization.hollow_pact".to_string(),
                target_item_of_power_id: "item.hollow_pact.rift_shard".to_string(),
                source_zone_id: "zone.infernal.wailing_tombs.bone_choir".to_string(),
                source_circle_id: InfernalCircleId::WailingTombs,
                final_guard_ruler_ref: "lord.infernal.wailing_tombs.choirmaster".to_string(),
                final_guard_agent_id: "agent.infernal.wailing_tombs.choirmaster".to_string(),
                outer_guardian_ref: "guardian.hollow_pact.veil_jailer".to_string(),
                inner_guardian_ref: "guardian.hollow_pact.name_eater".to_string(),
                elite_group_required: true,
                faction_leader_order_required: true,
                minimum_party_size: 8,
                minimum_character_level: 47,
                pressure_tier_milli: 1000,
                phases: vec![
                    phase(
                        WitheringRaidPhaseKind::CipherTrail,
                        "phase.withering.cipher_trail",
                        "Vesper cipher trail",
                        "lore.vesper_ledger.name_ash_cipher",
                        "discovery.withering.name_ash",
                        900,
                    ),
                    phase(
                        WitheringRaidPhaseKind::OuterGuardian,
                        "phase.withering.outer_guardian",
                        "Veil Jailer breach",
                        "raid.hollow_pact.outer_guardian_defeated",
                        "component.black_candle",
                        950,
                    ),
                    phase(
                        WitheringRaidPhaseKind::InnerGuardian,
                        "phase.withering.inner_guardian",
                        "Name-Eater reliquary lock",
                        "raid.hollow_pact.inner_guardian_defeated",
                        "component.last_breath_salt",
                        980,
                    ),
                    phase(
                        WitheringRaidPhaseKind::ItemOfPowerSeizure,
                        "phase.withering.rift_shard",
                        "Rift-Shard seizure",
                        "item.hollow_pact.rift_shard.captured",
                        "discovery.withering.rift_shard_witness",
                        1000,
                    ),
                    phase(
                        WitheringRaidPhaseKind::WailingTombsHarvest,
                        "phase.withering.wailing_tombs_harvest",
                        "Wailing Tombs name-ash harvest",
                        "zone.infernal.wailing_tombs.bone_choir.harvested",
                        "component.name_ash",
                        1000,
                    ),
                    phase(
                        WitheringRaidPhaseKind::WitheringRite,
                        "phase.withering.no_name_vigil",
                        "No-Name Vigil",
                        "ritual.withering.no_name_vigil.ready",
                        "immortal.whisper.no_name_bargain",
                        1000,
                    ),
                ],
            },
            questline: WitheringQuestline {
                questline_id: "quest.withering.no_name_vigil".to_string(),
                display_name: "The No-Name Vigil".to_string(),
                secret_in_character: true,
                revealed_by_default_ui: false,
                quest_log_autocomplete: false,
                required_discovery_refs: vec![
                    "discovery.withering.name_ash".to_string(),
                    "discovery.withering.rift_shard_witness".to_string(),
                    "immortal.whisper.no_name_bargain".to_string(),
                ],
                required_components: vec![
                    "component.name_ash".to_string(),
                    "component.black_candle".to_string(),
                    "component.last_breath_salt".to_string(),
                ],
                required_focus_ids: vec![
                    "focus.vessel_of_last_breath".to_string(),
                    "focus.grave_wrappings".to_string(),
                    "focus.veil_anchor".to_string(),
                ],
                allowed_forms: vec![UndeadForm::Lich, UndeadForm::Mummy, UndeadForm::Wraith],
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringFactionRaid {
    pub raid_id: String,
    pub display_name: String,
    pub attacking_faction_id: String,
    pub target_faction_id: String,
    pub target_item_of_power_id: String,
    pub source_zone_id: String,
    pub source_circle_id: InfernalCircleId,
    pub final_guard_ruler_ref: String,
    pub final_guard_agent_id: String,
    pub outer_guardian_ref: String,
    pub inner_guardian_ref: String,
    pub elite_group_required: bool,
    pub faction_leader_order_required: bool,
    pub minimum_party_size: u8,
    pub minimum_character_level: u16,
    pub pressure_tier_milli: u16,
    pub phases: Vec<WitheringRaidPhase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringRaidPhase {
    pub kind: WitheringRaidPhaseKind,
    pub phase_id: String,
    pub display_name: String,
    pub required_ref: String,
    pub unlock_ref: String,
    pub pressure_tier_milli: u16,
    pub in_character_only: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub bypasses_withering_requirements: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringQuestline {
    pub questline_id: String,
    pub display_name: String,
    pub secret_in_character: bool,
    pub revealed_by_default_ui: bool,
    pub quest_log_autocomplete: bool,
    pub required_discovery_refs: Vec<String>,
    pub required_components: Vec<String>,
    pub required_focus_ids: Vec<String>,
    pub allowed_forms: Vec<UndeadForm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitheringRaidPathReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub raid_id: String,
    pub questline_id: String,
    pub elite_group_required: bool,
    pub minimum_party_size: u8,
    pub minimum_character_level: u16,
    pub pressure_tier_milli: u16,
    pub phase_count: usize,
    pub required_discovery_ref_count: usize,
    pub required_component_count: usize,
    pub allowed_form_count: usize,
    pub source_zone_id: String,
    pub source_zone_has_reagents: bool,
    pub final_guard_agent_id: String,
    pub target_faction_id: String,
    pub target_item_of_power_id: String,
    pub sample_withering_request_valid: bool,
    pub policy_sha256: String,
    pub undeath_policy_sha256: String,
    pub zone_policy_sha256: String,
    pub boss_policy_sha256: String,
    pub result_sha256: String,
}

fn phase(
    kind: WitheringRaidPhaseKind,
    phase_id: &str,
    display_name: &str,
    required_ref: &str,
    unlock_ref: &str,
    pressure_tier_milli: u16,
) -> WitheringRaidPhase {
    WitheringRaidPhase {
        kind,
        phase_id: phase_id.to_string(),
        display_name: display_name.to_string(),
        required_ref: required_ref.to_string(),
        unlock_ref: unlock_ref.to_string(),
        pressure_tier_milli,
        in_character_only: true,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        bypasses_withering_requirements: false,
    }
}
