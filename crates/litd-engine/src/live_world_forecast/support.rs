use calyx_aster::cf::full_content_hash;
use calyx_core::{CalyxError, VaultId};

use crate::EngineError;
use crate::diagnostics::log_engine_error;

use super::types::{
    DEFAULT_ENGINE_ORACLE_MIN_HISTORY, EngineOracleServiceConfig, LiveWorldMetricKind,
    LiveWorldObservation,
};

pub(super) const FORECAST_SCHEMA: &str = "litd.engine.live_world_forecast.v1";
pub(super) const FORECAST_KEY_PREFIX: &[u8] = b"litd-oracle-forecast-v1\0";
pub(super) const STREAM_PANEL_VERSION: u32 = 1_198;

pub(super) fn validate_service_config(
    config: &EngineOracleServiceConfig,
) -> Result<(), EngineError> {
    if config.vault_dir.as_os_str().is_empty() {
        return Err(invalid_config("vault_dir", "path must be non-empty"));
    }
    validate_stream_id(&config.actor)?;
    parse_vault_id(&config.vault_id)?;
    if config.vault_salt.is_empty() {
        return Err(invalid_config("vault_salt", "salt must be non-empty"));
    }
    if !config.confidence_ceiling.is_finite() || !(0.0..=1.0).contains(&config.confidence_ceiling) {
        return Err(invalid_config(
            "confidence_ceiling",
            "value must be finite and in [0, 1]",
        ));
    }
    if config.min_history < DEFAULT_ENGINE_ORACLE_MIN_HISTORY {
        return Err(invalid_config(
            "min_history",
            format!("value must be at least {DEFAULT_ENGINE_ORACLE_MIN_HISTORY}"),
        ));
    }
    if !config.overdue_alpha.is_finite() || !(0.0..1.0).contains(&config.overdue_alpha) {
        return Err(invalid_config(
            "overdue_alpha",
            "value must be finite and in [0, 1)",
        ));
    }
    if config.cusum_baseline_gaps < 2 {
        return Err(invalid_config(
            "cusum_baseline_gaps",
            "value must be at least 2",
        ));
    }
    Ok(())
}

pub(super) fn validate_observation(observation: &LiveWorldObservation) -> Result<(), EngineError> {
    validate_stream_id(&observation.stream_id)?;
    if observation.t_secs < 0 {
        return Err(invalid_config(
            "t_secs",
            format!("timestamp must be non-negative, got {}", observation.t_secs),
        ));
    }
    if !observation.value.is_finite() {
        return Err(invalid_config("value", "observation value must be finite"));
    }
    Ok(())
}

pub(super) fn validate_stream_id(value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(invalid_config("stream_id", "value must be non-empty"));
    }
    if value.len() > 96 {
        return Err(invalid_config("stream_id", "value must be <= 96 bytes"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(invalid_config(
            "stream_id",
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

pub(super) fn forecast_key(stream_id: &str, kind: LiveWorldMetricKind) -> Vec<u8> {
    let mut key = Vec::with_capacity(FORECAST_KEY_PREFIX.len() + stream_id.len() + 16);
    key.extend_from_slice(FORECAST_KEY_PREFIX);
    key.extend_from_slice(kind.as_str().as_bytes());
    key.push(0);
    key.extend_from_slice(stream_id.as_bytes());
    key
}

pub(super) fn run_calyx<T>(
    op: &'static str,
    action: impl FnOnce() -> calyx_core::Result<T>,
) -> Result<T, EngineError> {
    action().map_err(|source| {
        tracing::error!(
            op,
            calyx_error_code = source.code,
            calyx_error_message = %source.message,
            calyx_remediation = source.remediation,
            "engine oracle operation failed"
        );
        EngineError::OracleOperationFailed {
            op: op.to_string(),
            source: Box::new(source),
        }
    })
}

pub(super) fn invalid_config(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::OracleInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    };
    log_engine_error("oracle.invalid_config", &error);
    error
}

pub(super) fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::OraclePersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    };
    log_engine_error("oracle.persistence", &error);
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
    log_engine_error("oracle.readback_mismatch", &error);
    error
}

pub(super) fn insufficient_history(
    stream_id: impl Into<String>,
    support: usize,
    min_support: usize,
) -> EngineError {
    let error = EngineError::OracleInsufficientHistory {
        stream_id: stream_id.into(),
        support,
        min_support,
    };
    log_engine_error("oracle.insufficient_history", &error);
    error
}

pub(super) fn non_stationary(
    stream_id: impl Into<String>,
    direction: impl Into<String>,
    occurrence_index: usize,
) -> EngineError {
    let error = EngineError::OracleNonStationary {
        stream_id: stream_id.into(),
        direction: direction.into(),
        occurrence_index,
    };
    log_engine_error("oracle.non_stationary", &error);
    error
}

pub(super) fn calyx_insufficient(message: impl Into<String>) -> CalyxError {
    CalyxError::oracle_insufficient(message)
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
