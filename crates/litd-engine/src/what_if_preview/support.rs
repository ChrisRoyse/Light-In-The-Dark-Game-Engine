use calyx_aster::cf::full_content_hash;
use calyx_core::{CalyxError, VaultId};
use calyx_oracle::{OracleError, SufficiencyBound};

use crate::EngineError;
use crate::diagnostics::log_engine_error;

use super::types::{
    WhatIfPreviewRejectionReason, WhatIfPreviewRequest, WhatIfPreviewServiceConfig,
};

pub(super) const PREVIEW_SCHEMA: &str = "litd.engine.what_if_preview.v1";
pub(super) const PREVIEW_KEY_PREFIX: &[u8] = b"litd-oracle-what-if-preview-v1\0";

pub(super) fn validate_config(config: &WhatIfPreviewServiceConfig) -> Result<(), EngineError> {
    if config.vault_dir.as_os_str().is_empty() {
        return Err(invalid_config("vault_dir", "path must be non-empty"));
    }
    validate_id("actor", &config.actor)?;
    parse_vault_id(&config.vault_id)?;
    if config.vault_salt.is_empty() {
        return Err(invalid_config("vault_salt", "salt must be non-empty"));
    }
    Ok(())
}

pub(super) fn validate_request(request: &WhatIfPreviewRequest) -> Result<(), EngineError> {
    validate_id("domain_id", &request.domain_id)?;
    validate_id("action_id", &request.action_id)?;
    if request.panel.slots.is_empty() {
        return Err(invalid_config(
            "panel.slots",
            "what-if preview requires at least one panel slot for sufficiency",
        ));
    }
    Ok(())
}

pub(super) fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(invalid_config(field, "value must be non-empty"));
    }
    if value.len() > 96 {
        return Err(invalid_config(field, "value must be <= 96 bytes"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(invalid_config(
            field,
            "value must contain only ASCII letters, digits, '-', '_' or '.'",
        ));
    }
    Ok(())
}

pub(super) fn parse_vault_id(value: &str) -> Result<VaultId, EngineError> {
    value
        .parse()
        .map_err(|error| invalid_config("vault_id", format!("invalid ULID: {error}")))
}

pub(super) fn preview_key(domain_id: &str, action_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(PREVIEW_KEY_PREFIX.len() + domain_id.len() + action_id.len());
    key.extend_from_slice(PREVIEW_KEY_PREFIX);
    key.extend_from_slice(domain_id.as_bytes());
    key.push(0);
    key.extend_from_slice(action_id.as_bytes());
    key
}

pub(super) fn map_oracle_rejection(error: &OracleError) -> WhatIfPreviewRejectionReason {
    match error {
        OracleError::Insufficient { .. } => WhatIfPreviewRejectionReason::Insufficient,
        OracleError::NoRecurrence { .. } => WhatIfPreviewRejectionReason::NoRecurrence,
        OracleError::DomainNotFound => WhatIfPreviewRejectionReason::DomainNotFound,
        OracleError::LedgerWriteFailure => WhatIfPreviewRejectionReason::LedgerWriteFailure,
        OracleError::FlakyAnchor { .. }
        | OracleError::SlotConflict { .. }
        | OracleError::AssayFailure { .. } => WhatIfPreviewRejectionReason::OracleOperationFailed,
    }
}

pub(super) fn oracle_bound(error: &OracleError) -> Option<SufficiencyBound> {
    match error {
        OracleError::Insufficient { bound } => Some(bound.clone()),
        _ => None,
    }
}

pub(super) fn oracle_to_engine_error(
    domain_id: &str,
    action_id: &str,
    error: OracleError,
) -> EngineError {
    if let OracleError::Insufficient { bound } = error {
        let out = EngineError::OraclePreviewInsufficient {
            domain_id: domain_id.to_string(),
            action_id: action_id.to_string(),
            i_panel_oracle: bound.i_panel_oracle,
            dpi_ceiling: bound.dpi_ceiling,
        };
        log_engine_error("oracle.preview.insufficient", &out);
        return out;
    }
    EngineError::OracleOperationFailed {
        op: "oracle.what_if_preview".to_string(),
        source: Box::new(CalyxError::from(error)),
    }
}

pub(super) fn invalid_config(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::OracleInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    };
    log_engine_error("oracle.what_if.invalid_config", &error);
    error
}

pub(super) fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::OraclePersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    };
    log_engine_error("oracle.what_if.persistence", &error);
    error
}

pub(super) fn readback_mismatch(
    field: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> EngineError {
    let error = EngineError::OracleReadbackMismatch {
        field: field.into(),
        expected: expected.into(),
        actual: actual.into(),
    };
    log_engine_error("oracle.what_if.readback_mismatch", &error);
    error
}

pub(super) fn calyx_failed(op: &'static str, source: CalyxError) -> EngineError {
    tracing::error!(
        op,
        calyx_error_code = source.code,
        calyx_error_message = %source.message,
        calyx_remediation = source.remediation,
        "engine what-if oracle operation failed"
    );
    EngineError::OracleOperationFailed {
        op: op.to_string(),
        source: Box::new(source),
    }
}

pub(super) fn hash_hex(bytes: &[u8]) -> String {
    let hash = full_content_hash([bytes]);
    hex_bytes(&hash)
}

pub(super) fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + value - 10),
        _ => unreachable!("nibble out of range"),
    }
}
