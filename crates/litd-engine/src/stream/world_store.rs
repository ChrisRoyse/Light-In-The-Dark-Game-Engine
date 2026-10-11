use std::{collections::BTreeMap, path::Path};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::world::{
    WORLD_ASTER_CHUNK_SCHEMA, WorldChunkCoord, WorldChunkStreamLoadRequest, coord_for_tile,
    stream_invalid, stream_persistence, stream_readback,
};

pub(super) const WORLD_ASTER_COLLECTION: &str = "litd_editor_worlds";
const WORLD_ASTER_ROW_PREFIX: &str = "world/";
pub(super) const WORLD_ASTER_MANIFEST_COLUMN: &[u8] = b"world_record_manifest_json";
const WORLD_ASTER_CHUNK_PREFIX: &str = "world_record_chunk";
const WORLD_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const WORLD_VAULT_PURPOSE: &[u8] = b"litd-editor-world-creation-issue1210";

#[derive(Debug, Clone)]
pub(super) struct WorldSnapshot {
    pub document_id: String,
    pub source_sha256: String,
    pub document_fingerprint: String,
    pub storage_schema: String,
    pub storage_chunk_count: usize,
    pub storage_bytes: usize,
    pub graph_rows: usize,
    pub width_tiles: u32,
    pub height_tiles: u32,
    pub loaded_on_background_thread: bool,
    pub chunks: BTreeMap<WorldChunkCoord, WorldChunkSource>,
}

#[derive(Debug, Clone)]
pub(super) struct WorldChunkSource {
    pub cells: Vec<RuntimeTerrainCell>,
    pub source_bytes: usize,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeWorldChunk {
    pub cells: Vec<RuntimeTerrainCell>,
    pub resident_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct RuntimeTerrainCell {
    id: String,
    tile_x: i32,
    tile_z: i32,
    tile_id: String,
    cliff_type: String,
    cliff_level: i16,
    height: f32,
}

#[derive(Debug, Deserialize)]
struct RuntimeWorldRecord {
    record_kind: String,
    spec: RuntimeWorldSpec,
    document: RuntimeEditorDocument,
    document_fingerprint: String,
    terrain_cell_count: usize,
}

#[derive(Debug, Deserialize)]
struct RuntimeWorldSpec {
    calyx_document_id: String,
    properties: RuntimeWorldProperties,
}

#[derive(Debug, Deserialize)]
struct RuntimeWorldProperties {
    width_tiles: u32,
    height_tiles: u32,
}

#[derive(Debug, Deserialize)]
struct RuntimeEditorDocument {
    terrain_cells: BTreeMap<String, RuntimeTerrainCell>,
}

#[derive(Debug, Deserialize, Serialize)]
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

impl WorldChunkSource {
    pub fn load(&self) -> RuntimeWorldChunk {
        RuntimeWorldChunk {
            cells: self.cells.clone(),
            resident_bytes: self.source_bytes,
        }
    }
}

pub(super) fn read_world_snapshot_from_aster(
    request: &WorldChunkStreamLoadRequest,
    loaded_on_background_thread: bool,
) -> Result<WorldSnapshot, EngineError> {
    let vault = open_aster(&request.aster_dir)?;
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION)
        .map_err(|error| stream_persistence("aster.column", error.to_string()))?;
    let seq = vault.latest_seq();
    let row_key = row_key(&request.calyx_document_id)?;
    let manifest_bytes = column
        .get(seq, &row_key, WORLD_ASTER_MANIFEST_COLUMN)
        .map_err(|error| stream_persistence("aster.manifest.get", error.to_string()))?
        .ok_or_else(|| {
            stream_persistence(
                "aster.manifest.missing",
                "world_record_manifest_json is absent",
            )
        })?;
    let manifest = parse_manifest(&manifest_bytes, &request.calyx_document_id)?;
    let mut chunks = Vec::with_capacity(manifest.chunk_count);
    for (index, column_name) in manifest.chunk_columns.iter().enumerate() {
        let chunk = column
            .get(seq, &row_key, column_name.as_bytes())
            .map_err(|error| stream_persistence("aster.chunk.get", error.to_string()))?
            .ok_or_else(|| {
                stream_persistence(
                    "aster.chunk.missing",
                    format!("manifest chunk {index} column {column_name} is absent"),
                )
            })?;
        chunks.push(chunk);
    }
    let bytes = assemble_chunks(&manifest, chunks)?;
    let source_sha256 = sha256_hex(&bytes);
    let record = parse_runtime_world_record(&bytes, &request.calyx_document_id)?;
    let graph_rows = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| stream_persistence("aster.graph_scan", error.to_string()))?
        .len();
    let chunks = build_spatial_chunks(&record, request.config.spatial_chunk_tiles)?;
    Ok(WorldSnapshot {
        document_id: request.calyx_document_id.clone(),
        source_sha256,
        document_fingerprint: record.document_fingerprint,
        storage_schema: manifest.schema,
        storage_chunk_count: manifest.chunk_count,
        storage_bytes: bytes.len(),
        graph_rows,
        width_tiles: record.spec.properties.width_tiles,
        height_tiles: record.spec.properties.height_tiles,
        loaded_on_background_thread,
        chunks,
    })
}

