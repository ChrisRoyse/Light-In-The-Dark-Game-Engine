use std::collections::BTreeSet;

use serde_json::Value;

use crate::{calyx_agent_runtime::EngineCalyxAgentRuntimeError, math::FixedVec3};

pub(crate) fn string_field(
    value: Option<&Value>,
    field: &str,
) -> Result<String, EngineCalyxAgentRuntimeError> {
    match value.and_then(Value::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        _ => invalid(field, "must be a non-empty string"),
    }
}

pub(crate) fn string_array(
    value: Option<&Value>,
    field: &str,
) -> Result<Vec<String>, EngineCalyxAgentRuntimeError> {
    let Some(values) = value.and_then(Value::as_array) else {
        return invalid(field, "must be a non-empty string array");
    };
    values
        .iter()
        .map(|value| string_field(Some(value), field))
        .collect()
}

pub(crate) fn u32_field(
    value: Option<&Value>,
    field: &str,
) -> Result<u32, EngineCalyxAgentRuntimeError> {
    let Some(value) = value.and_then(Value::as_u64) else {
        return invalid(field, "must be an unsigned integer");
    };
    u32::try_from(value).map_err(|_| invalid_error(field, "must fit in u32"))
}

pub(crate) fn u16_field(
    value: Option<&Value>,
    field: &str,
) -> Result<u16, EngineCalyxAgentRuntimeError> {
    let Some(value) = value.and_then(Value::as_u64) else {
        return invalid(field, "must be an unsigned integer");
    };
    u16::try_from(value).map_err(|_| invalid_error(field, "must fit in u16"))
}

pub(crate) fn velocity_field(
    value: Option<&Value>,
) -> Result<FixedVec3, EngineCalyxAgentRuntimeError> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(FixedVec3::from_i32s(1, 0, 0));
    };
    if values.len() != 3 {
        return invalid(
            "order_velocity",
            "velocity must contain exactly three integers",
        );
    }
    let mut out = [0_i32; 3];
    for (index, value) in values.iter().enumerate() {
        let Some(component) = value.as_i64() else {
            return invalid("order_velocity", "velocity components must be integers");
        };
        out[index] = i32::try_from(component)
            .map_err(|_| invalid_error("order_velocity", "component must fit in i32"))?;
    }
    Ok(FixedVec3::from_i32s(out[0], out[1], out[2]))
}

pub(crate) fn normalize_terms(
    terms: Vec<String>,
) -> Result<Vec<String>, EngineCalyxAgentRuntimeError> {
    let mut normalized = terms
        .into_iter()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        return invalid("terms", "at least one term is required");
    }
    normalized.sort();
    Ok(normalized)
}

pub(crate) fn lowered_agent_id(fingerprint: &str) -> Result<u32, EngineCalyxAgentRuntimeError> {
    let hex = fingerprint
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(8)
        .collect::<String>();
    if hex.len() != 8 {
        return invalid(
            "input_sha256",
            "fingerprint must contain at least eight hex digits",
        );
    }
    u32::from_str_radix(&hex, 16)
        .map(|value| value.max(1))
        .map_err(|error| invalid_error("input_sha256", error.to_string()))
}

pub(crate) fn merged_refs(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .chain(right)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxAgentRuntimeError {
    EngineCalyxAgentRuntimeError::Codec(error.to_string())
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxAgentRuntimeError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineCalyxAgentRuntimeError {
    EngineCalyxAgentRuntimeError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    }
}
