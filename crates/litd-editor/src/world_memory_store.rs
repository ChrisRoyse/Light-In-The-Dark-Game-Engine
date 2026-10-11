use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{
    WorldMemoryConstellation, WorldMemoryIndex, validate_world_memory_constellation,
    validate_world_memory_index,
};

const WORLD_MEMORY_ASTER_COLLECTION: &str = "litd_editor_world_memory";
const WORLD_MEMORY_INDEX_COLUMN: &[u8] = b"world_memory_index_json";
const WORLD_MEMORY_CONSTELLATION_COLUMN: &[u8] = b"world_memory_constellation_json";
const WORLD_MEMORY_INDEX_ROW_PREFIX: &str = "world_memory/index/";
const WORLD_MEMORY_CONSTELLATION_ROW_PREFIX: &str = "world_memory/constellation/";
const WORLD_MEMORY_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const WORLD_MEMORY_VAULT_PURPOSE: &[u8] = b"litd-editor-world-memory-issue1209";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMemoryAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub constellation_rows: Vec<WorldMemoryConstellationAsterReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMemoryConstellationAsterReadback {
    pub constellation_id: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
}

pub fn write_world_memory_index(
    _state_path: &Path,
    aster_dir: &Path,
    index_id: &str,
    index: &WorldMemoryIndex,
) -> Result<WorldMemoryAsterReadback, EngineError> {
    validate_index_id(index_id)?;
    validate_world_memory_index(index)?;
    let bytes = serialize_index(index)?;
    let readback = write_aster_index(aster_dir, index_id, index, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.index.readback",
            "Aster index bytes did not match serialized world-memory index",
        );
    }
    Ok(readback)
}

pub fn read_world_memory_index(
    aster_dir: &Path,
    index_id: &str,
) -> Result<Option<WorldMemoryAsterReadback>, EngineError> {
    validate_index_id(index_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, WORLD_MEMORY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = index_row(index_id);
    let Some(bytes) = column
        .get(seq, &row, WORLD_MEMORY_INDEX_COLUMN)
        .map_err(|error| store_error("aster.index.get", error))?
    else {
        return Ok(None);
    };
    let index = parse_world_memory_index(&bytes)?;
    let mut constellation_rows = Vec::with_capacity(index.constellations.len());
    for constellation in &index.constellations {
        let row = read_world_memory_constellation(aster_dir, &constellation.constellation_id)?
            .ok_or_else(|| {
                store_error(
                    "aster.constellation.missing",
                    format!("missing constellation {}", constellation.constellation_id),
                )
            })?;
        constellation_rows.push(row);
    }
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(WorldMemoryAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        constellation_rows,
    }))
}

pub fn read_world_memory_constellation(
    aster_dir: &Path,
    constellation_id: &str,
) -> Result<Option<WorldMemoryConstellationAsterReadback>, EngineError> {
    validate_index_id(constellation_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, WORLD_MEMORY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = constellation_row(constellation_id);
    let Some(bytes) = column
        .get(seq, &row, WORLD_MEMORY_CONSTELLATION_COLUMN)
        .map_err(|error| store_error("aster.constellation.get", error))?
    else {
        return Ok(None);
    };
    let constellation = parse_world_memory_constellation(&bytes)?;
    if constellation.constellation_id != constellation_id {
        return invalid_store(
            "aster.constellation.id",
            format!(
                "read {} while requesting {constellation_id}",
                constellation.constellation_id
            ),
        );
    }
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(WorldMemoryConstellationAsterReadback {
        constellation_id: constellation_id.to_string(),
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
    }))
}

pub fn parse_world_memory_index(bytes: &[u8]) -> Result<WorldMemoryIndex, EngineError> {
    let index = serde_json::from_slice::<WorldMemoryIndex>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse world-memory index: {error}"),
        }
    })?;
    validate_world_memory_index(&index)?;
    Ok(index)
}

