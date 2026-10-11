use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalZonePolicy,
    RPG_WITHERING_RAID_PATH_SCHEMA, UndeathPolicy, WitheringRaidPathPolicy,
    rpg_authored_payload_without_nulls, validate_rpg_authored_content_records,
    validate_withering_raid_path_policy,
};

pub fn withering_raid_content_records(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    attrition: &InfernalAttritionPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_withering_raid_path_policy(policy, undeath, zones, bosses, attrition)?;
    let mut records = vec![
        content_record(
            EngineContentKind::Story,
            &policy.questline.questline_id,
            &policy.questline.display_name,
            json!({
                "schema": RPG_WITHERING_RAID_PATH_SCHEMA,
                "source_issue": 1064,
                "content_migration_issue": 1455,
                "surface_kind": "withering_questline",
                "questline": &policy.questline,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Trigger,
            &policy.raid.raid_id,
            &policy.raid.display_name,
            json!({
                "schema": RPG_WITHERING_RAID_PATH_SCHEMA,
                "source_issue": 1064,
                "content_migration_issue": 1455,
                "surface_kind": "withering_raid",
                "raid": &policy.raid,
                "requires_faction_leader_order": policy.raid.faction_leader_order_required,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Item,
            &policy.raid.target_item_of_power_id,
            "Rift-Shard",
            json!({
                "schema": RPG_WITHERING_RAID_PATH_SCHEMA,
                "source_issue": 1064,
                "content_migration_issue": 1455,
                "surface_kind": "withering_item_of_power",
                "target_faction_id": &policy.raid.target_faction_id,
                "item_of_power": true,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
                "bypasses_withering_requirements": false,
            }),
        )?,
    ];
    for phase in &policy.raid.phases {
        records.push(content_record(
            EngineContentKind::Trigger,
            &phase.phase_id,
            &phase.display_name,
            json!({
                "schema": RPG_WITHERING_RAID_PATH_SCHEMA,
                "source_issue": 1064,
                "content_migration_issue": 1455,
                "surface_kind": "withering_raid_phase",
                "raid_id": &policy.raid.raid_id,
                "phase": phase,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "withering_raid_path.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/withering_raid_path/{id}"),
            author: "editor_authoring.issue1455.withering_raid_path".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
