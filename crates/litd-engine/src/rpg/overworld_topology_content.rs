use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    OverworldTopologySpec, RPG_OVERWORLD_TOPOLOGY_SCHEMA, overworld_topology_readback,
    rpg_authored_payload_without_nulls, serialize_sha256, validate_overworld_topology,
    validate_rpg_authored_content_records,
};

/// Emits one topology-summary record, one record per region, and one record per
/// edge, mirroring the AreaSpec emitter (summary + per-row split so embedded
/// arrays stay small for the lowering valve's sorted field-path invariant). Every
/// payload carries the acquired-map doctrine flags (#944) as explicit `false`:
/// the overworld graph is authoritative data for the WORLD, never a free in-game
/// minimap handed to the player.
pub fn overworld_topology_content_records(
    spec: &OverworldTopologySpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_overworld_topology(spec)?;
    let readback = overworld_topology_readback(spec)?;
    let mut records = Vec::with_capacity(1 + spec.regions.len() + spec.edges.len());

    records.push(content_record(
        &spec.world_id,
        &format!("Overworld Topology: {}", spec.display_name),
        json!({
            "schema": RPG_OVERWORLD_TOPOLOGY_SCHEMA,
            "source_issue": spec.source_issue,
            "surface_kind": "overworld_topology",
            "world_id": spec.world_id,
            "display_name": spec.display_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "summary": spec.summary,
            "region_count": readback.region_count,
            "edge_count": readback.edge_count,
            "member_area_count": readback.member_area_count,
            "is_connected": readback.is_connected,
            "implementation_notes": spec.implementation_notes,
            "topology_sha256": readback.topology_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for region in &spec.regions {
        records.push(content_record(
            &region.region_id,
            &region.display_name,
            json!({
                "schema": RPG_OVERWORLD_TOPOLOGY_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "overworld_region",
                "world_id": spec.world_id,
                "region_id": region.region_id,
                "display_name": region.display_name,
                "sector": region.sector,
                "summary": region.summary,
                "level_band": region.level_band,
                "member_area_ids": region.member_area_ids,
                "member_area_count": region.member_area_ids.len(),
                "topology_sha256": readback.topology_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }

    for edge in &spec.edges {
        records.push(content_record(
            &edge.edge_id,
            &format!("Spine: {}", edge.edge_id),
            json!({
                "schema": RPG_OVERWORLD_TOPOLOGY_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "overworld_edge",
                "world_id": spec.world_id,
                "edge_id": edge.edge_id,
                "from_region_id": edge.from_region_id,
                "to_region_id": edge.to_region_id,
                "spine": edge.spine,
                "summary": edge.summary,
                "topology_sha256": readback.topology_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "overworld_topology.authored_content".to_string(),
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
            path: format!("editor://rpg/thera/overworld_topology/{id}"),
            author: "editor_authoring.issue1632.overworld_topology".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
