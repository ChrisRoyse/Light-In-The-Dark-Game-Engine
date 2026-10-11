use litd_hash::sha256_hex;
use std::{fs, path::Path};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{
    ContextFeedbackLedger, ContextFeedbackScreenshotEvidence, validate_context_feedback_ledger,
};

const CONTEXT_FEEDBACK_ASTER_COLLECTION: &str = "litd_context_feedback";
const CONTEXT_FEEDBACK_LEDGER_COLUMN: &[u8] = b"context_feedback_ledger_json";
const CONTEXT_FEEDBACK_SCREENSHOT_CHUNK_COLUMN: &[u8] = b"context_feedback_screenshot_chunk_bytes";
const CONTEXT_FEEDBACK_ROW_PREFIX: &str = "play/context_feedback/ledger/";
const CONTEXT_FEEDBACK_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB5";
const CONTEXT_FEEDBACK_VAULT_PURPOSE: &[u8] = b"litd-context-feedback-issue1184";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFeedbackAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub session_id: String,
    pub feedback_count: usize,
    pub screenshot_count: usize,
    pub screenshot_blobs: Vec<ContextFeedbackScreenshotBlobReadback>,
    pub latest_sextant_self_found: bool,
    pub latest_loom_duplicate_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFeedbackScreenshotBlobReadback {
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub chunk_count: usize,
    pub total_bytes: usize,
}

pub fn write_context_feedback_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &ContextFeedbackLedger,
) -> Result<ContextFeedbackAsterReadback, EngineError> {
    validate_context_feedback_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster context-feedback bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_context_feedback_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<ContextFeedbackAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, CONTEXT_FEEDBACK_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, CONTEXT_FEEDBACK_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_context_feedback_ledger(&bytes)?;
    let screenshot_blobs = read_screenshot_blobs(&column, seq, &ledger)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(ContextFeedbackAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        session_id: ledger.session.session_id,
        feedback_count: ledger.readback.feedback_count,
        screenshot_count: ledger.readback.screenshot_count,
        screenshot_blobs,
        latest_sextant_self_found: ledger.readback.latest_sextant_self_found,
        latest_loom_duplicate_count: ledger.readback.latest_loom_duplicate_count,
    }))
}

pub fn parse_context_feedback_ledger(bytes: &[u8]) -> Result<ContextFeedbackLedger, EngineError> {
    let ledger = serde_json::from_slice::<ContextFeedbackLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse context-feedback ledger: {error}"),
        }
    })?;
    validate_context_feedback_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &ContextFeedbackLedger,
    bytes: &[u8],
) -> Result<ContextFeedbackAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, CONTEXT_FEEDBACK_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    write_screenshot_blobs(&column, ledger)?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, CONTEXT_FEEDBACK_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, CONTEXT_FEEDBACK_LEDGER_COLUMN)
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
            "Aster Graph CF did not contain both context-feedback ledger rows",
        );
    }
    let screenshot_blobs = read_screenshot_blobs(&column, commit.seq, ledger)?;
    Ok(ContextFeedbackAsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        session_id: ledger.session.session_id.clone(),
        feedback_count: ledger.readback.feedback_count,
        screenshot_count: ledger.readback.screenshot_count,
        screenshot_blobs,
        latest_sextant_self_found: ledger.readback.latest_sextant_self_found,
        latest_loom_duplicate_count: ledger.readback.latest_loom_duplicate_count,
    })
}

