use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{EditorIntelligenceSurfaceDeck, validate_editor_intelligence_surface_deck};

const EDITOR_INTELLIGENCE_ASTER_COLLECTION: &str = "litd_editor_intelligence_surfaces";
const EDITOR_INTELLIGENCE_DECK_COLUMN: &[u8] = b"editor_intelligence_surface_deck_json";
const EDITOR_INTELLIGENCE_ROW_PREFIX: &str = "editor_intelligence_surfaces/deck/";
const EDITOR_INTELLIGENCE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB6";
const EDITOR_INTELLIGENCE_VAULT_PURPOSE: &[u8] = b"litd-editor-intelligence-surfaces-issue1420";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorIntelligenceSurfaceWriteback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub surface_count: usize,
    pub observation_count: usize,
}

pub fn write_editor_intelligence_surface_deck(
    _state_path: &Path,
    aster_dir: &Path,
    deck: &EditorIntelligenceSurfaceDeck,
) -> Result<EditorIntelligenceSurfaceWriteback, EngineError> {
    validate_editor_intelligence_surface_deck(deck)?;
    let bytes = serialize_deck(deck)?;
    let readback = write_aster_deck(aster_dir, deck, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster editor-intelligence bytes did not match serialized deck",
        );
    }
    Ok(readback)
}

pub fn read_editor_intelligence_surface_deck(
    aster_dir: &Path,
    deck_id: &str,
) -> Result<Option<EditorIntelligenceSurfaceWriteback>, EngineError> {
    validate_deck_id(deck_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, EDITOR_INTELLIGENCE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = deck_row(deck_id);
    let Some(bytes) = column
        .get(seq, &row, EDITOR_INTELLIGENCE_DECK_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let deck = parse_editor_intelligence_surface_deck(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(EditorIntelligenceSurfaceWriteback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        surface_count: deck.surface_count,
        observation_count: deck.observation_count,
    }))
}

pub fn parse_editor_intelligence_surface_deck(
    bytes: &[u8],
) -> Result<EditorIntelligenceSurfaceDeck, EngineError> {
    let deck = serde_json::from_slice::<EditorIntelligenceSurfaceDeck>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse editor-intelligence surface deck: {error}"),
        }
    })?;
    validate_editor_intelligence_surface_deck(&deck)?;
    Ok(deck)
}

fn write_aster_deck(
    dir: &Path,
    deck: &EditorIntelligenceSurfaceDeck,
    bytes: &[u8],
) -> Result<EditorIntelligenceSurfaceWriteback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, EDITOR_INTELLIGENCE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = deck_row(&deck.deck_id);
    let commit = column
        .put(&row, EDITOR_INTELLIGENCE_DECK_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, EDITOR_INTELLIGENCE_DECK_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
        .ok_or_else(|| store_error("aster.get", "deck missing immediately after put"))?;
    if value != bytes {
        return invalid_store(
            "aster.readback",
            "deck bytes differed immediately after put",
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
            "Aster Graph CF did not contain both editor-intelligence deck rows",
        );
    }
    Ok(EditorIntelligenceSurfaceWriteback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        surface_count: deck.surface_count,
        observation_count: deck.observation_count,
    })
}

fn serialize_deck(deck: &EditorIntelligenceSurfaceDeck) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(deck).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize editor-intelligence surface deck: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = EDITOR_INTELLIGENCE_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        EDITOR_INTELLIGENCE_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn deck_row(deck_id: &str) -> Vec<u8> {
    format!("{EDITOR_INTELLIGENCE_ROW_PREFIX}{deck_id}").into_bytes()
}

fn validate_deck_id(deck_id: &str) -> Result<(), EngineError> {
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(deck_id) {
        return invalid_store("row", "deck id must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.intelligence_surfaces.store.{}", field.into()),
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
