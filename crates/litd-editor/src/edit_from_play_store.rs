use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{EditFromPlayLedger, validate_edit_from_play_ledger};

const EDIT_FROM_PLAY_ASTER_COLLECTION: &str = "litd_edit_from_play";
const EDIT_FROM_PLAY_LEDGER_COLUMN: &[u8] = b"edit_from_play_ledger_json";
const EDIT_FROM_PLAY_ROW_PREFIX: &str = "play/edit_from_play/ledger/";
const EDIT_FROM_PLAY_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB4";
const EDIT_FROM_PLAY_VAULT_PURPOSE: &[u8] = b"litd-edit-from-play-issue1185";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditFromPlayAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub session_id: String,
    pub target_object_id: String,
    pub editor_focus_matches_play_target: bool,
    pub live_change_visible: bool,
    pub round_trip_preserves_play_state: bool,
}

pub fn write_edit_from_play_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &EditFromPlayLedger,
) -> Result<EditFromPlayAsterReadback, EngineError> {
    validate_edit_from_play_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster edit-from-play bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_edit_from_play_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<EditFromPlayAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, EDIT_FROM_PLAY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, EDIT_FROM_PLAY_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_edit_from_play_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(EditFromPlayAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        session_id: ledger.session_id,
        target_object_id: ledger.target.target_object_id,
        editor_focus_matches_play_target: ledger.readback.editor_focus_matches_play_target,
        live_change_visible: ledger.readback.live_change_visible,
        round_trip_preserves_play_state: ledger.readback.round_trip_preserves_play_state,
    }))
}

pub fn parse_edit_from_play_ledger(bytes: &[u8]) -> Result<EditFromPlayLedger, EngineError> {
    let ledger = serde_json::from_slice::<EditFromPlayLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse edit-from-play ledger: {error}"),
        }
    })?;
    validate_edit_from_play_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &EditFromPlayLedger,
    bytes: &[u8],
) -> Result<EditFromPlayAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, EDIT_FROM_PLAY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, EDIT_FROM_PLAY_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, EDIT_FROM_PLAY_LEDGER_COLUMN)
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
            "Aster Graph CF did not contain both edit-from-play ledger rows",
        );
    }
    Ok(EditFromPlayAsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        session_id: ledger.session_id.clone(),
        target_object_id: ledger.target.target_object_id.clone(),
        editor_focus_matches_play_target: ledger.readback.editor_focus_matches_play_target,
        live_change_visible: ledger.readback.live_change_visible,
        round_trip_preserves_play_state: ledger.readback.round_trip_preserves_play_state,
    })
}

fn serialize_ledger(ledger: &EditFromPlayLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize edit-from-play ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = EDIT_FROM_PLAY_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        EDIT_FROM_PLAY_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{EDIT_FROM_PLAY_ROW_PREFIX}{ledger_id}").into_bytes()
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
    let field = format!("editor.edit_from_play.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-EDIT-FROM-PLAY-STORE",
        "edit-from-play store rejected"
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
