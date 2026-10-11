use std::collections::BTreeSet;

use calyx_core::AnchorKind;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
        EngineContentKind, EngineContentRecord,
    },
    calyx_lowering::{EngineLoweredSimReadback, EngineLoweringError, EngineLoweringValve},
};

pub const RPG_AUTHORED_CONTENT_VALIDATION_SCHEMA: &str = "litd.rpg.authored_content_validation.v1";
pub const RPG_AUTHORED_CONTENT_LOWERING_SCHEMA: &str = "litd.rpg.authored_content_lowering.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpgAuthoredContentValidationReadback {
    pub schema: String,
    pub record_count: usize,
    pub acquired_map_record_count: usize,
    pub all_editor_authored_sources: bool,
    pub no_builtin_map_runtime: bool,
    pub no_self_attesting_anchors: bool,
    pub unique_record_ids: bool,
    pub source_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpgAuthoredContentEntityReadback {
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub source_path: String,
    pub cx_id: String,
    pub input_sha256: String,
    pub surface_kind: String,
    pub acquired_map_source_kind: Option<String>,
    pub lowered_field_count: usize,
    pub lowered_package_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpgAuthoredContentLoweringReadback {
    pub schema: String,
    pub validation: RpgAuthoredContentValidationReadback,
    pub row_count: usize,
    pub package_fingerprint: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub entities: Vec<RpgAuthoredContentEntityReadback>,
    pub lowered: Vec<EngineLoweredSimReadback>,
}

#[derive(Debug, thiserror::Error)]
pub enum RpgAuthoredContentError {
    #[error("RPG authored content field {field} is invalid: {reason}")]
    Invalid { field: String, reason: String },
    #[error("RPG authored content Calyx persistence failed: {0}")]
    Content(#[from] EngineCalyxContentError),
    #[error("RPG authored content lowering failed: {0}")]
    Lowering(#[from] EngineLoweringError),
    #[error("RPG authored content codec failed: {0}")]
    Codec(String),
}

pub fn validate_rpg_authored_content_records(
    records: &[EngineContentRecord],
) -> Result<RpgAuthoredContentValidationReadback, RpgAuthoredContentError> {
    if records.is_empty() {
        return invalid(
            "records",
            "at least one authored content record is required",
        );
    }

    let mut seen = BTreeSet::new();
    let mut source_paths = Vec::new();
    let mut acquired_map_record_count = 0;
    for record in records {
        validate_record(record)?;
        if !seen.insert((record.kind.as_str(), record.id.as_str())) {
            return invalid(
                "records.id",
                format!("duplicate authored content id {}", record.id),
            );
        }
        if extract_acquired_map_source_kind(&record.payload)?.is_some() {
            acquired_map_record_count += 1;
        }
        source_paths.push(record.source.path.clone());
    }

    Ok(RpgAuthoredContentValidationReadback {
        schema: RPG_AUTHORED_CONTENT_VALIDATION_SCHEMA.to_string(),
        record_count: records.len(),
        acquired_map_record_count,
        all_editor_authored_sources: true,
        no_builtin_map_runtime: true,
        no_self_attesting_anchors: true,
        unique_record_ids: true,
        source_paths,
    })
}

pub fn persist_and_lower_rpg_authored_content(
    store: &EngineCalyxContentStore,
    records: &[EngineContentRecord],
) -> Result<RpgAuthoredContentLoweringReadback, RpgAuthoredContentError> {
    let validation = validate_rpg_authored_content_records(records)?;
    let mut entities = Vec::new();
    let mut lowered = Vec::new();
    for record in records {
        let row = store.put_content(record)?;
        let package = EngineLoweringValve::lower_content_for_sim(&row)?;
        let lowered_readback = EngineLoweringValve::verify_lowered_package(&package)?;
        entities.push(entity_readback(&row, &lowered_readback)?);
        lowered.push(lowered_readback);
    }
    let package_fingerprint = authored_package_fingerprint(&entities)?;
    Ok(RpgAuthoredContentLoweringReadback {
        schema: RPG_AUTHORED_CONTENT_LOWERING_SCHEMA.to_string(),
        validation,
        row_count: entities.len(),
        package_fingerprint,
        calyx_live_query_allowed_in_hot_path: false,
        entities,
        lowered,
    })
}

pub fn rpg_authored_payload_without_nulls(mut payload: Value) -> Value {
    strip_object_nulls(&mut payload);
    payload
}

fn validate_record(record: &EngineContentRecord) -> Result<(), RpgAuthoredContentError> {
    validate_editor_source(&record.source.path)?;
    if record.source.author.trim().is_empty() {
        return invalid("source.author", "author must be non-empty");
    }
    if !is_sha256(&record.source.source_hash) {
        return invalid("source.source_hash", "must be a SHA-256 hex digest");
    }
    if record
        .anchors
        .iter()
        .any(|anchor| matches!(&anchor.kind, AnchorKind::TestPass))
    {
        return invalid(
            "anchors",
            "self-attesting TestPass anchors cannot be content provenance",
        );
    }
    validate_payload(&record.payload)
}

fn validate_editor_source(path: &str) -> Result<(), RpgAuthoredContentError> {
    if !(path.starts_with("editor://") || path.starts_with("mcp://")) {
        return invalid(
            "source.path",
            "RPG content must come from editor/MCP authoring, not compiled rpg:// source",
        );
    }
    Ok(())
}

fn validate_payload(payload: &Value) -> Result<(), RpgAuthoredContentError> {
    let Some(object) = payload.as_object() else {
        return invalid("payload", "authored payload must be a JSON object");
    };
    require_string(object.get("schema"), "payload.schema")?;
    let surface_kind = require_string(object.get("surface_kind"), "payload.surface_kind")?;
    if object.contains_key("readback") && surface_kind.contains("package") {
        return invalid(
            "payload.readback",
            "per-entity content cannot hide a whole package readback blob",
        );
    }
    reject_nulls(payload, "")?;
    reject_forbidden_runtime_map_flags(payload, "")?;
    validate_acquired_map_source(object.get("acquired_map_source"))?;
    Ok(())
}

fn strip_object_nulls(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, child| !child.is_null());
            for child in object.values_mut() {
                strip_object_nulls(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                strip_object_nulls(child);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn reject_nulls(value: &Value, path: &str) -> Result<(), RpgAuthoredContentError> {
    match value {
        Value::Null => invalid(path, "null cannot be lowered into deterministic sim data"),
        Value::Object(object) => {
            for (key, child) in object {
                reject_nulls(child, &format!("{path}/{key}"))?;
            }
            Ok(())
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_nulls(child, &format!("{path}/{index}"))?;
            }
            Ok(())
        }
        Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

fn validate_acquired_map_source(value: Option<&Value>) -> Result<(), RpgAuthoredContentError> {
    let Some(value) = value else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return invalid(
            "payload.acquired_map_source",
            "acquired map source must be an object",
        );
    };
    let source_kind = require_string(
        object.get("source_kind"),
        "payload.acquired_map_source.source_kind",
    )?;
    if !matches!(
        source_kind,
        "cartographer_purchase"
            | "found_or_looted"
            | "quest_reward"
            | "archive_copy"
            | "exploration_knowledge"
            | "npc_knowledge"
    ) {
        return invalid(
            "payload.acquired_map_source.source_kind",
            "must be a diegetic acquired map source kind",
        );
    }
    require_string(
        object.get("scope_id"),
        "payload.acquired_map_source.scope_id",
    )?;
    require_bool(
        object.get("requires_possession_or_knowledge"),
        "payload.acquired_map_source.requires_possession_or_knowledge",
        true,
    )?;
    require_bool(
        object.get("default_available"),
        "payload.acquired_map_source.default_available",
        false,
    )?;
    require_bool(
        object.get("player_position_indicator"),
        "payload.acquired_map_source.player_position_indicator",
        false,
    )?;
    require_bool(
        object.get("quest_markers"),
        "payload.acquired_map_source.quest_markers",
        false,
    )?;
    require_bool(
        object.get("route_lines"),
        "payload.acquired_map_source.route_lines",
        false,
    )?;
    require_bool(
        object.get("minimap_hud"),
        "payload.acquired_map_source.minimap_hud",
        false,
    )?;
    Ok(())
}

fn reject_forbidden_runtime_map_flags(
    value: &Value,
    path: &str,
) -> Result<(), RpgAuthoredContentError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_path = format!("{path}/{key}");
                if child.as_bool() == Some(true) && forbidden_true_key(key) {
                    return invalid(
                        child_path,
                        format!("{key}=true would create free runtime map/GPS access"),
                    );
                }
                reject_forbidden_runtime_map_flags(child, &child_path)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_forbidden_runtime_map_flags(child, &format!("{path}/{index}"))?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn forbidden_true_key(key: &str) -> bool {
    matches!(
        key,
        "built_in_map_required"
            | "grants_map_or_minimap"
            | "grants_gps_markers"
            | "default_player_map"
            | "default_world_map"
            | "free_map_screen"
            | "map_surface_visible_by_default"
            | "minimap_hud"
            | "gps_indicators"
            | "quest_markers"
            | "route_lines"
            | "player_position_indicator"
            | "player_dot"
            | "revealed_by_default_ui"
    )
}

fn entity_readback(
    row: &EngineContentConstellationReadback,
    lowered: &EngineLoweredSimReadback,
) -> Result<RpgAuthoredContentEntityReadback, RpgAuthoredContentError> {
    Ok(RpgAuthoredContentEntityReadback {
        kind: row.kind,
        id: row.id.clone(),
        display_name: row.display_name.clone(),
        source_path: row.source_path.clone(),
        cx_id: row.cx_id.clone(),
        input_sha256: row.input_sha256.clone(),
        surface_kind: require_string(row.payload.get("surface_kind"), "payload.surface_kind")?
            .to_string(),
        acquired_map_source_kind: extract_acquired_map_source_kind(&row.payload)?,
        lowered_field_count: lowered.field_count,
        lowered_package_fingerprint: lowered.package_fingerprint.clone(),
    })
}

fn extract_acquired_map_source_kind(
    payload: &Value,
) -> Result<Option<String>, RpgAuthoredContentError> {
    let Some(source) = payload.get("acquired_map_source") else {
        return Ok(None);
    };
    let Some(object) = source.as_object() else {
        return invalid(
            "payload.acquired_map_source",
            "acquired map source must be an object",
        );
    };
    Ok(Some(
        require_string(
            object.get("source_kind"),
            "payload.acquired_map_source.source_kind",
        )?
        .to_string(),
    ))
}

fn authored_package_fingerprint(
    entities: &[RpgAuthoredContentEntityReadback],
) -> Result<String, RpgAuthoredContentError> {
    serde_json::to_vec(entities)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| RpgAuthoredContentError::Codec(error.to_string()))
}

fn require_string<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<&'a str, RpgAuthoredContentError> {
    let Some(value) = value.and_then(Value::as_str) else {
        return invalid(field, "must be a non-empty string");
    };
    if value.trim().is_empty() {
        return invalid(field, "must be a non-empty string");
    }
    Ok(value)
}

fn require_bool(
    value: Option<&Value>,
    field: &str,
    expected: bool,
) -> Result<(), RpgAuthoredContentError> {
    match value.and_then(Value::as_bool) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) | None => invalid(field, format!("must be {expected}")),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, RpgAuthoredContentError> {
    Err(RpgAuthoredContentError::Invalid {
        field: field.into(),
        reason: reason.to_string(),
    })
}
