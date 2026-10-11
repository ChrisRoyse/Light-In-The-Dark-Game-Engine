use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    RPG_RACE_SPEC_SCHEMA, RaceSpec, race_spec_readback, rpg_authored_payload_without_nulls,
    validate_race_spec, validate_rpg_authored_content_records,
};

pub fn race_spec_content_records(spec: &RaceSpec) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_race_spec(spec)?;
    let readback = race_spec_readback(spec)?;
    let records = vec![content_record(
        EngineContentKind::Unit,
        &spec.race_id,
        &format!("Race Spec: {}", spec.display_name),
        json!({
            "schema": RPG_RACE_SPEC_SCHEMA,
            "source_issue": spec.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "race_spec",
            "race_id": spec.race_id,
            "display_name": spec.display_name,
            "player_facing_canon_name": spec.player_facing_canon_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "source_history_label": spec.source_history_label,
            "culture_summary": spec.culture_summary,
            "stat_caps": spec.stat_caps,
            "alignment": spec.alignment,
            "ethos": spec.ethos,
            "size": spec.size,
            "resistances": spec.resistances,
            "vulnerabilities": spec.vulnerabilities,
            "innate_abilities": spec.innate_abilities,
            "vision": spec.vision,
            "class_access": spec.class_access,
            "experience_penalty_percent": spec.experience_penalty_percent,
            "hometowns": spec.hometowns,
            "visual": spec.visual,
            "implementation_notes": spec.implementation_notes,
            "race_spec_sha256": readback.race_spec_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "race_specs.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/race_specs/{id}"),
            author: "editor_authoring.issue1455.race_specs".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
