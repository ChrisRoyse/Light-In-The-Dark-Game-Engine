use std::{
    env,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;
use litd_hash::sha256_hex;

use super::{EditorWorldRecord, invalid_world_creation, validate_world_record};

#[path = "world_creation_chunk_store.rs"]
mod chunk_store;
use chunk_store::{
    WORLD_ASTER_CHUNK_SCHEMA as CHUNK_SCHEMA, WORLD_ASTER_CHUNK_SIZE as CHUNK_SIZE,
    WORLD_ASTER_LEGACY_SCHEMA as LEGACY_SCHEMA, WORLD_ASTER_MANIFEST_COLUMN as MANIFEST_COLUMN,
    assemble_chunks, build_chunk_manifest, manifest_to_bytes, parse_chunk_manifest,
    verify_world_record_bytes,
};

pub const WORLD_ASTER_ENV: &str = "LITD_EDITOR_WORLD_ASTER";
pub const WORLD_ASTER_COLLECTION: &str = "litd_editor_worlds";
pub const WORLD_ASTER_ROW_PREFIX: &str = "world/";
pub const WORLD_ASTER_COLUMN: &[u8] = b"world_record_json";
pub const WORLD_ASTER_MANIFEST_COLUMN: &[u8] = MANIFEST_COLUMN;
pub const WORLD_ASTER_CHUNK_SCHEMA: &str = CHUNK_SCHEMA;
pub const WORLD_ASTER_CHUNK_SIZE: usize = CHUNK_SIZE;
const WORLD_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const WORLD_VAULT_PURPOSE: &[u8] = b"litd-editor-world-creation-issue1210";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub storage_schema: String,
    pub chunk_count: usize,
    pub sha256: String,
    pub document_fingerprint: String,
    pub manifest_bytes: usize,
    pub chunk_columns: Vec<String>,
}

pub fn aster_dir_from_env() -> Result<PathBuf, EngineError> {
    env_path(WORLD_ASTER_ENV, default_aster_dir())
}

pub fn default_aster_dir() -> PathBuf {
    workspace_root().join("tmp").join("litd-editor-world-aster")
}

pub fn write_world_record(
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<WorldAsterReadback, EngineError> {
    validate_world_record(record)?;
    let bytes =
        serde_json::to_vec_pretty(record).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize world record: {error}"),
        })?;
    let readback = write_aster_record(aster_dir, record, &bytes)?;
    if readback.bytes != bytes {
        return invalid_world_creation(
            "aster.readback",
            "Aster world_record_json bytes did not match serialized world record",
        );
    }
    Ok(readback)
}

pub fn read_world_record(
    aster_dir: &Path,
    calyx_document_id: &str,
) -> Result<Option<WorldAsterReadback>, EngineError> {
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION)
        .map_err(|error| world_store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row_key = row_key(calyx_document_id)?;
    let manifest = column
        .get(seq, &row_key, WORLD_ASTER_MANIFEST_COLUMN)
        .map_err(|error| world_store_error("aster.manifest.get", error))?;
    if let Some(manifest_bytes) = manifest {
        return read_chunked_aster_record(
            &vault,
            &column,
            seq,
            &row_key,
            calyx_document_id,
            &manifest_bytes,
        )
        .map(Some);
    }

    let value = column
        .get(seq, &row_key, WORLD_ASTER_COLUMN)
        .map_err(|error| world_store_error("aster.get", error))?;
    let Some(bytes) = value else {
        return Ok(None);
    };
    let document_fingerprint = verify_world_record_bytes(calyx_document_id, &bytes, None)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| world_store_error("aster.graph_scan", error))?;
    Ok(Some(WorldAsterReadback {
        sha256: sha256_hex(&bytes),
        document_fingerprint,
        storage_schema: LEGACY_SCHEMA.to_string(),
        chunk_count: 1,
        manifest_bytes: 0,
        chunk_columns: Vec::new(),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
    }))
}

pub fn parse_world_record(bytes: &[u8]) -> Result<EditorWorldRecord, EngineError> {
    let record = serde_json::from_slice::<EditorWorldRecord>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse world record: {error}"),
        }
    })?;
    validate_world_record(&record)?;
    Ok(record)
}