fn parse_manifest(
    bytes: &[u8],
    calyx_document_id: &str,
) -> Result<WorldRecordChunkManifest, EngineError> {
    let manifest = serde_json::from_slice::<WorldRecordChunkManifest>(bytes)
        .map_err(|error| stream_persistence("aster.manifest.parse", error.to_string()))?;
    manifest.validate(calyx_document_id)?;
    Ok(manifest)
}

fn assemble_chunks(
    manifest: &WorldRecordChunkManifest,
    chunks: Vec<Vec<u8>>,
) -> Result<Vec<u8>, EngineError> {
    if chunks.len() != manifest.chunk_count {
        return stream_readback(
            "aster.chunk.count",
            manifest.chunk_count.to_string(),
            chunks.len().to_string(),
        );
    }
    let mut bytes = Vec::with_capacity(manifest.total_bytes);
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.is_empty() || chunk.len() > manifest.chunk_size {
            return stream_invalid("aster.chunk.size", format!("chunk {index} size is invalid"));
        }
        if index + 1 < manifest.chunk_count && chunk.len() != manifest.chunk_size {
            return stream_readback(
                "aster.chunk.size",
                manifest.chunk_size.to_string(),
                chunk.len().to_string(),
            );
        }
        bytes.extend_from_slice(chunk);
    }
    if bytes.len() != manifest.total_bytes {
        return stream_readback(
            "aster.total_bytes",
            manifest.total_bytes.to_string(),
            bytes.len().to_string(),
        );
    }
    let actual = sha256_hex(&bytes);
    if actual != manifest.sha256 {
        return stream_readback("aster.sha256", manifest.sha256.clone(), actual);
    }
    Ok(bytes)
}

fn parse_runtime_world_record(
    bytes: &[u8],
    calyx_document_id: &str,
) -> Result<RuntimeWorldRecord, EngineError> {
    let record = serde_json::from_slice::<RuntimeWorldRecord>(bytes)
        .map_err(|error| stream_persistence("world_record.parse", error.to_string()))?;
    if record.record_kind != "litd.editor.world_record"
        || record.spec.calyx_document_id != calyx_document_id
        || record.document_fingerprint.trim().is_empty()
    {
        return stream_invalid("world_record.identity", "world record identity is invalid");
    }
    if record.spec.properties.width_tiles == 0 || record.spec.properties.height_tiles == 0 {
        return stream_invalid(
            "world_record.dimensions",
            "world dimensions must be non-zero",
        );
    }
    if record.terrain_cell_count != record.document.terrain_cells.len()
        || record.document.terrain_cells.is_empty()
    {
        return stream_invalid("world_record.terrain_cells", "terrain cell count drifted");
    }
    Ok(record)
}

