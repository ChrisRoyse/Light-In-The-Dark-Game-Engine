use std::collections::BTreeSet;

use crate::EngineError;

pub(super) fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be {max_len} bytes or shorter"));
    }
    Ok(())
}

pub(super) fn validate_option_token(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    if let Some(value) = value {
        validate_token(field, value)?;
    }
    Ok(())
}

pub(super) fn validate_token_list(
    field: &str,
    values: &[String],
    max_len: usize,
) -> Result<(), EngineError> {
    if values.len() > max_len {
        return invalid(field, format!("at most {max_len} entries"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        if !seen.insert(value.as_str()) {
            return invalid(field, format!("duplicate {value}"));
        }
    }
    Ok(())
}

pub(super) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > 96 {
        return invalid(field, "must be 96 bytes or shorter");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgAbilityInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
