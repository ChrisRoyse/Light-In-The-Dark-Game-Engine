//! Object-editor unit validation, property extraction, and lowering helpers (#1466 split from
//! `object_editor.rs`). Crate-visible so the public lowering API in `object_editor` keeps calling
//! these by name; not exported outside the crate.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use super::unit_fields::{build_wc3_fields, validate_wc3_fields};
use super::{
    OBJECT_EDITOR_LOWERING_VERSION, OBJECT_EDITOR_UNIT_TOML_SCHEMA_VERSION,
    ObjectEditorFieldValidation, ObjectEditorLoweredUnit, ObjectEditorLoweredUnitDocument,
    UNIT_HIT_POINTS_MAX, UNIT_HIT_POINTS_MIN, UNIT_MOVE_SPEED_MAX, UNIT_MOVE_SPEED_MIN,
};
use crate::{history::EditorObject, mcp_object_tools::ObjectDataKind};

pub(crate) fn lowered_unit_from_object(
    object: &EditorObject,
) -> Result<ObjectEditorLoweredUnit, EngineError> {
    Ok(ObjectEditorLoweredUnit {
        object_id: object.id.clone(),
        rawcode: required_property(object, "object.rawcode")?,
        base_object_id: required_property(object, "object.base_id")?,
        display_name: required_property(object, "object.display_name")?,
        hit_points: parse_u32_property(
            object,
            "stats.hit_points",
            UNIT_HIT_POINTS_MIN,
            UNIT_HIT_POINTS_MAX,
        )?,
        move_speed: parse_u32_property(
            object,
            "stats.move_speed",
            UNIT_MOVE_SPEED_MIN,
            UNIT_MOVE_SPEED_MAX,
        )?,
        model: required_property(object, "art.model")?,
        abilities: ability_list(
            object
                .properties
                .get("abilities.normal")
                .map(String::as_str),
        )?,
        wc3: build_wc3_fields(object)?,
    })
}

pub(crate) fn validate_lowered_unit_document(
    document: &ObjectEditorLoweredUnitDocument,
) -> Result<(), EngineError> {
    if document.schema_version != OBJECT_EDITOR_UNIT_TOML_SCHEMA_VERSION {
        return invalid_object_editor("toml.schema_version", "expected schema version 1");
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.unit.object_id.trim().is_empty()
        || document.unit.rawcode.len() != 4
        || document.unit.base_object_id.trim().is_empty()
        || document.unit.display_name.trim().is_empty()
        || document.unit.model.trim().is_empty()
        || !document.validation.lower_unit_valid
        || !document.validation.range_checked
        || document.provenance.lowering_version != OBJECT_EDITOR_LOWERING_VERSION
    {
        return invalid_object_editor(
            "toml.required",
            "lowered unit TOML is missing required data",
        );
    }
    validate_range(
        "toml.unit.move_speed",
        document.unit.move_speed,
        UNIT_MOVE_SPEED_MIN,
        UNIT_MOVE_SPEED_MAX,
    )?;
    validate_range(
        "toml.unit.hit_points",
        document.unit.hit_points,
        UNIT_HIT_POINTS_MIN,
        UNIT_HIT_POINTS_MAX,
    )?;
    validate_ability_uniqueness(&document.unit.abilities)?;
    validate_wc3_fields(&document.unit.wc3)?;
    Ok(())
}

pub(crate) fn lowered_unit_fingerprint(
    document: &ObjectEditorLoweredUnitDocument,
) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered unit fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}

pub(crate) fn object_data_kind(kind: &str) -> Option<ObjectDataKind> {
    Some(match kind {
        "Unit" => ObjectDataKind::Unit,
        "Hero" => ObjectDataKind::Hero,
        "Ability" => ObjectDataKind::Ability,
        "Item" => ObjectDataKind::Item,
        "Buff" => ObjectDataKind::Buff,
        "Upgrade" => ObjectDataKind::Upgrade,
        "Recipe" => ObjectDataKind::Recipe,
        "Shop" => ObjectDataKind::Shop,
        "DropTable" => ObjectDataKind::DropTable,
        "Destructible" => ObjectDataKind::Destructible,
        "Doodad" => ObjectDataKind::Doodad,
        "GameInterface" => ObjectDataKind::GameInterface,
        _ => return None,
    })
}

