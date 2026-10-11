use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{SemanticAuthoringOpsLedger, validate_semantic_authoring_ops_ledger};

const SEMANTIC_AUTHORING_ASTER_COLLECTION: &str = "litd_semantic_authoring_ops";
const SEMANTIC_AUTHORING_LEDGER_COLUMN: &[u8] = b"semantic_authoring_ops_ledger_json";
const SEMANTIC_AUTHORING_ROW_PREFIX: &str = "semantic_authoring_ops/ledger/";
const SEMANTIC_AUTHORING_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB4";
const SEMANTIC_AUTHORING_VAULT_PURPOSE: &[u8] = b"litd-semantic-authoring-ops-issue1203";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAuthoringOpsWriteback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub report_count: usize,
}

pub fn write_semantic_authoring_ops_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &SemanticAuthoringOpsLedger,
) -> Result<SemanticAuthoringOpsWriteback, EngineError> {
    validate_semantic_authoring_ops_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster semantic-authoring bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_semantic_authoring_ops_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<SemanticAuthoringOpsWriteback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, SEMANTIC_AUTHORING_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, SEMANTIC_AUTHORING_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_semantic_authoring_ops_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(SemanticAuthoringOpsWriteback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        report_count: ledger.report_count,
    }))
}

pub fn parse_semantic_authoring_ops_ledger(
    bytes: &[u8],
) -> Result<SemanticAuthoringOpsLedger, EngineError> {
    let ledger = serde_json::from_slice::<SemanticAuthoringOpsLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse semantic-authoring ledger: {error}"),
        }
    })?;
    validate_semantic_authoring_ops_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &SemanticAuthoringOpsLedger,
    bytes: &[u8],
) -> Result<SemanticAuthoringOpsWriteback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, SEMANTIC_AUTHORING_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, SEMANTIC_AUTHORING_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, SEMANTIC_AUTHORING_LEDGER_COLUMN)
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
            "Aster Graph CF did not contain both semantic-authoring ledger rows",
        );
    }
    Ok(SemanticAuthoringOpsWriteback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        report_count: ledger.report_count,
    })
}

fn serialize_ledger(ledger: &SemanticAuthoringOpsLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize semantic-authoring ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = SEMANTIC_AUTHORING_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        SEMANTIC_AUTHORING_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{SEMANTIC_AUTHORING_ROW_PREFIX}{ledger_id}").into_bytes()
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
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.semantic_authoring.store.{}", field.into()),
        reason: reason.to_string(),
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
