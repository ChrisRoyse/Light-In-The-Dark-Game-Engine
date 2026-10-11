use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{EditorWorldRecord, invalid_world_creation, validate_world_record};

pub(super) const WORLD_ASTER_MANIFEST_COLUMN: &[u8] = b"world_record_manifest_json";
pub(super) const WORLD_ASTER_CHUNK_COLUMN_PREFIX: &str = "world_record_chunk";
pub(super) const WORLD_ASTER_CHUNK_SCHEMA: &str = "litd.editor.world_record.chunks.v1";
pub(super) const WORLD_ASTER_LEGACY_SCHEMA: &str = "litd.editor.world_record.legacy_single_cell.v1";
pub(super) const WORLD_ASTER_CHUNK_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct WorldRecordChunkManifest {
    pub schema: String,
    pub calyx_document_id: String,
    pub document_fingerprint: String,
    pub total_bytes: usize,
    pub sha256: String,
    pub chunk_size: usize,
    pub chunk_count: usize,
    pub chunk_columns: Vec<String>,
}

pub(super) fn build_chunk_manifest(
    record: &EditorWorldRecord,
    bytes: &[u8],
) -> Result<WorldRecordChunkManifest, EngineError> {
    validate_world_record(record)?;
    if bytes.is_empty() {
        return invalid_world_creation("aster.chunk.bytes", "world record bytes must be non-empty");
    }
    let sha256 = sha256_hex(bytes);
    let chunk_count = bytes.chunks(WORLD_ASTER_CHUNK_SIZE).count();
    let manifest = WorldRecordChunkManifest {
        schema: WORLD_ASTER_CHUNK_SCHEMA.to_string(),
        calyx_document_id: record.spec.calyx_document_id.clone(),
        document_fingerprint: record.document_fingerprint.clone(),
        total_bytes: bytes.len(),
        sha256: sha256.clone(),
        chunk_size: WORLD_ASTER_CHUNK_SIZE,
        chunk_count,
        chunk_columns: (0..chunk_count)
            .map(|index| chunk_column_name(&sha256, index))
            .collect(),
    };
    manifest.validate(&record.spec.calyx_document_id)?;
    Ok(manifest)
}

pub(super) fn manifest_to_bytes(
    manifest: &WorldRecordChunkManifest,
) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(manifest).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize world chunk manifest: {error}"),
    })
}

pub(super) fn parse_chunk_manifest(
    bytes: &[u8],
    calyx_document_id: &str,
) -> Result<WorldRecordChunkManifest, EngineError> {
    let manifest = serde_json::from_slice::<WorldRecordChunkManifest>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse world chunk manifest: {error}"),
        }
    })?;
    manifest.validate(calyx_document_id)?;
    Ok(manifest)
}

pub(super) fn chunk_column_name(sha256: &str, index: usize) -> String {
    format!("{WORLD_ASTER_CHUNK_COLUMN_PREFIX}/{sha256}/{index:08}")
}

pub(super) fn assemble_chunks(
    manifest: &WorldRecordChunkManifest,
    chunks: Vec<Vec<u8>>,
) -> Result<Vec<u8>, EngineError> {
    if chunks.len() != manifest.chunk_count {
        return invalid_world_creation(
            "aster.chunk.count",
            format!(
                "manifest expected {} chunks but read {}",
                manifest.chunk_count,
                chunks.len()
            ),
        );
    }
    let mut bytes = Vec::with_capacity(manifest.total_bytes);
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.is_empty() {
            return invalid_world_creation("aster.chunk.empty", format!("chunk {index} is empty"));
        }
        if chunk.len() > manifest.chunk_size {
            return invalid_world_creation(
                "aster.chunk.size",
                format!(
                    "chunk {index} has {} bytes, over manifest chunk_size {}",
                    chunk.len(),
                    manifest.chunk_size
                ),
            );
        }
        if index + 1 < manifest.chunk_count && chunk.len() != manifest.chunk_size {
            return invalid_world_creation(
                "aster.chunk.size",
                format!(
                    "non-final chunk {index} has {} bytes, expected {}",
                    chunk.len(),
                    manifest.chunk_size
                ),
            );
        }
        bytes.extend_from_slice(chunk);
    }
    if bytes.len() != manifest.total_bytes {
        return invalid_world_creation(
            "aster.chunk.total_bytes",
            format!(
                "reassembled {} bytes, manifest expected {}",
                bytes.len(),
                manifest.total_bytes
            ),
        );
    }
    let actual_sha = sha256_hex(&bytes);
    if actual_sha != manifest.sha256 {
        return invalid_world_creation(
            "aster.chunk.sha256",
            format!(
                "reassembled sha256 {actual_sha} != manifest {}",
                manifest.sha256
            ),
        );
    }
    Ok(bytes)
}

