use super::serialize_sha256;
use serde_json::json;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    ImmortalCoembodimentPolicy, ImmortalCoembodimentReadback, RPG_IMMORTAL_COEMBODIMENT_SCHEMA,
    rpg_authored_payload_without_nulls, validate_immortal_coembodiment_readback,
    validate_rpg_authored_content_records,
};

pub fn immortal_coembodiment_content_records(
    policy: &ImmortalCoembodimentPolicy,
    readback: &ImmortalCoembodimentReadback,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_immortal_coembodiment_readback(policy, readback)?;
    let payload = rpg_authored_payload_without_nulls(json!({
        "schema": RPG_IMMORTAL_COEMBODIMENT_SCHEMA,
        "source_issue": 1046,
        "content_migration_issue": 1455,
        "surface_kind": "immortal_coembodiment_session",
        "readback": readback,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    }));
    let record = EngineContentRecord {
        kind: EngineContentKind::Agent,
        id: readback.session_id.clone(),
        display_name: format!("Immortal Co-Embodiment: {}", readback.public_identity),
        source: EngineContentSource {
            path: format!(
                "editor://rpg/ashen_reach/immortal_coembodiment/{}",
                readback.session_id
            ),
            author: "editor_authoring.issue1455.immortal_coembodiment".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    };
    validate_rpg_authored_content_records(std::slice::from_ref(&record)).map_err(|error| {
        EngineError::RpgImmortalInvalidConfig {
            field: "immortal_coembodiment.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(vec![record])
}
