use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    RPG_ROAD_NETWORK_SCHEMA, RoadNetworkSpec, road_network_readback,
    rpg_authored_payload_without_nulls, serialize_sha256, validate_road_network,
    validate_rpg_authored_content_records,
};

/// Emits one network-summary record, one record per named road, and one per
/// crossroad. Gates ride inside their road payload (small bounded arrays). Every
/// payload carries the acquired-map doctrine flags (#944) as explicit `false`:
/// the road spine is world data, never a free minimap. Roads are also
/// cartographer-sellable (helpfile 2924) as acquired items, not GPS.
pub fn road_network_content_records(
    spec: &RoadNetworkSpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_road_network(spec)?;
    let readback = road_network_readback(spec)?;
    let mut records = Vec::with_capacity(1 + spec.roads.len() + spec.crossroads.len());

    records.push(content_record(
        &spec.world_id,
        &format!("Road Network: {}", spec.display_name),
        json!({
            "schema": RPG_ROAD_NETWORK_SCHEMA,
            "source_issue": spec.source_issue,
            "surface_kind": "road_network",
            "world_id": spec.world_id,
            "display_name": spec.display_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "summary": spec.summary,
            "road_count": readback.road_count,
            "segment_count": readback.segment_count,
            "gate_count": readback.gate_count,
            "crossroad_count": readback.crossroad_count,
            "implementation_notes": spec.implementation_notes,
            "road_network_sha256": readback.road_network_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for road in &spec.roads {
        records.push(content_record(
            &road.road_id,
            &road.display_name,
            json!({
                "schema": RPG_ROAD_NETWORK_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "named_road",
                "world_id": spec.world_id,
                "road_id": road.road_id,
                "display_name": road.display_name,
                "road_kind": road.road_kind,
                "exposure": road.exposure,
                "summary": road.summary,
                "waypoint_area_ids": road.waypoint_area_ids,
                "gates": road.gates,
                "segment_count": road.waypoint_area_ids.len().saturating_sub(1),
                "road_network_sha256": readback.road_network_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }

    for crossroad in &spec.crossroads {
        records.push(content_record(
            &crossroad.crossroad_id,
            &crossroad.display_name,
            json!({
                "schema": RPG_ROAD_NETWORK_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "crossroad",
                "world_id": spec.world_id,
                "crossroad_id": crossroad.crossroad_id,
                "display_name": crossroad.display_name,
                "at_area_id": crossroad.at_area_id,
                "road_ids": crossroad.road_ids,
                "summary": crossroad.summary,
                "road_network_sha256": readback.road_network_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "road_network.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind: EngineContentKind::Terrain,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/thera/road_network/{id}"),
            author: "editor_authoring.issue1633.road_network".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
