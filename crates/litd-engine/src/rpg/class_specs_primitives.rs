//! Shared primitive validators for the ClassSpec framework, used by both
//! `class_specs.rs` and `class_specs_validate_v2.rs`. Kept in a module that is
//! NOT glob-re-exported through `rpg.rs` so these common names (`invalid`,
//! `validate_token`, ...) do not collide with the identically-named helpers in
//! other `rpg` submodules (e.g. `unhallowed_knight_validate`).
use crate::EngineError;

use super::race_specs_types::RaceAccessRule;

pub(crate) fn validate_access(field: &str, access: &RaceAccessRule) -> Result<(), EngineError> {
    if let RaceAccessRule::Restricted { values } = access {
        if values.is_empty() {
            return invalid(field, "restricted access requires values");
        }
        for value in values {
            validate_token(field, value)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(crate) fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
