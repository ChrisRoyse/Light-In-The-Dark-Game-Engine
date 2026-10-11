//! Engine-side world chunk streaming from verified Aster world records.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    thread::{self, JoinHandle},
};

use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec3};

use super::world_store::{RuntimeWorldChunk, WorldSnapshot, read_world_snapshot_from_aster};

pub const WORLD_STREAM_SCHEMA: &str = "litd.engine.world_stream.v1";
pub const WORLD_ASTER_CHUNK_SCHEMA: &str = "litd.editor.world_record.chunks.v1";
const MAX_STREAM_RADIUS: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorldChunkCoord {
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkStreamConfig {
    pub spatial_chunk_tiles: u32,
    pub load_radius_chunks: u32,
    pub unload_radius_chunks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldStreamAnchor {
    pub position: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkStreamReadback {
    pub active_chunks: Vec<WorldChunkCoord>,
    pub resident_cell_count: usize,
    pub resident_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldStreamSourceReadback {
    pub schema: String,
    pub calyx_document_id: String,
    pub source_sha256: String,
    pub document_fingerprint: String,
    pub storage_schema: String,
    pub storage_chunk_count: usize,
    pub storage_bytes: usize,
    pub graph_rows: usize,
    pub world_width_tiles: u32,
    pub world_height_tiles: u32,
    pub spatial_chunk_tiles: u32,
    pub source_chunk_count: usize,
    pub source_cell_count: usize,
    pub loaded_on_background_thread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkStreamUpdate {
    pub anchor_chunk: WorldChunkCoord,
    pub before: WorldChunkStreamReadback,
    pub after: WorldChunkStreamReadback,
    pub expected_active_chunks: Vec<WorldChunkCoord>,
    pub loaded_chunks: Vec<WorldChunkCoord>,
    pub unloaded_chunks: Vec<WorldChunkCoord>,
}

#[derive(Debug, Clone)]
pub struct WorldChunkStreamLoadRequest {
    pub aster_dir: PathBuf,
    pub calyx_document_id: String,
    pub config: WorldChunkStreamConfig,
}

#[derive(Debug)]
pub struct WorldChunkStreamLoadJob {
    join: JoinHandle<Result<WorldStreamer, EngineError>>,
}

#[derive(Debug, Clone)]
pub struct WorldStreamer {
    snapshot: WorldSnapshot,
    config: WorldChunkStreamConfig,
    active: BTreeMap<WorldChunkCoord, RuntimeWorldChunk>,
}

impl Default for WorldChunkStreamConfig {
    fn default() -> Self {
        Self {
            spatial_chunk_tiles: 16,
            load_radius_chunks: 1,
            unload_radius_chunks: 1,
        }
    }
}

impl WorldChunkStreamConfig {
    pub fn validate(self) -> Result<(), EngineError> {
        if self.spatial_chunk_tiles == 0 {
            return stream_invalid("config.spatial_chunk_tiles", "must be greater than zero");
        }
        if self.load_radius_chunks > MAX_STREAM_RADIUS
            || self.unload_radius_chunks > MAX_STREAM_RADIUS
            || self.unload_radius_chunks < self.load_radius_chunks
        {
            return stream_invalid(
                "config.radius",
                "load/unload radii must be bounded and unload >= load",
            );
        }
        Ok(())
    }
}

impl WorldChunkStreamLoadRequest {
    pub fn new(
        aster_dir: impl Into<PathBuf>,
        calyx_document_id: impl Into<String>,
        config: WorldChunkStreamConfig,
    ) -> Self {
        Self {
            aster_dir: aster_dir.into(),
            calyx_document_id: calyx_document_id.into(),
            config,
        }
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.aster_dir.as_os_str().is_empty() || self.calyx_document_id.trim().is_empty() {
            return stream_invalid(
                "load_request",
                "aster_dir and calyx_document_id must be non-empty",
            );
        }
        self.config.validate()
    }
}

impl WorldChunkStreamLoadJob {
    pub fn spawn(request: WorldChunkStreamLoadRequest) -> Self {
        Self {
            join: thread::spawn(move || WorldStreamer::from_aster_inner(request, true)),
        }
    }

    pub fn finish(self) -> Result<WorldStreamer, EngineError> {
        self.join.join().map_err(|_| {
            stream_persistence("load_job.join", "world chunk stream worker panicked")
        })?
    }
}

impl WorldStreamer {
    pub fn from_aster(request: WorldChunkStreamLoadRequest) -> Result<Self, EngineError> {
        Self::from_aster_inner(request, false)
    }

    pub fn load_background(request: WorldChunkStreamLoadRequest) -> WorldChunkStreamLoadJob {
        WorldChunkStreamLoadJob::spawn(request)
    }

    pub fn update_anchor(
        &mut self,
        anchor: WorldStreamAnchor,
    ) -> Result<WorldChunkStreamUpdate, EngineError> {
        let before = self.state_readback();
        let anchor_chunk = self.anchor_chunk(anchor)?;
        let load_set = self.chunk_set_within(anchor_chunk, self.config.load_radius_chunks);
        let keep_set = self.chunk_set_within(anchor_chunk, self.config.unload_radius_chunks);
        let active_before = self.active.keys().copied().collect::<Vec<_>>();
        let mut unloaded = Vec::new();
        for coord in active_before {
            if !keep_set.contains(&coord) {
                self.active.remove(&coord);
                unloaded.push(coord);
            }
        }
        let mut loaded = Vec::new();
        for coord in &load_set {
            if !self.active.contains_key(coord) {
                let source = self.snapshot.chunks.get(coord).ok_or_else(|| {
                    EngineError::StreamReadbackMismatch {
                        field: "world_chunk.source".to_string(),
                        expected: format!("{coord:?}"),
                        actual: "missing".to_string(),
                    }
                })?;
                self.active.insert(*coord, source.load());
                loaded.push(*coord);
            }
        }
        Ok(WorldChunkStreamUpdate {
            anchor_chunk,
            before,
            after: self.state_readback(),
            expected_active_chunks: load_set.into_iter().collect(),
            loaded_chunks: loaded,
            unloaded_chunks: unloaded,
        })
    }

    pub fn state_readback(&self) -> WorldChunkStreamReadback {
        let resident_cell_count = self.active.values().map(|chunk| chunk.cells.len()).sum();
        let resident_bytes = self.active.values().map(|chunk| chunk.resident_bytes).sum();
        WorldChunkStreamReadback {
            active_chunks: self.active.keys().copied().collect(),
            resident_cell_count,
            resident_bytes,
        }
    }

    pub fn source_readback(&self) -> WorldStreamSourceReadback {
        WorldStreamSourceReadback {
            schema: WORLD_STREAM_SCHEMA.to_string(),
            calyx_document_id: self.snapshot.document_id.clone(),
            source_sha256: self.snapshot.source_sha256.clone(),
            document_fingerprint: self.snapshot.document_fingerprint.clone(),
            storage_schema: self.snapshot.storage_schema.clone(),
            storage_chunk_count: self.snapshot.storage_chunk_count,
            storage_bytes: self.snapshot.storage_bytes,
            graph_rows: self.snapshot.graph_rows,
            world_width_tiles: self.snapshot.width_tiles,
            world_height_tiles: self.snapshot.height_tiles,
            spatial_chunk_tiles: self.config.spatial_chunk_tiles,
            source_chunk_count: self.snapshot.chunks.len(),
            source_cell_count: self
                .snapshot
                .chunks
                .values()
                .map(|chunk| chunk.cells.len())
                .sum(),
            loaded_on_background_thread: self.snapshot.loaded_on_background_thread,
        }
    }

    fn from_aster_inner(
        request: WorldChunkStreamLoadRequest,
        loaded_on_background_thread: bool,
    ) -> Result<Self, EngineError> {
        request.validate()?;
        let snapshot = read_world_snapshot_from_aster(&request, loaded_on_background_thread)?;
        Ok(Self {
            snapshot,
            config: request.config,
            active: BTreeMap::new(),
        })
    }

    fn anchor_chunk(&self, anchor: WorldStreamAnchor) -> Result<WorldChunkCoord, EngineError> {
        if !anchor.position.is_finite() || anchor.position.x < 0.0 || anchor.position.z < 0.0 {
            return stream_invalid(
                "anchor.position",
                "position must be finite and non-negative",
            );
        }
        if anchor.position.x >= self.snapshot.width_tiles as f32
            || anchor.position.z >= self.snapshot.height_tiles as f32
        {
            return stream_invalid("anchor.position", "position must stay inside world bounds");
        }
        let tile_x = anchor.position.x.floor() as i32;
        let tile_z = anchor.position.z.floor() as i32;
        Ok(coord_for_tile(
            tile_x,
            tile_z,
            self.config.spatial_chunk_tiles,
        ))
    }

    fn chunk_set_within(&self, center: WorldChunkCoord, radius: u32) -> BTreeSet<WorldChunkCoord> {
        self.snapshot
            .chunks
            .keys()
            .copied()
            .filter(|coord| {
                let dx = coord.x.abs_diff(center.x);
                let dz = coord.z.abs_diff(center.z);
                dx.max(dz) <= radius
            })
            .collect()
    }
}

pub(super) fn coord_for_tile(
    tile_x: i32,
    tile_z: i32,
    spatial_chunk_tiles: u32,
) -> WorldChunkCoord {
    let size = spatial_chunk_tiles as i32;
    WorldChunkCoord {
        x: tile_x.div_euclid(size),
        z: tile_z.div_euclid(size),
    }
}

pub(super) fn stream_invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::StreamInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

pub(super) fn stream_persistence(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::StreamPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}

pub(super) fn stream_readback<T>(
    field: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::StreamReadbackMismatch {
        field: field.into(),
        expected: expected.into(),
        actual: actual.into(),
    })
}

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