fn write_screenshot_blobs(
    column: &PlainColumn<'_, SystemClock>,
    ledger: &ContextFeedbackLedger,
) -> Result<(), EngineError> {
    for screenshot in unique_screenshots(ledger) {
        let bytes = fs::read(&screenshot.source_path).map_err(|error| {
            store_error(
                "screenshot.read",
                format!("read {}: {error}", screenshot.source_path),
            )
        })?;
        verify_screenshot_bytes(screenshot, &bytes)?;
        for (index, chunk) in bytes.chunks(screenshot.blob.chunk_size_bytes).enumerate() {
            let row = screenshot_chunk_row(screenshot, index);
            let commit = column
                .put(&row, CONTEXT_FEEDBACK_SCREENSHOT_CHUNK_COLUMN, chunk)
                .map_err(|error| store_error("screenshot.put", error))?;
            let value = column
                .get(commit.seq, &row, CONTEXT_FEEDBACK_SCREENSHOT_CHUNK_COLUMN)
                .map_err(|error| store_error("screenshot.get", error))?
                .ok_or_else(|| store_error("screenshot.get", "chunk missing after put"))?;
            if value.as_slice() != chunk {
                return invalid_store("screenshot.readback", "chunk bytes differed after put");
            }
        }
    }
    Ok(())
}

fn read_screenshot_blobs(
    column: &PlainColumn<'_, SystemClock>,
    seq: u64,
    ledger: &ContextFeedbackLedger,
) -> Result<Vec<ContextFeedbackScreenshotBlobReadback>, EngineError> {
    unique_screenshots(ledger)
        .into_iter()
        .map(|screenshot| {
            let mut bytes = Vec::with_capacity(screenshot.size_bytes);
            for index in 0..screenshot.blob.chunk_count {
                let row = screenshot_chunk_row(screenshot, index);
                let chunk = column
                    .get(seq, &row, CONTEXT_FEEDBACK_SCREENSHOT_CHUNK_COLUMN)
                    .map_err(|error| store_error("screenshot.get", error))?
                    .ok_or_else(|| {
                        store_error(
                            "screenshot.get",
                            format!(
                                "missing screenshot chunk {} for {}",
                                index, screenshot.sha256
                            ),
                        )
                    })?;
                bytes.extend_from_slice(&chunk);
            }
            verify_screenshot_bytes(screenshot, &bytes)?;
            Ok(ContextFeedbackScreenshotBlobReadback {
                sha256: screenshot.sha256.clone(),
                chunk_count: screenshot.blob.chunk_count,
                total_bytes: bytes.len(),
                bytes,
            })
        })
        .collect()
}

fn verify_screenshot_bytes(
    screenshot: &ContextFeedbackScreenshotEvidence,
    bytes: &[u8],
) -> Result<(), EngineError> {
    if bytes.len() != screenshot.size_bytes
        || bytes.len() != screenshot.blob.total_bytes
        || sha256_hex(bytes) != screenshot.sha256
        || screenshot.blob.sha256 != screenshot.sha256
    {
        return invalid_store(
            "screenshot.bytes",
            format!(
                "screenshot bytes do not match manifest for {}",
                screenshot.sha256
            ),
        );
    }
    Ok(())
}

fn unique_screenshots(ledger: &ContextFeedbackLedger) -> Vec<&ContextFeedbackScreenshotEvidence> {
    let mut by_sha = Vec::<&ContextFeedbackScreenshotEvidence>::new();
    for screenshot in ledger
        .events
        .iter()
        .filter_map(|event| event.feedback.as_ref())
        .map(|record| &record.screenshot)
    {
        if !by_sha.iter().any(|seen| seen.sha256 == screenshot.sha256) {
            by_sha.push(screenshot);
        }
    }
    by_sha
}

fn screenshot_chunk_row(screenshot: &ContextFeedbackScreenshotEvidence, index: usize) -> Vec<u8> {
    format!("{}{:06}", screenshot.blob.chunk_row_prefix, index).into_bytes()
}

fn serialize_ledger(ledger: &ContextFeedbackLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize context-feedback ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = CONTEXT_FEEDBACK_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        CONTEXT_FEEDBACK_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{CONTEXT_FEEDBACK_ROW_PREFIX}{ledger_id}").into_bytes()
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
    let field = format!("editor.context_feedback.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-CONTEXT-FEEDBACK-STORE",
        "context-feedback store rejected"
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
