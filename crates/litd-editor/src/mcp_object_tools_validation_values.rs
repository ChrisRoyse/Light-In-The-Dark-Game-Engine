//! Persist-time value validators for the #821 item + #822 commerce property keys (id lists,
//! booleans, drop-table `id:weight` entries). Split out of `mcp_object_tools_validation` to hold
//! the <500-line ratchet (#1466/#800 sec.6). Each rejects an out-of-band value with a typed
//! `invalid_mcp_object` error BEFORE any Aster write; the authoritative CROSS-record checks
//! (dangling refs, recipe cycles, weight sums) run later at lowering time against the document.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::object_editor::DROP_WEIGHT_MAX;

use super::{invalid_mcp_object, validate_identifier};

/// Validate a comma/pipe-separated identifier list: non-empty, each a bounded identifier, unique.
/// Used for item-ability refs (#821) and recipe/shop item refs (#822) at persist time.
pub(super) fn validate_id_list_value(
    field: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    let ids = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            "id list must contain at least one id",
        ));
    }
    let mut unique = BTreeSet::new();
    for id in ids {
        validate_identifier(key, id)?;
        if !unique.insert(id) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate id {id}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_bool_value(field: &str, key: &str, value: &str) -> Result<(), EngineError> {
    if !matches!(value.trim(), "true" | "false" | "1" | "0" | "yes" | "no") {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            format!("{key} must be a boolean (true/false)"),
        ));
    }
    Ok(())
}

/// Persist-time shape check for `drop.entries` = `item_id:weight,...`: valid ids, per-weight
/// range against the canonical `DROP_WEIGHT_MAX`, unique ids, at least one entry. The full
/// weight-sum + cross-record item-ref checks run at lowering time.
pub(super) fn validate_drop_entries_value(
    field: &str,
    key: &str,
    value: &str,
) -> Result<(), EngineError> {
    let tokens = value
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(invalid_mcp_object(
            format!("{field}.{key}"),
            "drop table must contain at least one weighted entry",
        ));
    }
    let mut unique = BTreeSet::new();
    for token in tokens {
        let (id, weight) = token.split_once(':').ok_or_else(|| {
            invalid_mcp_object(
                format!("{field}.{key}"),
                format!("entry {token} must be item_id:weight"),
            )
        })?;
        let id = id.trim();
        validate_identifier(key, id)?;
        let parsed = weight.trim().parse::<u32>().map_err(|_| {
            invalid_mcp_object(
                format!("{field}.{key}"),
                format!("weight for {id} must be an integer in 1..={DROP_WEIGHT_MAX}"),
            )
        })?;
        if !(1..=DROP_WEIGHT_MAX).contains(&parsed) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("weight for {id} must be in 1..={DROP_WEIGHT_MAX}; got {parsed}"),
            ));
        }
        if !unique.insert(id.to_string()) {
            return Err(invalid_mcp_object(
                format!("{field}.{key}"),
                format!("duplicate drop item {id}"),
            ));
        }
    }
    Ok(())
}
