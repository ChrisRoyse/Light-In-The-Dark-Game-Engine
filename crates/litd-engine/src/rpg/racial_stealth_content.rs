//! Editor-authored Calyx content records for the racial stealth profile table
//! (#1603). One summary row plus one row per race; every record routes through
//! the shared authored-content validator before persistence.

use serde_json::{Value, json};

use super::racial_stealth_types::{
    RPG_RACIAL_STEALTH_SCHEMA, RacialStealthProfile, RacialStealthProfileTable,
};
use super::racial_stealth_validate::validate_racial_stealth_table;
use super::serialize_sha256;
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

pub fn racial_stealth_content_records(
    table: &RacialStealthProfileTable,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = validate_racial_stealth_table(table)?;
    let mut records = Vec::with_capacity(table.profiles.len() + 1);
    records.push(content_record(
        &format!("{}.summary", table.table_id),
        "Racial Stealth Profiles: Ashen Reach",
        json!({
            "schema": RPG_RACIAL_STEALTH_SCHEMA,
            "source_issue": 1603,
            "content_migration_issue": 1455,
            "surface_kind": "racial_stealth_table",
            "table_id": table.table_id,
            "profile_count": readback.profile_count,
            "noise_class_counts": readback.noise_class_counts,
            "trait_role_counts": readback.trait_role_counts,
            "innate_trait_total": readback.innate_trait_total,
            "races_with_innate_stealth": readback.races_with_innate_stealth,
            "creep_penalized_races": readback.creep_penalized_races,
            "referenced_ability_ids": readback.referenced_ability_ids,
            "race_ids": readback.race_ids,
            "table_sha256": readback.table_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);

    for profile in &table.profiles {
        records.push(profile_record(table, profile, &readback.table_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "racial_stealth.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn profile_record(
    table: &RacialStealthProfileTable,
    profile: &RacialStealthProfile,
    table_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        &format!("{}.{}", table.table_id, profile.race_id),
        &format!("Racial Stealth: {}", profile.display_name),
        json!({
            "schema": RPG_RACIAL_STEALTH_SCHEMA,
            "source_issue": profile.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "racial_stealth_profile",
            "table_id": table.table_id,
            "race_id": profile.race_id,
            "display_name": profile.display_name,
            "mechanics_source_url": profile.mechanics_source_url,
            "size": profile.size,
            "movement_noise": profile.movement_noise,
            "footstep_radius_scale_percent": profile.footstep_radius_scale_percent,
            "creep_size_penalty": profile.creep_size_penalty,
            "innate_traits": profile.innate_traits,
            "notes": profile.notes,
            "profile_sha256": serialize_sha256(profile)?,
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
            path: format!("editor://rpg/ashen_reach/racial_stealth/{id}"),
            author: "editor_authoring.issue1603.racial_stealth".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
