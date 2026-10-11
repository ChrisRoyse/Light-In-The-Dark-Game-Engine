//! Editor-authored Calyx content emitter for the limited-item model (#1686).
//!
//! Mirrors `item_specs_content.rs`: emits an `EngineContentRecord` with an
//! editor-authored provenance (`editor://rpg/ashen_reach/limited_items/{id}`)
//! and routes it through the shared `validate_rpg_authored_content_records`
//! gate. Limited records persist under `EngineContentKind::Item` with
//! `surface_kind = "limited_item_spec"`. No compiled engine-scheme literals, no
//! self-attesting anchors -- the side-store gate rejects both.

use serde_json::{Value, json};

use super::serialize_sha256;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::item_limited::{LimitedItemSpec, RPG_LIMITED_ITEM_SPEC_SCHEMA, limited_item_readback};
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

pub fn limited_item_content_records(
    spec: &LimitedItemSpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = limited_item_readback(spec)?;
    let records = vec![content_record(
        &spec.limited_id,
        &format!("Limited Item: {}", spec.display_name),
        json!({
            "schema": RPG_LIMITED_ITEM_SPEC_SCHEMA,
            "source_issue": spec.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "limited_item_spec",
            "limited_id": spec.limited_id,
            "item_id": spec.item_id,
            "display_name": spec.display_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "world_cap": spec.world_cap,
            "may_enter_container": spec.may_enter_container,
            "keep_level_floor": spec.keep_level_floor,
            "crumble": spec.crumble,
            "illusion": spec.illusion,
            "hoarding_is_violation": spec.hoarding_is_violation,
            "runtime_enforcement_notes": spec.runtime_enforcement_notes,
            "implementation_notes": spec.implementation_notes,
            "limited_item_sha256": readback.limited_item_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "limited_items.authored_content".to_string(),
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
        kind: EngineContentKind::Item,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/limited_items/{id}"),
            author: "editor_authoring.issue1686.limited_items".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
