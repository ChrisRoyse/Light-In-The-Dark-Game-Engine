//! Generic authored-property extractors shared by the #819 ability and #820 hero lowering
//! field sets. Pure deterministic parse-and-range-check helpers over the `EditorObject`
//! property bag; every out-of-band value is rejected with the canonical `object_editor_error`
//! so a lowering can never emit an out-of-band artifact. Kept separate from the unit field
//! set so ability/hero modules stay under the 500-line ratchet without re-listing helpers.

use litd_engine::EngineError;

use crate::history::EditorObject;

use super::validation::object_editor_error;

/// Parse an optional integer property, defaulting when absent, range-checked when present.
pub(crate) fn u32_field(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
    default: u32,
) -> Result<u32, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default);
    };
    let parsed = raw
        .trim()
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    check_range(key, parsed, min, max)?;
    Ok(parsed)
}

/// Trim-and-default a free-text property.
pub(crate) fn string_field(object: &EditorObject, key: &str, default: &str) -> String {
    object
        .properties
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Parse an optional enum property against a canonical allowed set.
pub(crate) fn enum_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
    default: &str,
) -> Result<String, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default.to_string());
    };
    let value = raw.trim().to_string();
    check_enum(key, &value, allowed)?;
    Ok(value)
}

/// Parse an optional flag list against a canonical allowed set; empty when absent.
pub(crate) fn flag_list_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let flags = split_list(raw);
    check_flags(key, &flags, allowed)?;
    Ok(flags)
}

/// Parse an optional list of unique free-form ids; empty when absent.
pub(crate) fn id_list_field(object: &EditorObject, key: &str) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let ids = split_list(raw);
    let mut seen = std::collections::BTreeSet::new();
    for id in &ids {
        if !seen.insert(id.as_str()) {
            return Err(object_editor_error(key, format!("duplicate id {id}")));
        }
    }
    Ok(ids)
}

/// Parse an optional per-level integer array; empty when absent, each element range-checked.
pub(crate) fn u32_list_field(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
) -> Result<Vec<u32>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let mut values = Vec::new();
    for token in split_list(raw) {
        let parsed = token.parse::<u32>().map_err(|_| {
            object_editor_error(key, format!("{token} must be an integer in {min}..={max}"))
        })?;
        check_range(key, parsed, min, max)?;
        values.push(parsed);
    }
    Ok(values)
}

/// Split a comma/pipe delimited list, trimming empties.
pub(crate) fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn check_range(key: &str, value: u32, min: u32, max: u32) -> Result<(), EngineError> {
    if value < min || value > max {
        return Err(object_editor_error(
            key,
            format!("must be in {min}..={max}; got {value}"),
        ));
    }
    Ok(())
}

pub(crate) fn check_enum(key: &str, value: &str, allowed: &[&str]) -> Result<(), EngineError> {
    if !allowed.contains(&value) {
        return Err(object_editor_error(
            key,
            format!("{value} is not one of {allowed:?}"),
        ));
    }
    Ok(())
}

pub(crate) fn check_flags(
    key: &str,
    flags: &[String],
    allowed: &[&str],
) -> Result<(), EngineError> {
    let mut seen = std::collections::BTreeSet::new();
    for flag in flags {
        if !allowed.contains(&flag.as_str()) {
            return Err(object_editor_error(
                key,
                format!("{flag} is not one of {allowed:?}"),
            ));
        }
        if !seen.insert(flag.as_str()) {
            return Err(object_editor_error(key, format!("duplicate flag {flag}")));
        }
    }
    Ok(())
}
