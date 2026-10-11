use calyx_anneal::{
    ActionMetricSnapshot, AnnealAction, ArtifactKey, ArtifactPtr, HeldOutReplay, ReplayAnchor,
    ReplayQuery, RollbackReadback, TripwireMetric,
};
use calyx_aster::cf::{full_content_hash, ledger_key};
use calyx_core::{CxId, VaultId};
use calyx_ledger::{EntryKind, decode as decode_ledger};
use serde_json::json;

use crate::EngineError;
use crate::diagnostics::log_engine_error;

use super::types::{
    AnnealLedgerRowReadback, EngineAnnealRevertReason, EngineAnnealServiceConfig,
    EngineRollbackReadback, EngineTuneConfig, EngineTuneParams,
};

pub(super) struct EngineConfigAction {
    metrics: super::types::EngineTuneMetrics,
}

impl EngineConfigAction {
    pub(super) const fn new(metrics: super::types::EngineTuneMetrics) -> Self {
        Self { metrics }
    }
}

impl AnnealAction for EngineConfigAction {
    fn apply_shadow(&self, _query: &ReplayQuery) -> ActionMetricSnapshot {
        ActionMetricSnapshot::from_values([
            (TripwireMetric::RecallAtK, self.metrics.recall_at_k),
            (TripwireMetric::GuardFAR, self.metrics.guard_far),
            (TripwireMetric::GuardFRR, self.metrics.guard_frr),
            (TripwireMetric::SearchP99, self.metrics.search_p99_ms),
            (TripwireMetric::IngestP95, self.metrics.ingest_p95_ms),
        ])
    }
}

pub(super) fn held_out_replay(count: usize, seed: u64) -> HeldOutReplay {
    HeldOutReplay {
        seed,
        queries: (0..count)
            .map(|idx| ReplayQuery {
                query_id: idx as u64,
                query_vector: vec![idx as f32, 1.0, (seed % 17) as f32],
                expected_top_k: vec![ReplayAnchor {
                    cx_id: CxId::from_bytes([idx as u8; 16]),
                    similarity: 0.99,
                }],
            })
            .collect(),
    }
}

pub(super) fn validate_service_config(
    config: &EngineAnnealServiceConfig,
) -> Result<(), EngineError> {
    if config.vault_dir.as_os_str().is_empty() {
        return Err(invalid_config("vault_dir", "path must be non-empty"));
    }
    validate_profile_id(&config.actor)?;
    parse_vault_id(&config.vault_id)?;
    if config.vault_salt.is_empty() {
        return Err(invalid_config("vault_salt", "salt must be non-empty"));
    }
    if config.budget_ticks == 0 && config.replay_query_count > 0 {
        return Err(invalid_config(
            "budget_ticks",
            "budget must cover at least one replay query",
        ));
    }
    validate_fraction(
        "min_search_p99_gain_fraction",
        config.min_search_p99_gain_fraction,
    )
}

pub(super) fn validate_tune_pair(
    incumbent: &EngineTuneConfig,
    candidate: &EngineTuneConfig,
) -> Result<(), EngineError> {
    validate_tune_config(incumbent)?;
    validate_tune_config(candidate)?;
    if incumbent.domain != candidate.domain {
        return Err(invalid_config(
            "domain",
            "candidate domain must match incumbent",
        ));
    }
    if incumbent.profile_id != candidate.profile_id {
        return Err(invalid_config(
            "profile_id",
            "candidate profile_id must match incumbent",
        ));
    }
    Ok(())
}

pub(super) fn validate_tune_config(config: &EngineTuneConfig) -> Result<(), EngineError> {
    validate_profile_id(&config.profile_id)?;
    validate_params(&config.params)?;
    validate_metric("recall_at_k", config.metrics.recall_at_k, 0.0, 1.0)?;
    validate_metric("guard_far", config.metrics.guard_far, 0.0, 1.0)?;
    validate_metric("guard_frr", config.metrics.guard_frr, 0.0, 1.0)?;
    validate_metric("search_p99_ms", config.metrics.search_p99_ms, 0.0, f64::MAX)?;
    validate_metric("ingest_p95_ms", config.metrics.ingest_p95_ms, 0.0, f64::MAX)
}

fn validate_params(params: &EngineTuneParams) -> Result<(), EngineError> {
    if params.index_ef_search == 0 || params.index_ef_search > 4096 {
        return Err(invalid_config(
            "index_ef_search",
            "value must be in 1..=4096",
        ));
    }
    if !matches!(params.index_quant_bits, 4 | 8 | 16 | 32) {
        return Err(invalid_config(
            "index_quant_bits",
            "value must be one of 4, 8, 16, 32",
        ));
    }
    if params.forge_tile_m == 0 || params.forge_tile_n == 0 {
        return Err(invalid_config(
            "forge_tile",
            "tile dimensions must be non-zero",
        ));
    }
    if params.render_max_draws == 0 {
        return Err(invalid_config("render_max_draws", "value must be non-zero"));
    }
    if !(1..=8).contains(&params.render_shadow_cascades) {
        return Err(invalid_config(
            "render_shadow_cascades",
            "value must be in 1..=8",
        ));
    }
    if params.ward_tau_micros == 0 {
        return Err(invalid_config("ward_tau_micros", "value must be non-zero"));
    }
    Ok(())
}

