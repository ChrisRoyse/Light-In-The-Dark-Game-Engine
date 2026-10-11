//! Editor-authored Calyx content records for the area reset+repop table (#1668).
//! One summary row (with the by-policy histogram and the CF-documented cadence)
//! plus one row per profiled area; every record routes through the shared
//! authored-content validator before persistence. Provenance is editor-authored
//! (never a compiled engine URI). Reset/repop is timer-driven, so records carry the
//! `Trigger` content kind.

use serde_json::{Value, json};

use super::area_reset_repop::area_reset_repop_readback;
use super::area_reset_repop_types::{
    AreaResetRepopProfile, AreaResetRepopTable, RPG_AREA_RESET_REPOP_SCHEMA,
};
use super::serialize_sha256;
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

pub fn area_reset_repop_content_records(
    table: &AreaResetRepopTable,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = area_reset_repop_readback(table)?;
    let mut records = Vec::with_capacity(table.entries.len() + 1);
    records.push(content_record(
        &format!("{}.summary", table.table_id),
        "Area Reset + Repop: Thera",
        json!({
            "schema": RPG_AREA_RESET_REPOP_SCHEMA,
            "source_issue": 1668,
            "surface_kind": "area_reset_repop_table",
            "table_id": table.table_id,
            "area_count": readback.area_count,
            "policy_histogram": readback.policy_histogram,
            "boss_timer_count": readback.boss_timer_count,
            "rearms_doors_locks_count": readback.rearms_doors_locks_count,
            "rearms_traps_count": readback.rearms_traps_count,
            "cf_documented_policy_count": readback.cf_documented_policy_count,
            "derived_from_kind_policy_count": readback.derived_from_kind_policy_count,
            "min_empty_reset_area_minutes": readback.min_empty_reset_area_minutes,
            "max_occupied_reset_area_minutes": readback.max_occupied_reset_area_minutes,
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
            field: "area_reset_repop.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn entry_record(
    table: &AreaResetRepopTable,
    entry: &AreaResetRepopProfile,
    table_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("{}.{}", table.table_id, entry.area_id),
        &format!("Reset+Repop: {}", entry.area_id),
        json!({
            "schema": RPG_AREA_RESET_REPOP_SCHEMA,
            "source_issue": 1668,
            "surface_kind": "area_reset_repop",
            "table_id": table.table_id,
            "area_id": entry.area_id,
            "repop_policy": entry.repop_policy,
            "empty_reset_area_minutes": entry.empty_reset_area_minutes,
            "occupied_reset_area_minutes": entry.occupied_reset_area_minutes,
            "regen_objects_only_when_empty": entry.regen_objects_only_when_empty,
            "capped_items_respawn_while_held": entry.capped_items_respawn_while_held,
            "rearms_doors_locks": entry.rearms_doors_locks,
            "rearms_traps": entry.rearms_traps,
            "has_boss_timer": entry.has_boss_timer,
            "population_density_tier": entry.population_density_tier,
            "cadence_derivation": entry.cadence_derivation,
            "policy_derivation": entry.policy_derivation,
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
        kind: EngineContentKind::Trigger,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/thera/area_reset_repop/{id}"),
            author: "editor_authoring.issue1668.area_reset_repop".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