pub fn parse_world_memory_constellation(
    bytes: &[u8],
) -> Result<WorldMemoryConstellation, EngineError> {
    let constellation =
        serde_json::from_slice::<WorldMemoryConstellation>(bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("parse world-memory constellation: {error}"),
            }
        })?;
    validate_world_memory_constellation(&constellation)?;
    Ok(constellation)
}

fn write_aster_index(
    dir: &Path,
    index_id: &str,
    index: &WorldMemoryIndex,
    bytes: &[u8],
) -> Result<WorldMemoryAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, WORLD_MEMORY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let mut expected_graph_rows = Vec::new();
    let mut constellation_rows = Vec::new();
    for constellation in &index.constellations {
        let row = constellation_row(&constellation.constellation_id);
        let constellation_bytes = serialize_constellation(constellation)?;
        let commit = column
            .put(
                &row,
                WORLD_MEMORY_CONSTELLATION_COLUMN,
                &constellation_bytes,
            )
            .map_err(|error| store_error("aster.constellation.put", error))?;
        let value = column
            .get(commit.seq, &row, WORLD_MEMORY_CONSTELLATION_COLUMN)
            .map_err(|error| store_error("aster.constellation.get", error))?
            .ok_or_else(|| {
                store_error(
                    "aster.constellation.get",
                    format!(
                        "{} missing immediately after put",
                        constellation.constellation_id
                    ),
                )
            })?;
        if value != constellation_bytes {
            return invalid_store(
                "aster.constellation.readback",
                format!(
                    "{} bytes did not match immediately after put",
                    constellation.constellation_id
                ),
            );
        }
        expected_graph_rows.push((commit.cell_key, value.clone()));
        expected_graph_rows.push((commit.index_key, value.clone()));
        constellation_rows.push(WorldMemoryConstellationAsterReadback {
            constellation_id: constellation.constellation_id.clone(),
            sha256: sha256_hex(&value),
            bytes: value,
            seq: commit.seq,
            graph_rows: 0,
        });
    }

    let row = index_row(index_id);
    let commit = column
        .put(&row, WORLD_MEMORY_INDEX_COLUMN, bytes)
        .map_err(|error| store_error("aster.index.put", error))?;
    let value = column
        .get(commit.seq, &row, WORLD_MEMORY_INDEX_COLUMN)
        .map_err(|error| store_error("aster.index.get", error))?
        .ok_or_else(|| store_error("aster.index.get", "index missing immediately after put"))?;
    if value != bytes {
        return invalid_store("aster.index.readback", "index bytes differed after put");
    }
    expected_graph_rows.push((commit.cell_key.clone(), value.clone()));
    expected_graph_rows.push((commit.index_key.clone(), value.clone()));
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    for (key, value) in expected_graph_rows {
        if !raw
            .iter()
            .any(|(raw_key, raw_value)| raw_key == &key && raw_value == &value)
        {
            return invalid_store(
                "aster.graph_cf",
                "Aster Graph CF did not contain every index and constellation row",
            );
        }
    }
    for row in &mut constellation_rows {
        row.graph_rows = raw.len();
    }
    Ok(WorldMemoryAsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        constellation_rows,
    })
}

fn serialize_index(index: &WorldMemoryIndex) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(index).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize world-memory index: {error}"),
    })
}

fn serialize_constellation(
    constellation: &WorldMemoryConstellation,
) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(constellation).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize world-memory constellation: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = WORLD_MEMORY_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        WORLD_MEMORY_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn index_row(index_id: &str) -> Vec<u8> {
    format!("{WORLD_MEMORY_INDEX_ROW_PREFIX}{index_id}").into_bytes()
}

fn constellation_row(constellation_id: &str) -> Vec<u8> {
    format!("{WORLD_MEMORY_CONSTELLATION_ROW_PREFIX}{constellation_id}").into_bytes()
}

fn validate_index_id(value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_store("row", "Aster row id must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_memory.{}", field.into()),
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