pub(crate) fn editable_properties(object: &EditorObject) -> BTreeMap<String, String> {
    object
        .properties
        .iter()
        .filter(|(key, _)| !key.starts_with("object."))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub(crate) fn required_property(object: &EditorObject, key: &str) -> Result<String, EngineError> {
    object
        .properties
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| object_editor_error(key, "required unit property is missing"))
}

pub(crate) fn parse_u32_property(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
) -> Result<u32, EngineError> {
    let value = required_property(object, key)?;
    let parsed = value
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    validate_range(key, parsed, min, max)?;
    Ok(parsed)
}

pub(crate) fn validate_range(
    field: &str,
    value: u32,
    min: u32,
    max: u32,
) -> Result<(), EngineError> {
    if value < min || value > max {
        return invalid_object_editor(field, format!("must be in {min}..={max}; got {value}"));
    }
    Ok(())
}

pub(crate) fn ability_list(value: Option<&str>) -> Result<Vec<String>, EngineError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let abilities = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|ability| !ability.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    validate_ability_uniqueness(&abilities)?;
    Ok(abilities)
}

pub(crate) fn validate_ability_uniqueness(abilities: &[String]) -> Result<(), EngineError> {
    let mut unique = BTreeSet::new();
    for ability in abilities {
        if ability.trim().is_empty() {
            return invalid_object_editor("unit.abilities", "ability id must be non-empty");
        }
        if !unique.insert(ability.as_str()) {
            return invalid_object_editor(
                "unit.abilities",
                format!("duplicate ability id {ability}"),
            );
        }
    }
    Ok(())
}

pub(crate) fn push_required(
    fields: &mut Vec<ObjectEditorFieldValidation>,
    field: &str,
    value: &str,
    valid: bool,
) {
    fields.push(ObjectEditorFieldValidation {
        field: field.to_string(),
        value: value.to_string(),
        valid,
        message: if valid {
            "ok".to_string()
        } else {
            "required value failed LowerUnit validation".to_string()
        },
    });
}

pub(crate) fn push_required_property(
    fields: &mut Vec<ObjectEditorFieldValidation>,
    object: &EditorObject,
    key: &str,
) {
    let value = object.properties.get(key).cloned().unwrap_or_default();
    push_required(fields, key, &value, !value.trim().is_empty());
}

pub(crate) fn push_u32_property(
    fields: &mut Vec<ObjectEditorFieldValidation>,
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
) {
    let value = object.properties.get(key).cloned().unwrap_or_default();
    let valid = value
        .parse::<u32>()
        .map(|parsed| parsed >= min && parsed <= max)
        .unwrap_or(false);
    fields.push(ObjectEditorFieldValidation {
        field: key.to_string(),
        value,
        valid,
        message: if valid {
            format!("ok {min}..={max}")
        } else {
            format!("must be an integer in {min}..={max}")
        },
    });
}

pub(crate) fn push_model_property(
    fields: &mut Vec<ObjectEditorFieldValidation>,
    object: &EditorObject,
    key: &str,
) {
    let value = object.properties.get(key).cloned().unwrap_or_default();
    let lower = value.to_ascii_lowercase();
    let valid = lower.ends_with(".mdl") || lower.ends_with(".glb") || lower.ends_with(".gltf");
    fields.push(ObjectEditorFieldValidation {
        field: key.to_string(),
        value,
        valid,
        message: if valid {
            "ok".to_string()
        } else {
            "model path must end in .mdl, .glb, or .gltf".to_string()
        },
    });
}

pub(crate) fn push_abilities_property(
    fields: &mut Vec<ObjectEditorFieldValidation>,
    object: &EditorObject,
    key: &str,
) {
    let value = object.properties.get(key).cloned().unwrap_or_default();
    let valid = ability_list(Some(&value)).is_ok();
    fields.push(ObjectEditorFieldValidation {
        field: key.to_string(),
        value,
        valid,
        message: if valid {
            "ok".to_string()
        } else {
            "ability ids must be non-empty and unique".to_string()
        },
    });
}

pub(crate) fn invalid_object_editor<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(object_editor_error(field, reason))
}

pub(crate) fn object_editor_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.object_editor.{}", field.into()),
        reason: reason.into(),
    }
}
