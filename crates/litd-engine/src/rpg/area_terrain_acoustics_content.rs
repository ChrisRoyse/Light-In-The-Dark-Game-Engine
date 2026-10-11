//! Editor-authored Calyx content records for the area terrain+acoustics table
//! (#1667). One summary row (with the by-terrain histogram) plus one row per
//! tagged area; every record routes through the shared authored-content validator
//! before persistence. Provenance is editor-authored (never a compiled engine URI).

use serde_json::{Value, json};

use super::area_terrain_acoustics::area_terrain_acoustics_readback;
use super::area_terrain_acoustics_types::{
    AreaTerrainAcoustics, AreaTerrainAcousticsTable, RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA,
};
use super::serialize_sha256;
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

pub fn area_terrain_acoustics_content_records(
    table: &AreaTerrainAcousticsTable,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = area_terrain_acoustics_readback(table)?;
    let mut records = Vec::with_capacity(table.entries.len() + 1);
    records.push(content_record(
        &format!("{}.summary", table.table_id),
        "Area Terrain + Acoustics: Thera",
        json!({
            "schema": RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA,
            "source_issue": 1667,
            "surface_kind": "area_terrain_acoustics_table",
            "table_id": table.table_id,
            "area_count": readback.area_count,
            "terrain_histogram": readback.terrain_histogram,
            "light_histogram": readback.light_histogram,
            "surface_histogram": readback.surface_histogram,
            "reverb_histogram": readback.reverb_histogram,
            "hide_legal_count": readback.hide_legal_count,
            "exposed_count": readback.exposed_count,
            "no_sneak_interior_count": readback.no_sneak_interior_count,
            "cf_documented_count": readback.cf_documented_count,
            "derived_from_kind_count": readback.derived_from_kind_count,
            "table_sha256": readback.table_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for entry in &table.entries {
        records.push(entry_record(table, entry, &readback.table_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "area_terrain_acoustics.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn entry_record(
    table: &AreaTerrainAcousticsTable,
    entry: &AreaTerrainAcoustics,
    table_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("{}.{}", table.table_id, entry.area_id),
        &format!("Terrain+Acoustics: {}", entry.area_id),
        json!({
            "schema": RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA,
            "source_issue": 1667,
            "surface_kind": "area_terrain_acoustics",
            "table_id": table.table_id,
            "area_id": entry.area_id,
            "primary_terrain": entry.primary_terrain,
            "secondary_terrain": entry.secondary_terrain,
            "light_level": entry.light_level,
            "surface_material": entry.surface_material,
            "reverb": entry.reverb,
            "occlusion": entry.occlusion,
            "hide_legal": entry.hide_legal,
            "exposed": entry.exposed,
            "no_sneak_interior": entry.no_sneak_interior,
            "derivation": entry.derivation,
            "note": entry.note,
            "entry_sha256": serialize_sha256(entry)?,
            "table_sha256": table_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
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
            path: format!("editor://rpg/thera/area_terrain_acoustics/{id}"),
            author: "editor_authoring.issue1667.area_terrain_acoustics".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
