use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    HallMemorialReadback, HallOfRememberedPolicy, RPG_HALL_OF_REMEMBERED_SCHEMA,
    rpg_authored_payload_without_nulls, validate_hall_memorial_readback,
    validate_rpg_authored_content_records,
};

pub fn hall_memorial_content_records(
    policy: &HallOfRememberedPolicy,
    readback: &HallMemorialReadback,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_hall_memorial_readback(policy, readback)?;
    let records = vec![
        content_record(
            EngineContentKind::Story,
            &readback.memorial_id,
            &format!("Memorial: {}", readback.character_name),
            json!({
                "schema": RPG_HALL_OF_REMEMBERED_SCHEMA,
                "source_issue": 1065,
                "content_migration_issue": 1455,
                "surface_kind": "hall_memorial_record",
                "record": &readback.record,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Lore,
            &format!(
                "surface.galdmere.hall.{}.{}",
                readback.character_id, readback.record.total_deaths
            ),
            &format!("Galdmere Hall Entry: {}", readback.character_name),
            json!({
                "schema": RPG_HALL_OF_REMEMBERED_SCHEMA,
                "source_issue": 1065,
                "content_migration_issue": 1455,
                "surface_kind": "galdmere_mourning_wall_browser",
                "surface_id": policy.galdmere_surface_id,
                "memorial_id": readback.memorial_id,
                "mourning_wall_entry_id": readback.mourning_wall_entry_id,
                "browsable": true,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Lore,
            &format!(
                "surface.online.hall.{}.{}",
                readback.character_id, readback.record.total_deaths
            ),
            &format!("Online Hall Entry: {}", readback.character_name),
            json!({
                "schema": RPG_HALL_OF_REMEMBERED_SCHEMA,
                "source_issue": 1065,
                "content_migration_issue": 1455,
                "surface_kind": "online_hall_browser",
                "surface_id": policy.online_surface_id,
                "memorial_id": readback.memorial_id,
                "tribute_count": readback.tribute_count,
                "browsable": true,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "hall_of_remembered.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/hall_of_remembered/{id}"),
            author: "editor_authoring.issue1455.hall_of_remembered".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
