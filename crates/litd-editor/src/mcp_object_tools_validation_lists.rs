//! Persist-time leaf validators for list/enum/flag/range property values shared by the unit,
//! ability (#819), and hero (#820) field sets. Split out of `mcp_object_tools_validation` to hold
//! the <500-line ratchet (#1466/#800 sec.6). Each rejects an out-of-band value with a typed
//! `invalid_mcp_object` error; the allowed enum/flag sets are passed in by the caller so this
//! module stays free of any single-source-of-truth constant duplication.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::object_editor::{HERO_LEVEL_MAX, HERO_LEVEL_MIN};

use super::{invalid_mcp_object, validate_identifier};

pub(super) fn validate_u32_list(
    field: &str,
    key: &str,
    value: &str,
    min: u32,
    max: u32,
) -> Result<(), EngineError> {
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        validate_u32_range(field, key, token, min, max)?;
    }
    Ok(())
}

pub(super) fn validate_id_list(field: &str, key: &str, value: &str) -> Result<(), EngineError> {
    let ids = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    for id in ids {
        validate_identifier(key, id)?;
        if !seen.insert(id) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate id {id}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_learnable_list(
    field: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for token in value
        .split([',', '|'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let (ability_id, level) = token.split_once('@').ok_or_else(|| {
            invalid_mcp_object(
                format!("{field}.{key}"),
                format!("slot {token} must be <ability_id>@<level>"),
            )
        })?;
        validate_identifier(key, ability_id.trim())?;
        validate_u32_range(field, key, level.trim(), HERO_LEVEL_MIN, HERO_LEVEL_MAX)?;
        if !seen.insert(ability_id.trim().to_string()) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate learnable ability {}", ability_id.trim()),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_enum_value(
    field: &str,
    key: &str,
    value: &str,
    allowed: &[&str],
) -> Result<(), EngineError> {
    if !allowed.contains(&value.trim()) {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{value} is not one of {allowed:?}"),
        ));
    }
    Ok(())
}

pub(super) fn validate_flag_list(
    field: &str,
    key: &str,
    value: &str,
    allowed: &[&str],
) -> Result<(), EngineError> {
    let flags = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|flag| !flag.is_empty())
        .collect::<Vec<_>>();
    if flags.is_empty() {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            "at least one flag is required",
        ));
    }
    let mut seen = BTreeSet::new();
    for flag in flags {
        if !allowed.contains(&flag) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("{flag} is not one of {allowed:?}"),
            ));
        }
        if !seen.insert(flag) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate flag {flag}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_u32_range(
    field: &str,
    key: &str,
    value: &str,
    min: u32,
    max: u32,
) -> Result<(), EngineError> {
    let parsed = value.parse::<u32>().map_err(|_| {
        invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must be an integer in {min}..={max}"),
        )
    })?;
    if parsed < min || parsed > max {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must be in {min}..={max}; got {parsed}"),
        ));
    }
    Ok(())
}

pub(super) fn validate_ability_list(
    field: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    let abilities = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|ability| !ability.is_empty())
        .collect::<Vec<_>>();
    if abilities.is_empty() {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            "ability list must contain at least one ability id",
        ));
    }
    let mut unique = BTreeSet::new();
    for ability in abilities {
        validate_identifier(key, ability)?;
        if !unique.insert(ability) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate ability id {ability}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_model_path(field: &str, key: &str, value: &str) -> Result<(), EngineError> {
    let lower = value.to_ascii_lowercase();
    if !(lower.ends_with(".mdl") || lower.ends_with(".glb") || lower.ends_with(".gltf")) {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            "model path must end in .mdl, .glb, or .gltf",
        ));
    }
    Ok(())
}
