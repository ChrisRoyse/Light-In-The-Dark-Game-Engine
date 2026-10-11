//! Editor-authored Calyx content emitter for the item data model (#1682).
//!
//! Mirrors `race_specs_content.rs`: emits an `EngineContentRecord` with an
//! editor-authored provenance (`editor://rpg/ashen_reach/item_specs/{id}`) and
//! routes it through the shared `validate_rpg_authored_content_records` gate.
//! Items persist under `EngineContentKind::Item`. No compiled engine-scheme
//! source literals, no self-attesting anchors -- the side-store gate rejects
//! both; provenance is the editor-authoring path only.

use serde_json::{Value, json};

use super::serialize_sha256;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::item_specs::item_spec_readback;
use super::item_specs_types::{ItemSpec, RPG_ITEM_SPEC_SCHEMA};
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

pub fn item_spec_content_records(spec: &ItemSpec) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = item_spec_readback(spec)?;
    let records = vec![content_record(
        &spec.item_id,
        &format!("Item Spec: {}", spec.display_name),
        json!({
            "schema": RPG_ITEM_SPEC_SCHEMA,
            "source_issue": spec.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "item_spec",
            "item_id": spec.item_id,
            "display_name": spec.display_name,
            "keywords": spec.keywords,
            "source_area_id": spec.source_area_id,
            "source_area_name": spec.source_area_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "kind_payload": spec.kind_payload,
            "kind": spec.kind_payload.kind().as_id(),
            "wear": spec.wear,
            "level": spec.level,
            "weight": spec.weight,
            "value_coin": spec.value_coin,
            "material": spec.material,
            "flags": spec.flags,
            "armor": spec.armor,
            "weapon": spec.weapon,
            "affects": spec.affects,
            "condition": spec.condition,
            "restring": spec.restring,
            "identify": spec.identify,
            "implementation_notes": spec.implementation_notes,
            "item_spec_sha256": readback.item_spec_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "item_specs.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/item_specs/{id}"),
            author: "editor_authoring.issue1682.item_specs".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