pub(super) fn verify_world_record_bytes(
    calyx_document_id: &str,
    bytes: &[u8],
    expected_fingerprint: Option<&str>,
) -> Result<String, EngineError> {
    let record = serde_json::from_slice::<EditorWorldRecord>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse world record readback: {error}"),
        }
    })?;
    validate_world_record(&record)?;
    if record.spec.calyx_document_id != calyx_document_id {
        return invalid_world_creation(
            "aster.readback.document_id",
            format!(
                "read {} while requesting {}",
                record.spec.calyx_document_id, calyx_document_id
            ),
        );
    }
    if let Some(expected) = expected_fingerprint
        && record.document_fingerprint != expected
    {
        return invalid_world_creation(
            "aster.readback.fingerprint",
            format!(
                "record fingerprint {} != manifest {}",
                record.document_fingerprint, expected
            ),
        );
    }
    Ok(record.document_fingerprint)
}

impl WorldRecordChunkManifest {
    fn validate(&self, requested_document_id: &str) -> Result<(), EngineError> {
        if self.schema != WORLD_ASTER_CHUNK_SCHEMA {
            return invalid_world_creation(
                "aster.manifest.schema",
                format!("unsupported world chunk manifest schema {}", self.schema),
            );
        }
        if self.calyx_document_id != requested_document_id {
            return invalid_world_creation(
                "aster.manifest.document_id",
                format!(
                    "manifest document id {} != requested {}",
                    self.calyx_document_id, requested_document_id
                ),
            );
        }
        if self.document_fingerprint.trim().is_empty() {
            return invalid_world_creation(
                "aster.manifest.document_fingerprint",
                "document fingerprint must be non-empty",
            );
        }
        if self.total_bytes == 0 {
            return invalid_world_creation(
                "aster.manifest.total_bytes",
                "total_bytes must be non-zero",
            );
        }
        if !is_sha256_hex(&self.sha256) {
            return invalid_world_creation(
                "aster.manifest.sha256",
                "sha256 must be 64 lowercase hex characters",
            );
        }
        if self.chunk_size == 0 || self.chunk_size > (1 << 20) {
            return invalid_world_creation(
                "aster.manifest.chunk_size",
                "chunk_size must be 1..=1048576 bytes",
            );
        }
        if self.chunk_count == 0 || self.chunk_count != self.chunk_columns.len() {
            return invalid_world_creation(
                "aster.manifest.chunk_count",
                format!(
                    "chunk_count {} does not match {} chunk columns",
                    self.chunk_count,
                    self.chunk_columns.len()
                ),
            );
        }
        for (index, column) in self.chunk_columns.iter().enumerate() {
            let expected = chunk_column_name(&self.sha256, index);
            if column != &expected {
                return invalid_world_creation(
                    "aster.manifest.chunk_column",
                    format!("chunk column {index} was {column}, expected {expected}"),
                );
            }
        }
        Ok(())
    }
}

use litd_hash::is_sha256_hex;