fn build_spatial_chunks(
    record: &RuntimeWorldRecord,
    spatial_chunk_tiles: u32,
) -> Result<BTreeMap<WorldChunkCoord, WorldChunkSource>, EngineError> {
    let mut coords = std::collections::BTreeSet::new();
    let mut chunks = BTreeMap::<WorldChunkCoord, Vec<RuntimeTerrainCell>>::new();
    for (id, cell) in &record.document.terrain_cells {
        validate_cell(id, cell, record)?;
        if !coords.insert((cell.tile_x, cell.tile_z)) {
            return stream_invalid(
                "world_record.terrain_cells.coord",
                format!("duplicate coordinate {},{}", cell.tile_x, cell.tile_z),
            );
        }
        chunks
            .entry(coord_for_tile(
                cell.tile_x,
                cell.tile_z,
                spatial_chunk_tiles,
            ))
            .or_default()
            .push(cell.clone());
    }
    chunks
        .into_iter()
        .map(|(coord, cells)| {
            let source_bytes = serde_json::to_vec(&cells)
                .map_err(|error| stream_persistence("world_chunk.serialize", error.to_string()))?
                .len();
            Ok((
                coord,
                WorldChunkSource {
                    cells,
                    source_bytes,
                },
            ))
        })
        .collect()
}

fn validate_cell(
    id: &str,
    cell: &RuntimeTerrainCell,
    record: &RuntimeWorldRecord,
) -> Result<(), EngineError> {
    if id != cell.id
        || cell.id.trim().is_empty()
        || cell.tile_id.trim().is_empty()
        || cell.cliff_type.trim().is_empty()
        || !cell.height.is_finite()
        || cell.tile_x < 0
        || cell.tile_z < 0
        || cell.tile_x >= record.spec.properties.width_tiles as i32
        || cell.tile_z >= record.spec.properties.height_tiles as i32
    {
        return stream_invalid("world_record.terrain_cell", "terrain cell is invalid");
    }
    Ok(())
}

pub(super) fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = WORLD_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| stream_persistence("aster.vault_id", error.to_string()))?;
    AsterVault::open(
        dir,
        vault_id,
        WORLD_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| stream_persistence("aster.open", error.to_string()))
}

pub(super) fn row_key(calyx_document_id: &str) -> Result<Vec<u8>, EngineError> {
    if calyx_document_id.trim().is_empty() {
        return stream_invalid("aster.row", "calyx_document_id must be non-empty");
    }
    Ok(format!("{WORLD_ASTER_ROW_PREFIX}{calyx_document_id}").into_bytes())
}

impl WorldRecordChunkManifest {
    fn validate(&self, requested_document_id: &str) -> Result<(), EngineError> {
        if self.schema != WORLD_ASTER_CHUNK_SCHEMA
            || self.calyx_document_id != requested_document_id
            || self.document_fingerprint.trim().is_empty()
            || self.total_bytes == 0
            || !is_sha256_hex(&self.sha256)
            || self.chunk_size == 0
            || self.chunk_count == 0
            || self.chunk_count != self.chunk_columns.len()
        {
            return stream_invalid("aster.manifest", "world chunk manifest is invalid");
        }
        for (index, column) in self.chunk_columns.iter().enumerate() {
            let expected = chunk_column_name(&self.sha256, index);
            if column != &expected {
                return stream_invalid("aster.manifest.chunk_column", "chunk column drifted");
            }
        }
        Ok(())
    }
}

pub(super) fn chunk_column_name(sha256: &str, index: usize) -> String {
    format!("{WORLD_ASTER_CHUNK_PREFIX}/{sha256}/{index:08}")
}

use litd_hash::is_sha256_hex;
