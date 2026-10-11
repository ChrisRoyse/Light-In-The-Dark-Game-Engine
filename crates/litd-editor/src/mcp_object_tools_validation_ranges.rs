//! #825/#827 numeric-list + float persist-time validators, split out of
//! `mcp_object_tools_validation` to keep that file under the 500-line ratchet. These reject
//! out-of-band tech / gameplay-constant property values BEFORE any Aster write, against the SAME
//! canonical ranges the lowering-time validator uses (single source of truth).

use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::object_editor::{XP_MAX_LEVELS, XP_THRESHOLD_MAX};

use super::{invalid_mcp_object, validate_identifier};

pub(super) fn validate_f64_range(
    field: &str,
    key: &str,
    value: &str,
    max: f64,
) -> Result<(), EngineError> {
    let parsed = value.parse::<f64>().map_err(|_| {
        invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must be a finite number in 0..={max}"),
        )
    })?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > max {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must be in 0..={max}; got {parsed}"),
        ));
    }
    Ok(())
}

pub(super) fn validate_monotonic_u32_list(
    field: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    let tokens = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() || tokens.len() > XP_MAX_LEVELS {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must have 1..={XP_MAX_LEVELS} thresholds"),
        ));
    }
    let mut previous: Option<u32> = None;
    for token in tokens {
        let parsed = token.parse::<u32>().map_err(|_| {
            invalid_mcp_object(
                format!("{field}.{key}"),
                format!("{key} threshold {token} is not an integer"),
            )
        })?;
        if parsed > XP_THRESHOLD_MAX {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("{key} threshold {parsed} exceeds {XP_THRESHOLD_MAX}"),
            ));
        }
        if let Some(prev) = previous
            && parsed <= prev
        {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("{key} thresholds must be strictly increasing; {parsed} follows {prev}"),
            ));
        }
        previous = Some(parsed);
    }
    Ok(())
}

/// #825 bounded id-list validator (max entry count): distinct from the shared unbounded
/// `lists::validate_id_list`, used for tech `requires`/`equivalents` prerequisite lists.
pub(super) fn validate_bounded_id_list(
    field: &str,
    key: &str,
    value: &str,
    max_len: usize,
) -> Result<(), EngineError> {
    let ids = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    if ids.len() > max_len {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} allows at most {max_len} ids; got {}", ids.len()),
        ));
    }
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
