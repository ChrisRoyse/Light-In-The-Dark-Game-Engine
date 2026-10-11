//! Engine-side texture mip residency streaming from verified mip products.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    thread::{self, JoinHandle},
};

use litd_hash::{fnv1a64_hex, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::asset_stream_error::{
    invalid as asset_stream_invalid, persistence as asset_stream_persistence,
    readback as asset_stream_readback,
};
use super::texture_product::{
    TextureMipProduct, TextureMipSourceLevel, parse_texture_mip_product,
    validate_texture_mip_level_files,
};

pub const TEXTURE_MIP_STREAM_SCHEMA: &str = "litd.engine.texture_mip_stream.v1";
pub const TEXTURE_MIP_PRODUCT_SCHEMA: &str = "litd.engine.texture_mip_product.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipDistanceBand {
    pub max_distance_millimeters: u64,
    pub highest_resident_mip: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipStreamConfig {
    pub distance_bands: Vec<TextureMipDistanceBand>,
}

#[derive(Debug, Clone)]
pub struct TextureMipStreamLoadRequest {
    pub manifest_path: PathBuf,
    pub config: TextureMipStreamConfig,
}

#[derive(Debug)]
pub struct TextureMipStreamLoadJob {
    join: JoinHandle<Result<RuntimeTextureMipStream, EngineError>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipStreamSourceReadback {
    pub schema: String,
    pub product_schema: String,
    pub texture_id: String,
    pub manifest_path: String,
    pub manifest_bytes: u64,
    pub manifest_sha256: String,
    pub base_width: u32,
    pub base_height: u32,
    pub gpu_format: String,
    pub level_count: usize,
    pub total_mip_bytes: u64,
    pub distance_bands: Vec<TextureMipDistanceBand>,
    pub loaded_on_background_thread: bool,
    pub request_thread_id: String,
    pub worker_thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipStreamReadback {
    pub resident_mips: Vec<u32>,
    pub highest_resident_mip: Option<u32>,
    pub resident_vram_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipLevelRequest {
    pub level_index: u32,
    pub width: u32,
    pub height: u32,
    pub path: PathBuf,
    pub expected_bytes: u64,
    pub expected_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipResidencyPlan {
    pub texture_id: String,
    pub distance_millimeters: u64,
    pub highest_resident_mip: u32,
    pub resident_mips: Vec<u32>,
    pub expected_vram_bytes: u64,
    pub levels: Vec<TextureMipLevelRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipLevelReadback {
    pub level_index: u32,
    pub width: u32,
    pub height: u32,
    pub path: PathBuf,
    pub bytes_read: u64,
    pub verified_sha256: String,
    pub bytes_fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipResidencyReadback {
    pub texture_id: String,
    pub resident_mips: Vec<u32>,
    pub resident_vram_bytes: u64,
    pub loaded_on_background_thread: bool,
    pub request_thread_id: String,
    pub worker_thread_id: String,
    pub levels: Vec<TextureMipLevelReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureMipStreamUpdate {
    pub before: TextureMipStreamReadback,
    pub after: TextureMipStreamReadback,
    pub plan: TextureMipResidencyPlan,
}

#[derive(Debug, Clone)]
pub struct TextureMipResidentLevel {
    pub readback: TextureMipLevelReadback,
    pub bytes: Arc<[u8]>,
}

#[derive(Debug, Clone)]
pub struct TextureMipResidencyReceipt {
    pub plan: TextureMipResidencyPlan,
    pub readback: TextureMipResidencyReadback,
    pub resident_levels: Vec<TextureMipResidentLevel>,
}

#[derive(Debug)]
pub struct TextureMipResidencyJob {
    join: JoinHandle<Result<TextureMipResidencyReceipt, EngineError>>,
}

#[derive(Debug, Clone)]
pub struct RuntimeTextureMipStream {
    product: TextureMipProduct,
    levels: Vec<TextureMipSourceLevel>,
    config: TextureMipStreamConfig,
    resident: BTreeMap<u32, TextureMipResidentLevel>,
    source_readback: TextureMipStreamSourceReadback,
}

impl TextureMipDistanceBand {
    pub fn new(max_distance_millimeters: u64, highest_resident_mip: u32) -> Self {
        Self {
            max_distance_millimeters,
            highest_resident_mip,
        }
    }
}

impl TextureMipStreamConfig {
    pub fn new(distance_bands: Vec<TextureMipDistanceBand>) -> Self {
        Self { distance_bands }
    }

    fn validate_for_levels(&self, max_mip: u32) -> Result<(), EngineError> {
        if self.distance_bands.is_empty() {
            return asset_stream_invalid("texture_mip.distance_bands", "distance bands are empty");
        }
        let mut previous_distance = None;
        let mut previous_mip = 0;
        for (index, band) in self.distance_bands.iter().enumerate() {
            if band.highest_resident_mip > max_mip {
                return asset_stream_invalid(
                    "texture_mip.distance_bands.highest_resident_mip",
                    "distance band references a missing mip level",
                );
            }
            if previous_distance.is_some_and(|previous| band.max_distance_millimeters <= previous) {
                return asset_stream_invalid(
                    "texture_mip.distance_bands.max_distance_millimeters",
                    "distance band maxima must be strictly increasing",
                );
            }
            if index > 0 && band.highest_resident_mip < previous_mip {
                return asset_stream_invalid(
                    "texture_mip.distance_bands.highest_resident_mip",
                    "farther bands cannot require higher-resolution mips than nearer bands",
                );
            }
            previous_distance = Some(band.max_distance_millimeters);
            previous_mip = band.highest_resident_mip;
        }
        Ok(())
    }
}

impl TextureMipStreamLoadRequest {
    pub fn new(
        manifest_path: impl Into<PathBuf>,
        config: TextureMipStreamConfig,
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
                "texture_mip.load_request",
                "manifest_path and distance_bands must be non-empty",
            );
        }
        Ok(())
    }
}

impl TextureMipStreamLoadJob {
    fn spawn(request: TextureMipStreamLoadRequest) -> Self {
        let request_thread_id = current_thread_id();
        Self {
            join: thread::spawn(move || {
                RuntimeTextureMipStream::from_manifest_inner(request, request_thread_id)
            }),
        }
    }

    pub fn finish(self) -> Result<RuntimeTextureMipStream, EngineError> {
        self.join.join().map_err(|_| {
            asset_stream_persistence("texture_mip.load_job.join", "texture mip worker panicked")
        })?
    }
}

impl RuntimeTextureMipStream {
    pub fn load_background(request: TextureMipStreamLoadRequest) -> TextureMipStreamLoadJob {
        TextureMipStreamLoadJob::spawn(request)
    }

    pub fn source_readback(&self) -> &TextureMipStreamSourceReadback {
        &self.source_readback
    }

    pub fn state_readback(&self) -> TextureMipStreamReadback {
        let resident_mips = self.resident.keys().copied().collect::<Vec<_>>();
        TextureMipStreamReadback {
            highest_resident_mip: resident_mips.first().copied(),
            resident_vram_bytes: self
                .resident
                .values()
                .map(|level| level.readback.bytes_read)
                .sum(),
            resident_mips,
        }
    }

    pub fn plan_residency(
        &self,
        distance_millimeters: u64,
    ) -> Result<TextureMipResidencyPlan, EngineError> {
        let Some(band) = self
            .config
            .distance_bands
            .iter()
            .find(|band| distance_millimeters <= band.max_distance_millimeters)
        else {
            return asset_stream_invalid(
                "texture_mip.distance_millimeters",
                format!("distance {distance_millimeters} exceeds mip streaming coverage"),
            );
        };
        let levels = self
            .levels
            .iter()
            .filter(|level| level.level_index >= band.highest_resident_mip)
            .map(|level| TextureMipLevelRequest {
                level_index: level.level_index,
                width: level.width,
                height: level.height,
                path: level.path.clone(),
                expected_bytes: level.bytes,
                expected_sha256: level.sha256.clone(),
            })
            .collect::<Vec<_>>();
        if levels.is_empty() {
            return asset_stream_invalid(
                "texture_mip.resident_range",
                "resident mip range selected no source levels",
            );
        }
        let expected_vram_bytes = levels.iter().map(|level| level.expected_bytes).sum();
        Ok(TextureMipResidencyPlan {
            texture_id: self.product.texture_id.clone(),
            distance_millimeters,
            highest_resident_mip: band.highest_resident_mip,
            resident_mips: levels.iter().map(|level| level.level_index).collect(),
            expected_vram_bytes,
            levels,
        })
    }

    pub fn load_residency_background(
        &self,
        plan: TextureMipResidencyPlan,
    ) -> TextureMipResidencyJob {
        TextureMipResidencyJob::spawn(plan)
    }

    pub fn apply_residency(
        &mut self,
        receipt: TextureMipResidencyReceipt,
    ) -> Result<TextureMipStreamUpdate, EngineError> {
        if receipt.plan.texture_id != self.product.texture_id {
            return asset_stream_invalid(
                "texture_mip.receipt.texture_id",
                "residency receipt belongs to another texture",
            );
        }
        if receipt.readback.resident_vram_bytes != receipt.plan.expected_vram_bytes {
            return asset_stream_readback(
                "texture_mip.resident_vram_bytes",
                receipt.plan.expected_vram_bytes.to_string(),
                receipt.readback.resident_vram_bytes.to_string(),
            );
        }
        let before = self.state_readback();
        self.resident = receipt
            .resident_levels
            .into_iter()
            .map(|level| (level.readback.level_index, level))
            .collect();
        Ok(TextureMipStreamUpdate {
            before,
            after: self.state_readback(),
            plan: receipt.plan,
        })
    }

    fn from_manifest_inner(
        request: TextureMipStreamLoadRequest,
        request_thread_id: String,
    ) -> Result<Self, EngineError> {
        request.validate()?;
        let worker_thread_id = current_thread_id();
        let manifest_path = fs::canonicalize(&request.manifest_path).map_err(|error| {
            asset_stream_persistence(
                "texture_mip.manifest.canonicalize",
                format!(
                    "failed to canonicalize {}: {error}",
                    request.manifest_path.display()
                ),
            )
        })?;
        let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
            asset_stream_persistence(
                "texture_mip.manifest.read",
                format!("failed to read {}: {error}", manifest_path.display()),
            )
        })?;
        let product = parse_texture_mip_product(&manifest_bytes)?;
        let max_mip = product.level_count() as u32 - 1;
        request.config.validate_for_levels(max_mip)?;
        let manifest_dir = manifest_path.parent().ok_or_else(|| {
            asset_stream_persistence("texture_mip.manifest.parent", "manifest has no parent")
        })?;
        let levels = validate_texture_mip_level_files(&product, manifest_dir)?;
        let total_mip_bytes = levels.iter().map(|level| level.bytes).sum();
        let source_readback = TextureMipStreamSourceReadback {
            schema: TEXTURE_MIP_STREAM_SCHEMA.to_string(),
            product_schema: product.schema.clone(),
            texture_id: product.texture_id.clone(),
            manifest_path: manifest_path.display().to_string(),
            manifest_bytes: manifest_bytes.len() as u64,
            manifest_sha256: sha256_hex(&manifest_bytes),
            base_width: product.width,
            base_height: product.height,
            gpu_format: product.gpu_format.clone(),
            level_count: levels.len(),
            total_mip_bytes,
            distance_bands: request.config.distance_bands.clone(),
            loaded_on_background_thread: worker_thread_id != request_thread_id,
            request_thread_id,
            worker_thread_id,
        };
        tracing::info!(
            texture_id = %source_readback.texture_id,
            level_count = source_readback.level_count,
            total_mip_bytes = source_readback.total_mip_bytes,
            code = "LITD-TEXTURE-MIP-STREAM-SOURCE-LOADED",
            "texture mip stream source loaded"
        );
        Ok(Self {
            product,
            levels,
            config: request.config,
            resident: BTreeMap::new(),
            source_readback,
        })
    }
}

impl TextureMipResidencyJob {
    fn spawn(plan: TextureMipResidencyPlan) -> Self {
        let request_thread_id = current_thread_id();
        Self {
            join: thread::spawn(move || load_residency_inner(plan, request_thread_id)),
        }
    }

    pub fn finish(self) -> Result<TextureMipResidencyReceipt, EngineError> {
        self.join.join().map_err(|_| {
            asset_stream_persistence(
                "texture_mip.residency_job.join",
                "texture mip residency worker panicked",
            )
        })?
    }
}

fn load_residency_inner(
    plan: TextureMipResidencyPlan,
    request_thread_id: String,
) -> Result<TextureMipResidencyReceipt, EngineError> {
    let worker_thread_id = current_thread_id();
    let mut resident_levels = Vec::with_capacity(plan.levels.len());
    let mut level_readbacks = Vec::with_capacity(plan.levels.len());
    for level in &plan.levels {
        let bytes = fs::read(&level.path).map_err(|error| {
            asset_stream_persistence(
                "texture_mip.resident.read",
                format!("failed to read {}: {error}", level.path.display()),
            )
        })?;
        verify_requested_level(level, &bytes)?;
        let readback = TextureMipLevelReadback {
            level_index: level.level_index,
            width: level.width,
            height: level.height,
            path: level.path.clone(),
            bytes_read: bytes.len() as u64,
            verified_sha256: sha256_hex(&bytes),
            bytes_fnv1a64: fnv1a64_hex(&bytes),
        };
        level_readbacks.push(readback.clone());
        resident_levels.push(TextureMipResidentLevel {
            readback,
            bytes: Arc::from(bytes),
        });
    }
    let resident_vram_bytes = level_readbacks.iter().map(|level| level.bytes_read).sum();
    let readback = TextureMipResidencyReadback {
        texture_id: plan.texture_id.clone(),
        resident_mips: level_readbacks
            .iter()
            .map(|level| level.level_index)
            .collect(),
        resident_vram_bytes,
        loaded_on_background_thread: worker_thread_id != request_thread_id,
        request_thread_id,
        worker_thread_id,
        levels: level_readbacks,
    };
    Ok(TextureMipResidencyReceipt {
        plan,
        readback,
        resident_levels,
    })
}

fn verify_requested_level(level: &TextureMipLevelRequest, bytes: &[u8]) -> Result<(), EngineError> {
    if bytes.len() as u64 != level.expected_bytes {
        return asset_stream_readback(
            "texture_mip.resident.file_bytes",
            level.expected_bytes.to_string(),
            bytes.len().to_string(),
        );
    }
    let actual = sha256_hex(bytes);
    if actual != level.expected_sha256 {
        return asset_stream_readback(
            "texture_mip.resident.sha256",
            level.expected_sha256.clone(),
            actual,
        );
    }
    Ok(())
}

fn current_thread_id() -> String {
    format!("{:?}", thread::current().id())
}

#[cfg(test)]
#[path = "texture_tests.rs"]
mod tests;
