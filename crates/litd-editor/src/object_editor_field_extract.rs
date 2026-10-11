//! Shared property-bag field extraction + validation helpers for the #821 item and #822
//! shop/recipe/drop-table field sets (mirrors the private helpers baked into the #818
//! `object_editor_unit_fields` module, but factored out so the item and shop field-set
//! modules share ONE implementation instead of drifting copies). Every helper resolves an
//! authored `EditorObject` property to a typed value with a range/enum/flag/id-shape check,
//! so a lowering can never emit an out-of-band artifact and persist-time validation can call
//! the SAME canonical constant sets.

use litd_engine::EngineError;

use crate::history::EditorObject;

use super::validation::object_editor_error;

const MAX_ID_LEN: usize = 128;

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
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    check_range(key, parsed, min, max)?;
    Ok(parsed)
}

pub(crate) fn bool_field(
    object: &EditorObject,
    key: &str,
    default: bool,
) -> Result<bool, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default);
    };
    match raw.trim() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(object_editor_error(
            key,
            format!("must be a boolean (true/false); got {other}"),
        )),
    }
}

pub(crate) fn string_field(object: &EditorObject, key: &str, default: &str) -> String {
    object
        .properties
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

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

/// Required, deduplicated identifier list (comma/pipe separated). Empty is allowed only when
/// the property is absent; a present-but-empty value is rejected loud.
pub(crate) fn id_list_field(object: &EditorObject, key: &str) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let ids = split_list(raw);
    check_id_list(key, &ids)?;
    Ok(ids)
}

/// Optional flag list restricted to a canonical `allowed` set; absent or empty yields an empty
/// vec (unlike unit targeting flags, an item may legitimately carry no inventory flags).
pub(crate) fn opt_flag_list_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let flags = split_list(raw);
    check_flags_subset(key, &flags, allowed)?;
    Ok(flags)
}

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

pub(crate) fn check_flags_subset(
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

/// Validate a resolved identifier list: each id ASCII-graphic, bounded, unique.
pub(crate) fn check_id_list(key: &str, ids: &[String]) -> Result<(), EngineError> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        check_identifier(key, id)?;
        if !seen.insert(id.as_str()) {
            return Err(object_editor_error(key, format!("duplicate id {id}")));
        }
    }
    Ok(())
}

pub(crate) fn check_identifier(key: &str, id: &str) -> Result<(), EngineError> {
    if id.trim().is_empty()
        || id.len() > MAX_ID_LEN
        || !id.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(object_editor_error(
            key,
            format!("{id} is not a bounded printable-ASCII identifier"),
        ));
    }
    Ok(())
}
