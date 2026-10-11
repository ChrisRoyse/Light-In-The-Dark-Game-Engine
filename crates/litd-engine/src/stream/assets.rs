//! Engine-side asset LOD streaming from verified editor asset_lod products.

use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
};

use litd_hash::{fnv1a64_hex, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::{
    asset_stream_error::{
        invalid as asset_stream_invalid, persistence as asset_stream_persistence,
        readback as asset_stream_readback,
    },
    assets_product::{
        AssetLodProduct, RuntimeAssetLodLevel, parse_asset_lod_product, validate_level_files,
    },
};

pub const ASSET_LOD_STREAM_SCHEMA: &str = "litd.engine.asset_lod_stream.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLodDistanceBand {
    pub level_index: usize,
    pub max_distance_millimeters: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLodStreamConfig {
    pub distance_bands: Vec<AssetLodDistanceBand>,
}

#[derive(Debug, Clone)]
pub struct AssetLodStreamLoadRequest {
    pub manifest_path: PathBuf,
    pub config: AssetLodStreamConfig,
}

#[derive(Debug)]
pub struct AssetLodStreamLoadJob {
    join: JoinHandle<Result<RuntimeAssetLod, EngineError>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLodStreamSourceReadback {
    pub schema: String,
    pub product_schema: String,
    pub product_id: String,
    pub manifest_path: String,
    pub manifest_bytes: u64,
    pub manifest_sha256: String,
    pub level_count: usize,
    pub total_index_buffer_bytes: u64,
    pub distance_bands: Vec<AssetLodDistanceBand>,
    pub loaded_on_background_thread: bool,
    pub request_thread_id: String,
    pub worker_thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLodSelection {
    pub product_id: String,
    pub level_index: usize,
    pub level_id: String,
    pub distance_millimeters: u64,
    pub max_distance_millimeters: u64,
    pub index_buffer_path: PathBuf,
    pub expected_bytes: u64,
    pub expected_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLodLoadReadback {
    pub selection: AssetLodSelection,
    pub bytes_read: u64,
    pub verified_sha256: String,
    pub bytes_fnv1a64: String,
    pub loaded_on_background_thread: bool,
    pub request_thread_id: String,
    pub worker_thread_id: String,
}

#[derive(Debug, Clone)]
pub struct AssetLodLoadReceipt {
    pub readback: AssetLodLoadReadback,
    pub bytes: Arc<[u8]>,
}

#[derive(Debug)]
pub struct AssetLodLoadJob {
    join: JoinHandle<Result<AssetLodLoadReceipt, EngineError>>,
}

#[derive(Debug, Clone)]
pub struct RuntimeAssetLod {
    product: AssetLodProduct,
    levels: Vec<RuntimeAssetLodLevel>,
    config: AssetLodStreamConfig,
    source_readback: AssetLodStreamSourceReadback,
}

impl AssetLodDistanceBand {
    pub fn new(level_index: usize, max_distance_millimeters: u64) -> Self {
        Self {
            level_index,
            max_distance_millimeters,
        }
    }
}

impl AssetLodStreamConfig {
    pub fn new(distance_bands: Vec<AssetLodDistanceBand>) -> Self {
        Self { distance_bands }
    }

    fn validate_for_levels(&self, level_count: usize) -> Result<(), EngineError> {
        if level_count == 0 || self.distance_bands.len() != level_count {
            return asset_stream_invalid(
                "asset_lod.distance_bands",
                "distance bands must cover every LOD product level exactly once",
            );
        }
        let mut previous_max = None;
        for (expected_index, band) in self.distance_bands.iter().enumerate() {
            if band.level_index != expected_index || band.level_index >= level_count {
                return asset_stream_invalid(
                    "asset_lod.distance_bands.level_index",
                    "LOD distance bands must be consecutive and match product levels",
                );
            }
            if previous_max.is_some_and(|previous| band.max_distance_millimeters <= previous) {
                return asset_stream_invalid(
                    "asset_lod.distance_bands.max_distance_millimeters",
                    "LOD distance band maxima must be strictly increasing",
                );
            }
            previous_max = Some(band.max_distance_millimeters);
        }
        Ok(())
    }
}

impl AssetLodStreamLoadRequest {
    pub fn new(
        manifest_path: impl Into<PathBuf>,
        config: AssetLodStreamConfig,
    ) -> Result<Self, EngineError> {
        let request = Self {
            manifest_path: manifest_path.into(),
            config,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.manifest_path.as_os_str().is_empty() || self.config.distance_bands.is_empty() {
            return asset_stream_invalid(
                "load_request",
                "manifest_path and distance_bands must be non-empty",
            );
        }
        Ok(())
    }
}

impl AssetLodStreamLoadJob {
    fn spawn(request: AssetLodStreamLoadRequest) -> Self {
        let request_thread_id = current_thread_id();
        Self {
            join: thread::spawn(move || {
                RuntimeAssetLod::from_manifest_inner(request, request_thread_id)
            }),
        }
    }

    pub fn finish(self) -> Result<RuntimeAssetLod, EngineError> {
        self.join
            .join()
            .map_err(|_| asset_stream_persistence("load_job.join", "asset LOD worker panicked"))?
    }
}

impl RuntimeAssetLod {
    pub fn load_background(request: AssetLodStreamLoadRequest) -> AssetLodStreamLoadJob {
        AssetLodStreamLoadJob::spawn(request)
    }

    pub fn source_readback(&self) -> &AssetLodStreamSourceReadback {
        &self.source_readback
    }

    pub fn select_lod(&self, distance_millimeters: u64) -> Result<AssetLodSelection, EngineError> {
        let Some(band) = self
            .config
            .distance_bands
            .iter()
            .find(|band| distance_millimeters <= band.max_distance_millimeters)
        else {
            return asset_stream_invalid(
                "asset_lod.distance_millimeters",
                format!("distance {distance_millimeters} exceeds configured LOD band coverage"),
            );
        };
        let Some(level) = self
            .levels
            .iter()
            .find(|level| level.level_index == band.level_index)
        else {
            return asset_stream_invalid(
                "asset_lod.level_index",
                format!("LOD level {} is absent from the product", band.level_index),
            );
        };
        Ok(AssetLodSelection {
            product_id: self.product.product_id.clone(),
            level_index: level.level_index,
            level_id: level.level_id.clone(),
            distance_millimeters,
            max_distance_millimeters: band.max_distance_millimeters,
            index_buffer_path: level.index_buffer_path.clone(),
            expected_bytes: level.index_buffer_bytes,
            expected_sha256: level.index_buffer_sha256.clone(),
        })
    }

    pub fn load_selected_background(&self, selection: AssetLodSelection) -> AssetLodLoadJob {
        AssetLodLoadJob::spawn(selection)
    }

    fn from_manifest_inner(
        request: AssetLodStreamLoadRequest,
        request_thread_id: String,
    ) -> Result<Self, EngineError> {
        request.validate()?;
        let worker_thread_id = current_thread_id();
        let loaded_on_background_thread = worker_thread_id != request_thread_id;
        let manifest_path = fs::canonicalize(&request.manifest_path).map_err(|error| {
            asset_stream_persistence(
                "asset_lod.manifest.canonicalize",
                format!(
                    "failed to canonicalize {}: {error}",
                    request.manifest_path.display()
                ),
            )
        })?;
        let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
            asset_stream_persistence(
                "asset_lod.manifest.read",
                format!("failed to read {}: {error}", manifest_path.display()),
            )
        })?;
        let product = parse_asset_lod_product(&manifest_bytes)?;
        request.config.validate_for_levels(product.levels.len())?;
        let manifest_dir = manifest_path.parent().ok_or_else(|| {
            asset_stream_persistence(
                "asset_lod.manifest.parent",
                "manifest has no parent directory",
            )
        })?;
        let levels = validate_level_files(&product, manifest_dir)?;
        let total_index_buffer_bytes = levels.iter().map(|level| level.index_buffer_bytes).sum();
        let source_readback = AssetLodStreamSourceReadback {
            schema: ASSET_LOD_STREAM_SCHEMA.to_string(),
            product_schema: product.schema.clone(),
            product_id: product.product_id.clone(),
            manifest_path: manifest_path.display().to_string(),
            manifest_bytes: manifest_bytes.len() as u64,
            manifest_sha256: sha256_hex(&manifest_bytes),
            level_count: levels.len(),
            total_index_buffer_bytes,
            distance_bands: request.config.distance_bands.clone(),
            loaded_on_background_thread,
            request_thread_id,
            worker_thread_id,
        };
        tracing::info!(
            product_id = %source_readback.product_id,
            levels = source_readback.level_count,
            total_index_buffer_bytes = source_readback.total_index_buffer_bytes,
            loaded_on_background_thread = source_readback.loaded_on_background_thread,
            code = "LITD-ASSET-STREAM-MANIFEST-LOADED",
            "asset LOD stream source loaded"
        );
        Ok(Self {
            product,
            levels,
            config: request.config,
            source_readback,
        })
    }
}

impl AssetLodLoadJob {
    fn spawn(selection: AssetLodSelection) -> Self {
        let request_thread_id = current_thread_id();
        Self {
            join: thread::spawn(move || load_selected_inner(selection, request_thread_id)),
        }
    }

    pub fn finish(self) -> Result<AssetLodLoadReceipt, EngineError> {
        self.join.join().map_err(|_| {
            asset_stream_persistence("selected_lod.join", "asset LOD worker panicked")
        })?
    }
}

fn load_selected_inner(
    selection: AssetLodSelection,
    request_thread_id: String,
) -> Result<AssetLodLoadReceipt, EngineError> {
    let worker_thread_id = current_thread_id();
    let bytes = fs::read(&selection.index_buffer_path).map_err(|error| {
        asset_stream_persistence(
            "asset_lod.selected.read",
            format!(
                "failed to read {}: {error}",
                selection.index_buffer_path.display()
            ),
        )
    })?;
    if bytes.len() as u64 != selection.expected_bytes {
        return asset_stream_readback(
            "asset_lod.selected.bytes",
            selection.expected_bytes.to_string(),
            bytes.len().to_string(),
        );
    }
    let verified_sha256 = sha256_hex(&bytes);
    if verified_sha256 != selection.expected_sha256 {
        return asset_stream_readback(
            "asset_lod.selected.sha256",
            selection.expected_sha256.clone(),
            verified_sha256,
        );
    }
    let readback = AssetLodLoadReadback {
        selection,
        bytes_read: bytes.len() as u64,
        verified_sha256: sha256_hex(&bytes),
        bytes_fnv1a64: fnv1a64_hex(&bytes),
        loaded_on_background_thread: worker_thread_id != request_thread_id,
        request_thread_id,
        worker_thread_id,
    };
    tracing::info!(
        product_id = %readback.selection.product_id,
        level_index = readback.selection.level_index,
        bytes_read = readback.bytes_read,
        loaded_on_background_thread = readback.loaded_on_background_thread,
        code = "LITD-ASSET-STREAM-LOD-LOADED",
        "asset LOD bytes loaded"
    );
    Ok(AssetLodLoadReceipt {
        readback,
        bytes: Arc::from(bytes),
    })
}

fn current_thread_id() -> String {
    format!("{:?}", thread::current().id())
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
