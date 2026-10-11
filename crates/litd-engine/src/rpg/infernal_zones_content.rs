use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAttritionPolicy, InfernalZonePolicy, InfernalZoneTopologyReadback,
    RPG_INFERNAL_ZONES_SCHEMA, derive_infernal_zone_topology, infernal_zone_policy_hash,
    rpg_authored_payload_without_nulls, validate_infernal_zone_policy,
    validate_rpg_authored_content_records,
};

pub fn infernal_zone_content_records(
    policy: &InfernalZonePolicy,
    attrition: &InfernalAttritionPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_zone_policy(policy, attrition)?;
    let topology = derive_infernal_zone_topology(policy, attrition)?;
    let policy_sha256 = infernal_zone_policy_hash(policy)?;
    let mut records = Vec::with_capacity(8);
    records.push(topology_record(policy, &topology, &policy_sha256)?);
    for step in &topology.chain {
        let zone = policy
            .zones
            .iter()
            .find(|zone| zone.zone_id == step.zone_id)
            .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
                field: "infernal_zones.authored_content.zone_id".to_string(),
                reason: format!("missing zone {}", step.zone_id),
            })?;
        records.push(content_record(
            EngineContentKind::Terrain,
            &zone.zone_id,
            &zone.display_name,
            json!({
                "schema": RPG_INFERNAL_ZONES_SCHEMA,
                "source_issue": 1067,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_zone_node",
                "circle_id": zone.circle_id,
                "zone_id": zone.zone_id,
                "display_name": zone.display_name,
                "depth": zone.depth,
                "hazard_profile_id": zone.hazard_profile_id,
                "hazard_kind": zone.hazard_kind,
                "attrition_health_damage_per_tick": step.attrition_health_damage_per_tick,
                "attrition_constitution_strain_per_tick": step.attrition_constitution_strain_per_tick,
                "mob_population_ids": zone.mob_population_ids,
                "ruler_ref": zone.ruler_ref,
                "reset_profile": zone.reset_profile,
                "repop_profile": zone.repop_profile,
                "exits": zone.exits,
                "descends_to": step.descends_to,
                "withering_reagent_source": zone.withering_reagent_source,
                "relic_vault_source": zone.relic_vault_source,
                "policy_sha256": policy_sha256,
                "topology_result_sha256": topology.result_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_zones.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn topology_record(
    policy: &InfernalZonePolicy,
    topology: &InfernalZoneTopologyReadback,
    policy_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Terrain,
        &policy.server_policy_id,
        "Infernal Deeps Zone Topology",
        json!({
            "schema": RPG_INFERNAL_ZONES_SCHEMA,
            "source_issue": 1067,
            "content_migration_issue": 1455,
            "surface_kind": "infernal_zone_topology_summary",
            "zone_count": topology.zone_count,
            "entry_circle": topology.entry_circle,
            "terminal_circle": topology.terminal_circle,
            "wailing_tombs_reagent_source": topology.wailing_tombs_reagent_source,
            "throne_road_relic_vault_source": topology.throne_road_relic_vault_source,
            "policy_sha256": policy_sha256,
            "attrition_policy_sha256": topology.attrition_policy_sha256,
            "result_sha256": topology.result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
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
            path: format!("editor://rpg/ashen_reach/infernal_zones/{id}"),
            author: "editor_authoring.issue1455.infernal_zones".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
