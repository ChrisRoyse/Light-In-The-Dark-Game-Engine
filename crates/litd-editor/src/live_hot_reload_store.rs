use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{LiveHotReloadLedger, validate_live_hot_reload_ledger};

const LIVE_HOT_RELOAD_ASTER_COLLECTION: &str = "litd_live_hot_reload";
const LIVE_HOT_RELOAD_LEDGER_COLUMN: &[u8] = b"live_hot_reload_ledger_json";
const LIVE_HOT_RELOAD_ROW_PREFIX: &str = "play/live_hot_reload/ledger/";
const LIVE_HOT_RELOAD_VAULT_ID: &str = "01J2X3Y4Z5A6B7C8D9E0F1G2H4";
const LIVE_HOT_RELOAD_VAULT_PURPOSE: &[u8] = b"litd-live-hot-reload-issue1181";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveHotReloadAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub patch_count: usize,
    pub applied_live_count: usize,
    pub deferred_structural_count: usize,
    pub final_generation: u64,
    pub final_runtime_state_hash: u64,
}

pub fn write_live_hot_reload_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &LiveHotReloadLedger,
) -> Result<LiveHotReloadAsterReadback, EngineError> {
    validate_live_hot_reload_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster live-hot-reload bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_live_hot_reload_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<LiveHotReloadAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, LIVE_HOT_RELOAD_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, LIVE_HOT_RELOAD_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_live_hot_reload_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(readback_from_ledger(bytes, seq, raw.len(), ledger)))
}

pub fn parse_live_hot_reload_ledger(bytes: &[u8]) -> Result<LiveHotReloadLedger, EngineError> {
    let ledger = serde_json::from_slice::<LiveHotReloadLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse live-hot-reload ledger: {error}"),
        }
    })?;
    validate_live_hot_reload_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &LiveHotReloadLedger,
    bytes: &[u8],
) -> Result<LiveHotReloadAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, LIVE_HOT_RELOAD_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, LIVE_HOT_RELOAD_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, LIVE_HOT_RELOAD_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
        .ok_or_else(|| store_error("aster.get", "ledger missing immediately after put"))?;
    if value != bytes {
        return invalid_store(
            "aster.readback",
            "ledger bytes differed immediately after put",
        );
    }
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return invalid_store(
            "aster.graph_cf",
            "Aster Graph CF did not contain both live-hot-reload ledger rows",
        );
    }
    let mut readback = readback_from_ledger(value, commit.seq, raw.len(), ledger.clone());
    readback.cell_key = hex_preview(&commit.cell_key);
    readback.index_key = hex_preview(&commit.index_key);
    Ok(readback)
}

fn readback_from_ledger(
    bytes: Vec<u8>,
    seq: u64,
    graph_rows: usize,
    ledger: LiveHotReloadLedger,
) -> LiveHotReloadAsterReadback {
    LiveHotReloadAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows,
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        patch_count: ledger.readback.patch_count,
        applied_live_count: ledger.readback.applied_live_count,
        deferred_structural_count: ledger.readback.deferred_structural_count,
        final_generation: ledger.readback.final_generation,
        final_runtime_state_hash: ledger.readback.final_runtime_state_hash,
    }
}

fn serialize_ledger(ledger: &LiveHotReloadLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize live-hot-reload ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = LIVE_HOT_RELOAD_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        LIVE_HOT_RELOAD_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{LIVE_HOT_RELOAD_ROW_PREFIX}{ledger_id}").into_bytes()
}

fn validate_ledger_id(ledger_id: &str) -> Result<(), EngineError> {
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(ledger_id) {
        return invalid_store("row", "ledger id must be non-empty and single-line");
    }
    Ok(())
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.live_hot_reload.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-LIVE-HOT-RELOAD-STORE",
        "live-hot-reload store rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
