use std::collections::BTreeMap;

use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentKind,
        EngineContentRecord, EngineContentSource,
    },
};

use super::{
    ClassLegacy, ClassLeveledAbility, ClassSpec, RPG_CLASS_SPEC_SCHEMA, class_spec_readback,
    rpg_authored_payload_without_nulls, validate_class_spec, validate_rpg_authored_content_records,
};

pub fn class_spec_content_records(
    spec: &ClassSpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_class_spec(spec)?;
    let readback = class_spec_readback(spec)?;
    // The shared lowering flattener (`calyx_lowering::flatten_payload`) sorts
    // object keys but emits array indices numerically, so an array of >= 11
    // elements yields field paths that are not strictly lexicographically
    // sorted (".../10" < ".../9") and the lowering valve rejects them. The two
    // large collections (61 leveled abilities, 36 legacies) are therefore
    // persisted as key-sorted JSON objects keyed by a stable, unique id; the
    // structural `ClassSpec` API keeps them as ordered `Vec`s and the
    // `class_spec_sha256` covers the full struct, so integrity is unchanged.
    let records = vec![content_record(
        EngineContentKind::Unit,
        &spec.class_id,
        &format!("Class Spec: {}", spec.display_name),
        json!({
            "schema": RPG_CLASS_SPEC_SCHEMA,
            "source_issue": spec.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "class_spec",
            "class_id": spec.class_id,
            "max_level": spec.max_level,
            "display_name": spec.display_name,
            "player_facing_canon_name": spec.player_facing_canon_name,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "source_history_label": spec.source_history_label,
            "concept_summary": spec.concept_summary,
            "prime_stat": spec.prime_stat,
            "stat_minimums": spec.stat_minimums,
            "alignment": spec.alignment,
            "ethos": spec.ethos,
            "eligibility": spec.eligibility,
            "alignment_exceptions": spec.alignment_exceptions,
            "fount": spec.fount,
            "empowerment": spec.empowerment,
            "race_access": spec.race_access,
            "allowed_weapons": spec.allowed_weapons,
            "allowed_armor": spec.allowed_armor,
            "leveled_abilities": leveled_abilities_map(&spec.leveled_abilities)?,
            "leveled_ability_count": spec.leveled_abilities.len(),
            "specializations": spec.specializations,
            "specialization_choice_levels": spec.specialization_choice_levels,
            "legacies": legacies_map(&spec.legacies)?,
            "legacy_count": spec.legacies.len(),
            "legacy_choice_levels": spec.legacy_choice_levels,
            "visual": spec.visual,
            "implementation_notes": spec.implementation_notes,
            "class_spec_sha256": readback.class_spec_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "class_specs.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

/// Decode one editor-authored ClassSpec from the canonical content-row payload.
///
/// The lowering representation uses key-sorted objects for the two large collections;
/// restore those collections in key order, validate the reconstructed spec, and compare
/// its independent hash to the persisted `class_spec_sha256` before returning it.
/// Missing or malformed fields are hard errors; no v1/default reconstruction is allowed.
pub fn class_spec_from_content_payload(payload: &Value) -> Result<ClassSpec, EngineError> {
    let mut object = payload.as_object().cloned().ok_or_else(|| {
        EngineError::RpgStatsInvalidConfig {
            field: "class_spec.payload".to_string(),
            reason: "authored ClassSpec payload must be a JSON object".to_string(),
        }
    })?;
    let leveled = object
        .remove("leveled_abilities")
        .ok_or_else(|| invalid_payload("leveled_abilities", "field is missing"))?;
    let legacies = object
        .remove("legacies")
        .ok_or_else(|| invalid_payload("legacies", "field is missing"))?;
    let expected_hash = object
        .get("class_spec_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_payload("class_spec_sha256", "field is missing or not a string"))?
        .to_string();
    object.remove("leveled_ability_count");
    object.remove("legacy_count");
    object.remove("class_spec_sha256");
    object.insert(
        "leveled_abilities".to_string(),
        map_values_to_array("leveled_abilities", leveled)?,
    );
    object.insert(
        "legacies".to_string(),
        map_values_to_array("legacies", legacies)?,
    );
    let spec: ClassSpec = serde_json::from_value(Value::Object(object)).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "class_spec.payload".to_string(),
            reason: format!("decode authored ClassSpec: {error}"),
        }
    })?;
    validate_class_spec(&spec)?;
    let actual_hash = class_spec_readback(&spec)?.class_spec_sha256;
    if actual_hash != expected_hash {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "class_spec_sha256".to_string(),
            reason: format!("authored hash mismatch: expected {expected_hash}, actual {actual_hash}"),
        });
    }
    Ok(spec)
}

/// Load and decode one ClassSpec from the canonical Calyx content store. This is a
/// startup/editor boundary; callers must freeze the result before entering a sim hot path.
pub fn load_class_spec_from_calyx(
    store: &EngineCalyxContentStore,
    class_id: &str,
) -> Result<ClassSpec, EngineCalyxContentError> {
    let readback = store.get_content_for_editor(EngineContentKind::Unit, class_id)?;
    class_spec_from_content_payload(&readback.payload).map_err(|error| {
        EngineCalyxContentError::InvalidContent {
            field: format!("class_spec.{class_id}"),
            reason: error.to_string(),
        }
    })
}

fn map_values_to_array(field: &str, value: Value) -> Result<Value, EngineError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_payload(field, "field must be a key-sorted JSON object"))?;
    Ok(Value::Array(object.values().cloned().collect()))
}

fn invalid_payload(field: &str, reason: &str) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("class_spec.{field}"),
        reason: reason.to_string(),
    }
}

/// Leveled abilities keyed by `LLL.ability_id` (zero-padded level) so the
/// lowering flattener's key sort matches level-then-id order and the
/// `(level, ability_id)` uniqueness the validator already guarantees.
fn leveled_abilities_map(
    abilities: &[ClassLeveledAbility],
) -> Result<BTreeMap<String, Value>, EngineError> {
    let mut map = BTreeMap::new();
    for ability in abilities {
        let key = format!("{:03}.{}", ability.level, ability.ability_id);
        map.insert(key, to_value(ability)?);
    }
    Ok(map)
}

/// Legacies keyed by their unique `legacy_id`.
fn legacies_map(legacies: &[ClassLegacy]) -> Result<BTreeMap<String, Value>, EngineError> {
    let mut map = BTreeMap::new();
    for legacy in legacies {
        map.insert(legacy.legacy_id.clone(), to_value(legacy)?);
    }
    Ok(map)
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize class spec collection: {error}"),
    })
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
            path: format!("editor://rpg/ashen_reach/class_specs/{id}"),
            author: "editor_authoring.issue1455.class_specs".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