fn write_aster_record(
    dir: &Path,
    record: &EditorWorldRecord,
    bytes: &[u8],
) -> Result<WorldAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION)
        .map_err(|error| world_store_error("aster.column", error))?;
    let row_key = row_key(&record.spec.calyx_document_id)?;
    let manifest = build_chunk_manifest(record, bytes)?;
    let mut expected_graph_rows = Vec::new();
    for (index, chunk) in bytes.chunks(WORLD_ASTER_CHUNK_SIZE).enumerate() {
        let column_name = manifest.chunk_columns[index].as_bytes();
        let commit = column
            .put(&row_key, column_name, chunk)
            .map_err(|error| world_store_error("aster.chunk.put", error))?;
        let value = column
            .get(commit.seq, &row_key, column_name)
            .map_err(|error| world_store_error("aster.chunk.get", error))?
            .ok_or_else(|| {
                world_store_error(
                    "aster.chunk.get",
                    format!(
                        "{} missing immediately after put",
                        manifest.chunk_columns[index]
                    ),
                )
            })?;
        if value != chunk {
            return invalid_world_creation(
                "aster.chunk.readback",
                format!("chunk {index} bytes did not match immediately after put"),
            );
        }
        expected_graph_rows.push((commit.cell_key, chunk.to_vec()));
        expected_graph_rows.push((commit.index_key, chunk.to_vec()));
    }
    let manifest_bytes = manifest_to_bytes(&manifest)?;
    let commit = column
        .put(&row_key, WORLD_ASTER_MANIFEST_COLUMN, &manifest_bytes)
        .map_err(|error| world_store_error("aster.manifest.put", error))?;
    let value = column
        .get(commit.seq, &row_key, WORLD_ASTER_MANIFEST_COLUMN)
        .map_err(|error| world_store_error("aster.manifest.get", error))?
        .ok_or_else(|| {
            world_store_error(
                "aster.manifest.get",
                "world_record_manifest_json missing immediately after put",
            )
        })?;
    if value != manifest_bytes {
        return invalid_world_creation(
            "aster.manifest.readback",
            "Aster world_record_manifest_json bytes did not match serialized manifest",
        );
    }
    expected_graph_rows.push((commit.cell_key.clone(), manifest_bytes.clone()));
    expected_graph_rows.push((commit.index_key.clone(), manifest_bytes));
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| world_store_error("aster.graph_scan", error))?;
    for (key, value) in expected_graph_rows {
        if !raw
            .iter()
            .any(|(raw_key, raw_value)| raw_key == &key && raw_value == &value)
        {
            return invalid_world_creation(
                "aster.graph_cf",
                "Aster Graph CF did not contain every world chunk/manifest row-major and column-major row",
            );
        }
    }
    read_chunked_aster_record(
        &vault,
        &column,
        commit.seq,
        &row_key,
        &record.spec.calyx_document_id,
        &value,
    )
    .map(|mut readback| {
        readback.graph_rows = raw.len();
        readback.cell_key = hex_preview(&commit.cell_key);
        readback.index_key = hex_preview(&commit.index_key);
        readback
    })
}

fn read_chunked_aster_record(
    vault: &AsterVault<SystemClock>,
    column: &PlainColumn<SystemClock>,
    seq: u64,
    row_key: &[u8],
    calyx_document_id: &str,
    manifest_bytes: &[u8],
) -> Result<WorldAsterReadback, EngineError> {
    let manifest = parse_chunk_manifest(manifest_bytes, calyx_document_id)?;
    let mut chunks = Vec::with_capacity(manifest.chunk_count);
    for (index, column_name) in manifest.chunk_columns.iter().enumerate() {
        let chunk = column
            .get(seq, row_key, column_name.as_bytes())
            .map_err(|error| world_store_error("aster.chunk.get", error))?
            .ok_or_else(|| {
                world_store_error(
                    "aster.chunk.missing",
                    format!("manifest chunk {index} column {column_name} is absent"),
                )
            })?;
        chunks.push(chunk);
    }
    let bytes = assemble_chunks(&manifest, chunks)?;
    let document_fingerprint = verify_world_record_bytes(
        calyx_document_id,
        &bytes,
        Some(&manifest.document_fingerprint),
    )?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| world_store_error("aster.graph_scan", error))?;
    Ok(WorldAsterReadback {
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "manifest".to_string(),
        index_key: "manifest".to_string(),
        storage_schema: manifest.schema,
        chunk_count: manifest.chunk_count,
        sha256: manifest.sha256,
        document_fingerprint,
        manifest_bytes: manifest_bytes.len(),
        chunk_columns: manifest.chunk_columns,
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    if dir.as_os_str().is_empty() {
        return Err(world_store_error(
            "aster.path",
            "Aster vault directory must be non-empty",
        ));
    }
    if dir.is_file() {
        return Err(world_store_error(
            "aster.path",
            format!(
                "{} is a file; world state must be a Calyx Aster vault directory",
                dir.display()
            ),
        ));
    }
    let vault_id = WORLD_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| world_store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        WORLD_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| world_store_error("aster.open", error))
}

fn row_key(calyx_document_id: &str) -> Result<Vec<u8>, EngineError> {
    if calyx_document_id.trim().is_empty() {
        return invalid_world_creation("aster.row", "calyx_document_id must be non-empty");
    }
    Ok(format!("{WORLD_ASTER_ROW_PREFIX}{calyx_document_id}").into_bytes())
}

fn env_path(name: &str, default: PathBuf) -> Result<PathBuf, EngineError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => {
            invalid_world_creation("path.env", format!("{name} is empty"))
        }
        Ok(value) => Ok(absolute_or_workspace_relative(PathBuf::from(value))),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => invalid_world_creation("path.env", format!("{name}: {error}")),
    }
}

fn absolute_or_workspace_relative(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        workspace_root().join(path)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor must live under the workspace crates directory")
        .to_path_buf()
}

fn world_store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_creation.{}", field.into()),
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

#[cfg(test)]
#[path = "world_creation_store_tests.rs"]
mod tests;
