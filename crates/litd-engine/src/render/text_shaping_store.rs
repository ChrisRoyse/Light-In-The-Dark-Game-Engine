use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{ShapedTextRun, verify_shaped_text_run_integrity};
use crate::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

const SHAPED_TEXT_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCF";
const SHAPED_TEXT_VAULT_PURPOSE: &[u8] = b"litd-engine-shaped-text-issue1363";
const SHAPED_TEXT_COLLECTION: &str = "litd_engine_shaped_text";
const SHAPED_TEXT_ROW: &[u8] = b"text/shaped_run/latest";
const SHAPED_TEXT_LEGACY_COLUMN: &[u8] = b"run_json";
const SHAPED_TEXT_MANIFEST_COLUMN: &[u8] = b"run_manifest_json";
const SHAPED_TEXT_CHUNK_COLUMN_PREFIX: &str = "run_chunk";
const SHAPED_TEXT_CHUNK_SCHEMA: &str = "litd.render.text_shaping.chunks.v1";
const SHAPED_TEXT_LEGACY_SCHEMA: &str = "litd.render.text_shaping.legacy_single_cell.v1";
const SHAPED_TEXT_CHUNK_SIZE: usize = 256 * 1024;
const MAX_SHAPED_TEXT_JSON_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ShapedTextRunJsonReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub storage_schema: String,
    pub chunk_count: usize,
    pub run: ShapedTextRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ShapedTextChunkManifest {
    schema: String,
    total_bytes: usize,
    sha256: String,
    chunk_size: usize,
    chunk_count: usize,
    chunk_columns: Vec<String>,
}

