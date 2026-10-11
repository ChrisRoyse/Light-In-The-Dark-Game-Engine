//! Editor-authored Calyx content records for the stealth acquisition matrix
//! (#1623). One summary row plus one row per (ability x grantor) cell; every
//! record routes through the shared authored-content validator.

use serde_json::{Value, json};

use super::serialize_sha256;
use super::stealth_acquisition_types::{
    RPG_STEALTH_ACQUISITION_SCHEMA, StealthAcquisitionEntry, StealthAcquisitionMatrix,
};
use super::stealth_acquisition_validate::validate_stealth_acquisition_matrix;
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

pub fn stealth_acquisition_content_records(
    matrix: &StealthAcquisitionMatrix,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = validate_stealth_acquisition_matrix(matrix)?;
    let mut records = Vec::with_capacity(matrix.entries.len() + 1);
    records.push(content_record(
        &format!("{}.summary", matrix.table_id),
        "Stealth Acquisition Matrix: Ashen Reach",
        json!({
            "schema": RPG_STEALTH_ACQUISITION_SCHEMA,
            "source_issue": 1623,
            "content_migration_issue": 1455,
            "surface_kind": "stealth_acquisition_table",
            "table_id": matrix.table_id,
            "entry_count": readback.entry_count,
            "class_entry_count": readback.class_entry_count,
            "race_entry_count": readback.race_entry_count,
            "acquisition_counts": readback.acquisition_counts,
            "distinct_ability_count": readback.distinct_ability_count,
            "distinct_grantor_count": readback.distinct_grantor_count,
            "provisional_entry_count": readback.provisional_entry_count,
            "provisional_grantors": readback.provisional_grantors,
            "abilities_validated": readback.abilities_validated,
            "grantors_validated": readback.grantors_validated,
            "referenced_ability_ids": readback.referenced_ability_ids,
            "owners_per_ability": readback.owners_per_ability,
            "matrix_sha256": readback.matrix_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for entry in &matrix.entries {
        records.push(entry_record(matrix, entry, &readback.matrix_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgAbilityInvalidConfig {
            field: "stealth_acquisition.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn entry_record(
    matrix: &StealthAcquisitionMatrix,
    entry: &StealthAcquisitionEntry,
    matrix_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    let row_id = format!(
        "{}.{}.{}",
        matrix.table_id, entry.ability_id, entry.grantor_id
    );
    content_record(
        &row_id,
        &format!("{} <- {}", entry.ability_id, entry.grantor_id),
        json!({
            "schema": RPG_STEALTH_ACQUISITION_SCHEMA,
            "source_issue": 1623,
            "content_migration_issue": 1455,
            "surface_kind": "stealth_acquisition_entry",
            "table_id": matrix.table_id,
            "ability_id": entry.ability_id,
            "grantor_kind": entry.grantor_kind,
            "grantor_id": entry.grantor_id,
            "level": entry.level,
            "acquisition": entry.acquisition,
            "path_id": entry.path_id,
            "point_cost": entry.point_cost,
            "terrain_profile": entry.terrain_profile,
            "provisional": entry.provisional,
            "source_note": entry.source_note,
            "entry_sha256": serialize_sha256(entry)?,
            "matrix_sha256": matrix_sha256,
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
            path: format!("editor://rpg/ashen_reach/stealth_acquisition/{id}"),
            author: "editor_authoring.issue1623.stealth_acquisition".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
