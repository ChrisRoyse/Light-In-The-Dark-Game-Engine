use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    AreaSpec, RPG_AREA_SPEC_SCHEMA, area_spec_readback, rpg_authored_payload_without_nulls,
    validate_area_spec, validate_rpg_authored_content_records,
};

/// Emits one area-summary record, one record per room, and one record per
/// connection, mirroring the infernal zone emitter (topology summary + per-zone
/// rows). Rooms and connections are separate rows rather than arrays embedded in
/// the summary: the lowering valve flattens arrays into positional field paths, and
/// an 11+ element array would sort index "10" before "2" and break the strictly
/// sorted field-path invariant. Splitting keeps every embedded array small. Every
/// payload carries the acquired-map doctrine flags (#944) as explicit `false`, so no
/// lowered area can grant a free minimap/GPS.
pub fn area_spec_content_records(spec: &AreaSpec) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_area_spec(spec)?;
    let readback = area_spec_readback(spec)?;
    let mut records = Vec::with_capacity(1 + spec.rooms.len() + spec.connections.len());
    records.push(content_record(
        spec,
        &spec.area_id,
        &format!("Area Spec: {}", spec.display_name),
        json!({
            "schema": RPG_AREA_SPEC_SCHEMA,
            "source_issue": spec.source_issue,
            "surface_kind": "area_spec",
            "area_id": spec.area_id,
            "display_name": spec.display_name,
            "player_facing_canon_name": spec.player_facing_canon_name,
            "area_kind": spec.area_kind,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "source_history_label": spec.source_history_label,
            "summary": spec.summary,
            "level_band": spec.level_band,
            "resets": spec.resets,
            "repop": spec.repop,
            "recall": spec.recall,
            "visual": spec.visual,
            "variant_of_area_id": spec.variant_of_area_id,
            "variant_label": spec.variant_label,
            "implementation_notes": spec.implementation_notes,
            "room_count": readback.room_count,
            "connection_count": readback.connection_count,
            "external_connection_count": readback.external_connection_count,
            "hidden_connection_count": readback.hidden_connection_count,
            "area_spec_sha256": readback.area_spec_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);
    for room in &spec.rooms {
        records.push(content_record(
            spec,
            &room.room_id,
            &room.display_name,
            json!({
                "schema": RPG_AREA_SPEC_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "area_room",
                "area_id": spec.area_id,
                "room_id": room.room_id,
                "display_name": room.display_name,
                "room_kind": room.room_kind,
                "summary": room.summary,
                "concealment": room.concealment,
                "npc_population_ids": room.npc_population_ids,
                "area_spec_sha256": readback.area_spec_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    for conn in &spec.connections {
        records.push(content_record(
            spec,
            &conn.connection_id,
            &format!("Connection: {}", conn.connection_id),
            json!({
                "schema": RPG_AREA_SPEC_SCHEMA,
                "source_issue": spec.source_issue,
                "surface_kind": "area_connection",
                "area_id": spec.area_id,
                "connection_id": conn.connection_id,
                "from_room_id": conn.from_room_id,
                "to_area_id": conn.to_area_id,
                "to_room_id": conn.to_room_id,
                "direction": conn.direction,
                "travel_kind": conn.travel_kind,
                "discovery": conn.discovery,
                "locked_by_ref": conn.locked_by_ref,
                "is_external": conn.to_area_id != spec.area_id,
                "area_spec_sha256": readback.area_spec_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "area_specs.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    spec: &AreaSpec,
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
            path: format!("editor://rpg/thera/area_specs/{}/{id}", spec.area_id),
            author: "editor_authoring.issue987.area_specs".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
