use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalDeepsPackagePolicy, InfernalDeepsPackageReadback, RPG_INFERNAL_DEEPS_SCHEMA,
    rpg_authored_payload_without_nulls, validate_infernal_deeps_readback,
    validate_rpg_authored_content_records,
};

pub fn infernal_deeps_content_records(
    policy: &InfernalDeepsPackagePolicy,
    readback: &InfernalDeepsPackageReadback,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_deeps_readback(policy, readback)?;
    let records = vec![
        content_record(
            EngineContentKind::Lore,
            &policy.package_id,
            &policy.display_name,
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1063,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_parent_summary",
                "ordered_hell_not_outer_dark": readback.ordered_hell_not_outer_dark,
                "group_required": readback.group_required,
                "no_recall_inside": readback.no_recall_inside,
                "entry_circle": readback.entry_circle,
                "terminal_circle": readback.terminal_circle,
                "child_hashes": readback.child_hashes,
                "policy_sha256": readback.policy_sha256,
                "result_sha256": readback.result_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Trigger,
            "server.ashen_reach.infernal_deeps.access_contract",
            "Infernal Deeps Access Contract",
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1066,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_access_contract",
                "entry_circle": readback.entry_circle,
                "access_path_count": readback.access_path_count,
                "denied_escape_kind_count": readback.denied_escape_kind_count,
                "no_recall_inside": readback.no_recall_inside,
                "source_hash": readback.child_hashes.access_policy_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Terrain,
            "server.ashen_reach.infernal_deeps.zone_topology",
            "Infernal Deeps Zone Topology",
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1067,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_zone_topology",
                "zone_count": readback.zone_count,
                "hazard_kind_count": readback.hazard_kind_count,
                "entry_circle": readback.entry_circle,
                "terminal_circle": readback.terminal_circle,
                "wailing_tombs_reagent_source": readback.wailing_tombs_reagent_source,
                "throne_road_relic_vault_source": readback.throne_road_relic_vault_source,
                "source_hash": readback.child_hashes.zone_topology_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Agent,
            "server.ashen_reach.infernal_deeps.boss_hierarchy",
            "Infernal Deeps Boss Hierarchy",
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1068,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_boss_hierarchy",
                "boss_count": readback.boss_count,
                "apex_agent_id": readback.apex_agent_id,
                "source_hash": readback.child_hashes.boss_hierarchy_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Item,
            "server.ashen_reach.infernal_deeps.relic_vaults",
            "Infernal Deeps Relic Vaults",
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1069,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_relic_vaults",
                "relic_vault_count": readback.relic_vault_count,
                "hellforged_item_count": readback.hellforged_item_count,
                "world_finite_item_total": readback.world_finite_item_total,
                "highest_power_rating_milli": readback.highest_power_rating_milli,
                "source_hash": readback.child_hashes.relic_vaults_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Story,
            "server.ashen_reach.infernal_deeps.withering_raid_path",
            "Infernal Deeps Withering Raid Path",
            json!({
                "schema": RPG_INFERNAL_DEEPS_SCHEMA,
                "source_issue": 1064,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_deeps_withering_raid_path",
                "withering_raid_phase_count": readback.withering_raid_phase_count,
                "withering_minimum_party_size": readback.withering_minimum_party_size,
                "withering_minimum_character_level": readback.withering_minimum_character_level,
                "source_hash": readback.child_hashes.withering_raid_path_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_deeps.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/infernal_deeps/{id}"),
            author: "editor_authoring.issue1455.infernal_deeps".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
