use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    AttributeBuildInput, AttributeScores, DerivedStatFormulaTable, RPG_STATS_SCHEMA,
    RaceAttributeCaps, derive_character_stats, rpg_authored_payload_without_nulls,
    validate_rpg_authored_content_records,
};

const ATTRIBUTE_FORMULA_TABLE_ID: &str = "attributes.formula_table.mud_defaults";

pub fn attribute_content_records(
    table: &DerivedStatFormulaTable,
    race_caps: &[RaceAttributeCaps],
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let mut sorted_caps = race_caps.to_vec();
    sorted_caps.sort_by(|left, right| left.race_id.cmp(&right.race_id));
    validate_attribute_content_inputs(table, &sorted_caps)?;

    let table_sha256 = serialize_sha256(table)?;
    let mut records = vec![content_record(
        EngineContentKind::Lore,
        ATTRIBUTE_FORMULA_TABLE_ID,
        "Attribute Formula Table: MUD Defaults",
        "formulas",
        json!({
            "schema": RPG_STATS_SCHEMA,
            "source_issue": 960,
            "content_migration_issue": 1455,
            "surface_kind": "attribute_formula_table",
            "formula_table_id": ATTRIBUTE_FORMULA_TABLE_ID,
            "formula_table": table,
            "formula_count": 8,
            "resistance_formula_count": table.resistances.len(),
            "race_cap_record_count": sorted_caps.len(),
            "formula_table_sha256": table_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];

    for race_cap in &sorted_caps {
        records.push(race_cap_record(race_cap, &table_sha256)?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "attributes.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn validate_attribute_content_inputs(
    table: &DerivedStatFormulaTable,
    race_caps: &[RaceAttributeCaps],
) -> Result<(), EngineError> {
    if race_caps.is_empty() {
        return invalid("race_caps", "at least one race cap row is required");
    }
    for race_cap in race_caps {
        let input = AttributeBuildInput {
            character_id: format!("content_probe.{}", race_cap.race_id),
            race_id: race_cap.race_id.clone(),
            level: 1,
            base: AttributeScores::new(1, 1, 1, 1, 1, 1),
            race_caps: race_caps.to_vec(),
            modifiers: Vec::new(),
        };
        derive_character_stats(&input, table)?;
    }
    Ok(())
}

fn race_cap_record(
    race_cap: &RaceAttributeCaps,
    table_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Unit,
        &format!("attributes.race_caps.{}", race_cap.race_id),
        &format!("Attribute Race Caps: {}", race_cap.race_id),
        "race_caps",
        json!({
            "schema": RPG_STATS_SCHEMA,
            "source_issue": 960,
            "content_migration_issue": 1455,
            "surface_kind": "attribute_race_caps",
            "formula_table_id": ATTRIBUTE_FORMULA_TABLE_ID,
            "race_id": race_cap.race_id,
            "caps": race_cap.caps,
            "race_caps_sha256": serialize_sha256(race_cap)?,
            "formula_table_sha256": table_sha256,
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
    collection: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/attributes/{collection}/{id}"),
            author: "editor_authoring.issue1455.attributes".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