pub fn write_shaped_text_run_json(
    path: impl AsRef<Path>,
    run: &ShapedTextRun,
) -> Result<ShapedTextRunJsonReadback, EngineError> {
    verify_shaped_text_run_integrity(run)?;
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = serde_json::to_vec_pretty(run)
        .map_err(|error| persistence_failed("serialize_shaped_text_json", error.to_string()))?;
    let manifest = build_chunk_manifest(&bytes)?;
    for (index, chunk) in bytes.chunks(SHAPED_TEXT_CHUNK_SIZE).enumerate() {
        let column = manifest.chunk_columns[index].as_bytes();
        let stored = write_calyx_json_bytes(&shaped_text_spec(path, column), chunk)
            .map_err(|error| persistence_failed("write_shaped_text_chunk", error))?;
        if stored.bytes != chunk {
            return Err(persistence_failed(
                "verify_shaped_text_chunk",
                format!("Aster shaped text chunk {index} did not match serialized JSON bytes"),
            ));
        }
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| persistence_failed("serialize_shaped_text_manifest", error.to_string()))?;
    let stored = write_calyx_json_bytes(
        &shaped_text_spec(path, SHAPED_TEXT_MANIFEST_COLUMN),
        &manifest_bytes,
    )
    .map_err(|error| persistence_failed("write_shaped_text_manifest", error))?;
    if stored.bytes != manifest_bytes {
        return Err(persistence_failed(
            "verify_shaped_text_manifest",
            "Aster shaped text manifest did not match serialized manifest bytes",
        ));
    }
    let readback = load_shaped_text_run_json(path)?;
    if readback.run != *run {
        return Err(EngineError::TextShapingReadbackMismatch {
            field: "shaped_text_json".to_string(),
            expected: run.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_shaped_text_run_json(
    path: impl AsRef<Path>,
) -> Result<ShapedTextRunJsonReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let manifest = read_calyx_json_bytes(&shaped_text_spec(path, SHAPED_TEXT_MANIFEST_COLUMN))
        .map_err(|error| persistence_failed("read_shaped_text_manifest", error))?;
    let (bytes, storage_schema, chunk_count) = if let Some(readback) = manifest {
        let manifest = parse_chunk_manifest(&readback.bytes)?;
        let chunks = manifest
            .chunk_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                read_calyx_json_bytes(&shaped_text_spec(path, column.as_bytes()))
                    .map_err(|error| persistence_failed("read_shaped_text_chunk", error))?
                    .map(|readback| readback.bytes)
                    .ok_or_else(|| {
                        persistence_failed(
                            "read_shaped_text_chunk",
                            format!("manifest chunk {index} column {column} is absent"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bytes = assemble_chunks(&manifest, chunks)?;
        (bytes, manifest.schema, manifest.chunk_count)
    } else {
        let readback = read_calyx_json_bytes(&shaped_text_spec(path, SHAPED_TEXT_LEGACY_COLUMN))
            .map_err(|error| persistence_failed("read_legacy_shaped_text_json", error))?
            .ok_or_else(|| {
                persistence_failed(
                    "read_shaped_text_json",
                    format!("missing Calyx Aster shaped text row at {}", path.display()),
                )
            })?;
        (readback.bytes, SHAPED_TEXT_LEGACY_SCHEMA.to_string(), 1)
    };
    let run = serde_json::from_slice::<ShapedTextRun>(&bytes).map_err(|error| {
        persistence_failed(
            "deserialize_shaped_text_json",
            format!("{}: {error}", path.display()),
        )
    })?;
    verify_shaped_text_run_integrity(&run)?;
    Ok(ShapedTextRunJsonReadback {
        path: path.to_path_buf(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        fnv1a64: run.fnv1a64.clone(),
        storage_schema,
        chunk_count,
        run,
    })
}

fn build_chunk_manifest(bytes: &[u8]) -> Result<ShapedTextChunkManifest, EngineError> {
    if bytes.is_empty() || bytes.len() > MAX_SHAPED_TEXT_JSON_BYTES {
        return Err(persistence_failed(
            "shaped_text_manifest.total_bytes",
            format!(
                "serialized shaped text must be 1..={MAX_SHAPED_TEXT_JSON_BYTES} bytes, got {}",
                bytes.len()
            ),
        ));
    }
    let sha256 = sha256_hex(bytes);
    let chunk_count = bytes.chunks(SHAPED_TEXT_CHUNK_SIZE).count();
    let manifest = ShapedTextChunkManifest {
        schema: SHAPED_TEXT_CHUNK_SCHEMA.to_string(),
        total_bytes: bytes.len(),
        sha256: sha256.clone(),
        chunk_size: SHAPED_TEXT_CHUNK_SIZE,
        chunk_count,
        chunk_columns: (0..chunk_count)
            .map(|index| chunk_column_name(&sha256, index))
            .collect(),
    };
    manifest.validate()?;
    Ok(manifest)
}

fn parse_chunk_manifest(bytes: &[u8]) -> Result<ShapedTextChunkManifest, EngineError> {
    let manifest = serde_json::from_slice::<ShapedTextChunkManifest>(bytes)
        .map_err(|error| persistence_failed("parse_shaped_text_manifest", error.to_string()))?;
    manifest.validate()?;
    Ok(manifest)
}

fn assemble_chunks(
    manifest: &ShapedTextChunkManifest,
    chunks: Vec<Vec<u8>>,
) -> Result<Vec<u8>, EngineError> {
    if chunks.len() != manifest.chunk_count {
        return Err(persistence_failed(
            "shaped_text_chunks.count",
            format!(
                "manifest expected {} chunks, read {}",
                manifest.chunk_count,
                chunks.len()
            ),
        ));
    }
    let mut bytes = Vec::with_capacity(manifest.total_bytes);
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.is_empty()
            || chunk.len() > manifest.chunk_size
            || (index + 1 < chunks.len() && chunk.len() != manifest.chunk_size)
        {
            return Err(persistence_failed(
                "shaped_text_chunks.size",
                format!("chunk {index} has invalid size {}", chunk.len()),
            ));
        }
        bytes.extend_from_slice(chunk);
    }
    if bytes.len() != manifest.total_bytes {
        return Err(persistence_failed(
            "shaped_text_chunks.total_bytes",
            format!(
                "reassembled {} bytes, expected {}",
                bytes.len(),
                manifest.total_bytes
            ),
        ));
    }
    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != manifest.sha256 {
        return Err(persistence_failed(
            "shaped_text_chunks.sha256",
            format!(
                "reassembled sha256 {actual_sha256} != manifest {}",
                manifest.sha256
            ),
        ));
    }
    Ok(bytes)
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "shaped_text_path",
            "path must be non-empty",
        ));
    }
    Ok(())
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::TextShapingPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}

fn shaped_text_spec<'a>(path: &'a Path, column: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: SHAPED_TEXT_VAULT_ID,
        purpose: SHAPED_TEXT_VAULT_PURPOSE,
        collection: SHAPED_TEXT_COLLECTION,
        row_key: SHAPED_TEXT_ROW,
        column,
    }
}

fn chunk_column_name(sha256: &str, index: usize) -> String {
    format!("{SHAPED_TEXT_CHUNK_COLUMN_PREFIX}/{sha256}/{index:08}")
}

impl ShapedTextChunkManifest {
    fn validate(&self) -> Result<(), EngineError> {
        let valid_sha256 = self.sha256.len() == 64
            && self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        let expected_count = self.total_bytes.div_ceil(SHAPED_TEXT_CHUNK_SIZE);
        if self.schema != SHAPED_TEXT_CHUNK_SCHEMA
            || self.total_bytes == 0
            || self.total_bytes > MAX_SHAPED_TEXT_JSON_BYTES
            || !valid_sha256
            || self.chunk_size != SHAPED_TEXT_CHUNK_SIZE
            || self.chunk_count == 0
            || self.chunk_count != expected_count
            || self.chunk_count != self.chunk_columns.len()
        {
            return Err(persistence_failed(
                "shaped_text_manifest",
                "chunk manifest schema, sizes, count, or sha256 is invalid",
            ));
        }
        for (index, column) in self.chunk_columns.iter().enumerate() {
            let expected = chunk_column_name(&self.sha256, index);
            if column != &expected {
                return Err(persistence_failed(
                    "shaped_text_manifest.chunk_column",
                    format!("chunk column {index} was {column}, expected {expected}"),
                ));
            }
        }
        Ok(())
    }
}