pub(super) fn validate_profile_id(value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(invalid_config("profile_id", "value must be non-empty"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(invalid_config(
            "profile_id",
            "value must contain only ASCII letters, digits, '-', '_' or '.'",
        ));
    }
    Ok(())
}

fn validate_metric(field: &'static str, value: f64, min: f64, max: f64) -> Result<(), EngineError> {
    if value.is_finite() && (min..=max).contains(&value) {
        return Ok(());
    }
    Err(invalid_config(
        field,
        format!("value must be finite and in [{min}, {max}], got {value}"),
    ))
}

fn validate_fraction(field: &'static str, value: f64) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    Err(invalid_config(
        field,
        format!("value must be finite and in [0, 1], got {value}"),
    ))
}

pub(super) fn parse_vault_id(value: &str) -> Result<VaultId, EngineError> {
    value
        .parse()
        .map_err(|error| invalid_config("vault_id", format!("invalid ULID: {error}")))
}

pub(super) fn artifact_key(
    domain: super::types::EngineTuneDomain,
    profile_id: &str,
) -> ArtifactKey {
    ArtifactKey::ConfigCache(artifact_key_hash(domain, profile_id))
}

pub(super) fn artifact_key_hash(
    domain: super::types::EngineTuneDomain,
    profile_id: &str,
) -> [u8; 32] {
    full_content_hash([domain.as_str().as_bytes(), profile_id.as_bytes()])
}

pub(super) fn artifact_id(key: &ArtifactKey) -> String {
    match key {
        ArtifactKey::ConfigCache(hash)
        | ArtifactKey::HnswGraph(hash)
        | ArtifactKey::QuantLevel(hash) => hash_hex(hash),
    }
}

pub(super) fn config_hash(config: &EngineTuneConfig) -> Result<[u8; 32], EngineError> {
    let bytes = serde_json::to_vec_pretty(config).map_err(|error| {
        persistence_failed(
            "anneal.config.serialize",
            format!("serialize config: {error}"),
        )
    })?;
    Ok(full_content_hash([bytes.as_slice()]))
}

pub(super) fn ptr_hash(ptr: &ArtifactPtr) -> Option<[u8; 32]> {
    match ptr {
        ArtifactPtr::ConfigCacheKeyHash(hash) | ArtifactPtr::QuantLevelRecordHash(hash) => {
            Some(*hash)
        }
        ArtifactPtr::HnswGraphPath(path) => Some(full_content_hash([path.as_bytes()])),
    }
}

pub(super) fn rollback_json(readback: &RollbackReadback) -> EngineRollbackReadback {
    EngineRollbackReadback {
        snapshot: readback.snapshot.clone(),
        live_ptr: readback.live_ptr.clone(),
        snapshot_key_hex: hex_bytes(&readback.snapshot_key),
        snapshot_bytes_hex: hex_bytes(&readback.snapshot_bytes),
        live_key_hex: hex_bytes(&readback.live_key),
        live_bytes_hex: hex_bytes(&readback.live_bytes),
    }
}

pub(super) fn ledger_row_from_bytes(
    key: Vec<u8>,
    bytes: Vec<u8>,
) -> Result<AnnealLedgerRowReadback, EngineError> {
    let entry = decode_ledger(&bytes).map_err(|error| {
        log_calyx_failure("anneal.ledger.decode", &error);
        EngineError::AnnealOperationFailed {
            op: "anneal.ledger.decode".to_string(),
            source: Box::new(error),
        }
    })?;
    if entry.kind != EntryKind::Anneal || key != ledger_key(entry.seq) {
        return Err(readback_mismatch(
            "ledger_row",
            format!("kind=Anneal key={}", hex_bytes(&ledger_key(entry.seq))),
            format!("kind={} key={}", entry.kind.as_str(), hex_bytes(&key)),
        ));
    }
    let payload_json = serde_json::from_slice(&entry.payload).map_err(|error| {
        persistence_failed(
            "anneal.ledger.payload.decode",
            format!("decode payload: {error}"),
        )
    })?;
    Ok(AnnealLedgerRowReadback {
        seq: entry.seq,
        key_hex: hex_bytes(&key),
        kind: entry.kind.as_str().to_string(),
        entry_hash: hex_bytes(&entry.entry_hash),
        prev_hash: hex_bytes(&entry.prev_hash),
        payload_json,
    })
}

pub(super) fn reason_details(reason: &EngineAnnealRevertReason) -> serde_json::Value {
    json!({ "engine_revert_reason": reason })
}

pub(super) fn run_calyx<T>(
    op: &'static str,
    action: impl FnOnce() -> calyx_core::Result<T>,
) -> Result<T, EngineError> {
    action().map_err(|source| {
        log_calyx_failure(op, &source);
        EngineError::AnnealOperationFailed {
            op: op.to_string(),
            source: Box::new(source),
        }
    })
}

fn log_calyx_failure(op: &'static str, error: &calyx_core::CalyxError) {
    tracing::error!(
        op,
        calyx_error_code = error.code,
        calyx_error_message = %error.message,
        calyx_remediation = error.remediation,
        "engine anneal operation failed"
    );
}

fn invalid_config(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::AnnealInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    };
    log_engine_error("anneal.invalid_config", &error);
    error
}

pub(super) fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let error = EngineError::AnnealPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    };
    log_engine_error("anneal.persistence", &error);
    error
}

pub(super) fn readback_mismatch(
    field: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> EngineError {
    let error = EngineError::AnnealReadbackMismatch {
        field: field.into(),
        expected: expected.into(),
        actual: actual.into(),
    };
    log_engine_error("anneal.readback_mismatch", &error);
    error
}

pub(super) fn hash_hex(hash: &[u8; 32]) -> String {
    hex_bytes(hash)
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
