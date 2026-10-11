use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{QuickShareLedger, validate_quick_share_ledger};

const QUICK_SHARE_ASTER_COLLECTION: &str = "litd_quick_share";
const QUICK_SHARE_LEDGER_COLUMN: &[u8] = b"quick_share_ledger_json";
const QUICK_SHARE_ROW_PREFIX: &str = "quick_share/playtest/ledger/";
const QUICK_SHARE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB4";
const QUICK_SHARE_VAULT_PURPOSE: &[u8] = b"litd-quick-share-issue1187";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickShareAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub share_id: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub world_hash_matches: bool,
    pub runtime_lock_verified: bool,
}

pub fn write_quick_share_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &QuickShareLedger,
) -> Result<QuickShareAsterReadback, EngineError> {
    validate_ledger_id(&ledger.ledger_id)?;
    validate_quick_share_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster quick-share bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_quick_share_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<QuickShareAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, QUICK_SHARE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, QUICK_SHARE_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_quick_share_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(QuickShareAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        share_id: ledger.share_id,
        package_sha256: ledger.package_sha256,
        package_bytes: ledger.package_bytes,
        world_hash_matches: ledger.world_hash_matches,
        runtime_lock_verified: ledger.runtime_lock_verified,
    }))
}

pub fn parse_quick_share_ledger(bytes: &[u8]) -> Result<QuickShareLedger, EngineError> {
    let ledger = serde_json::from_slice::<QuickShareLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse quick-share ledger: {error}"),
        }
    })?;
    validate_quick_share_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &QuickShareLedger,
    bytes: &[u8],
) -> Result<QuickShareAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, QUICK_SHARE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    validate_ledger_id(&ledger.ledger_id)?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, QUICK_SHARE_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, QUICK_SHARE_LEDGER_COLUMN)
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
            "Aster Graph CF did not contain both quick-share ledger rows",
        );
    }
    Ok(QuickShareAsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        share_id: ledger.share_id.clone(),
        package_sha256: ledger.package_sha256.clone(),
        package_bytes: ledger.package_bytes,
        world_hash_matches: ledger.world_hash_matches,
        runtime_lock_verified: ledger.runtime_lock_verified,
    })
}

fn serialize_ledger(ledger: &QuickShareLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize quick-share ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = QUICK_SHARE_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        QUICK_SHARE_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{QUICK_SHARE_ROW_PREFIX}{ledger_id}").into_bytes()
}

fn validate_ledger_id(ledger_id: &str) -> Result<(), EngineError> {
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(ledger_id) {
        return invalid_store("row", "ledger id must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.quick_share.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-QUICK-SHARE-STORE",
        "quick-share store operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
