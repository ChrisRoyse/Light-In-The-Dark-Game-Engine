use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{ImmersivePlayLedger, validate_immersive_play_ledger};

const IMMERSIVE_PLAY_ASTER_COLLECTION: &str = "litd_immersive_play";
const IMMERSIVE_PLAY_LEDGER_COLUMN: &[u8] = b"immersive_play_ledger_json";
const IMMERSIVE_PLAY_ROW_PREFIX: &str = "play/immersive/ledger/";
const IMMERSIVE_PLAY_VAULT_ID: &str = "01J2X3Y4Z5A6B7C8D9E0F1G2H3";
const IMMERSIVE_PLAY_VAULT_PURPOSE: &[u8] = b"litd-immersive-play-issue1182";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmersivePlayAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub input_event_count: usize,
    pub screenshot_sha256: String,
    pub editor_restored_exact: bool,
    pub fullscreen_toggle_stable: bool,
}

pub fn write_immersive_play_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &ImmersivePlayLedger,
) -> Result<ImmersivePlayAsterReadback, EngineError> {
    validate_immersive_play_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster immersive-play bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_immersive_play_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<ImmersivePlayAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, IMMERSIVE_PLAY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, IMMERSIVE_PLAY_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_immersive_play_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(readback_from_ledger(bytes, seq, raw.len(), ledger)))
}

pub fn parse_immersive_play_ledger(bytes: &[u8]) -> Result<ImmersivePlayLedger, EngineError> {
    let ledger = serde_json::from_slice::<ImmersivePlayLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse immersive-play ledger: {error}"),
        }
    })?;
    validate_immersive_play_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &ImmersivePlayLedger,
    bytes: &[u8],
) -> Result<ImmersivePlayAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, IMMERSIVE_PLAY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, IMMERSIVE_PLAY_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, IMMERSIVE_PLAY_LEDGER_COLUMN)
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
            "Aster Graph CF did not contain both immersive-play ledger rows",
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
    ledger: ImmersivePlayLedger,
) -> ImmersivePlayAsterReadback {
    ImmersivePlayAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows,
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        visible_instances: ledger.readback.visible_instances,
        draw_batches: ledger.readback.draw_batches,
        input_event_count: ledger.readback.input_event_count,
        screenshot_sha256: ledger.readback.screenshot_sha256,
        editor_restored_exact: ledger.readback.editor_restored_exact,
        fullscreen_toggle_stable: ledger.readback.fullscreen_toggle_stable,
    }
}

fn serialize_ledger(ledger: &ImmersivePlayLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize immersive-play ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = IMMERSIVE_PLAY_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        IMMERSIVE_PLAY_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{IMMERSIVE_PLAY_ROW_PREFIX}{ledger_id}").into_bytes()
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
    let field = format!("editor.immersive_play.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-IMMERSIVE-PLAY-STORE",
        "immersive-play store rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
