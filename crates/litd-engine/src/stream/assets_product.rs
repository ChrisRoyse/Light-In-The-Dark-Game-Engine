use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::Deserialize;

use crate::EngineError;

use super::asset_stream_error::{
    invalid as asset_stream_invalid, persistence as asset_stream_persistence,
    readback as asset_stream_readback,
};

pub(super) const ASSET_LOD_PRODUCT_SCHEMA: &str = "litd.asset_lod_product.v1";
const ASSET_LOD_BUILDER_ID: &str = "litd.asset.meshopt_lod_builder";
const ASSET_LOD_BUILDER_FINGERPRINT: &str = "meshopt-0.6.2:index-cache-overdraw-simplify";

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AssetLodProduct {
    pub(super) schema: String,
    pub(super) builder_id: String,
    pub(super) builder_fingerprint: String,
    pub(super) product_id: String,
    pub(super) source_relative_path: String,
    pub(super) source_sha256: String,
    pub(super) source_vertex_count: usize,
    pub(super) source_index_count: usize,
    pub(super) source_triangle_count: usize,
    pub(super) source_mesh_primitives: u32,
    pub(super) levels: Vec<AssetLodLevel>,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeAssetLodLevel {
    pub(super) level_index: usize,
    pub(super) level_id: String,
    pub(super) index_buffer_path: PathBuf,
    pub(super) index_buffer_bytes: u64,
    pub(super) index_buffer_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AssetLodLevel {
    level_index: usize,
    level_id: String,
    target_index_ratio: f32,
    target_index_count: usize,
    output_index_count: usize,
    output_triangle_count: usize,
    result_error: f32,
    index_buffer_path: String,
    index_buffer_bytes: u64,
    index_buffer_sha256: String,
}

pub(super) fn parse_asset_lod_product(bytes: &[u8]) -> Result<AssetLodProduct, EngineError> {
    let product = serde_json::from_slice::<AssetLodProduct>(bytes)
        .map_err(|error| asset_stream_persistence("asset_lod.manifest.json", error.to_string()))?;
    product.validate_metadata()?;
    Ok(product)
}

pub(super) fn validate_level_files(
    product: &AssetLodProduct,
    manifest_dir: &Path,
) -> Result<Vec<RuntimeAssetLodLevel>, EngineError> {
    let mut levels = Vec::with_capacity(product.levels.len());
    for level in &product.levels {
        let path = resolve_lod_path(manifest_dir, &level.index_buffer_path);
        let bytes = fs::read(&path).map_err(|error| {
            asset_stream_persistence(
                "asset_lod.level.read",
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        verify_level_bytes(level, &bytes)?;
        let path = fs::canonicalize(&path).map_err(|error| {
            asset_stream_persistence(
                "asset_lod.level.canonicalize",
                format!("failed to canonicalize {}: {error}", path.display()),
            )
        })?;
        levels.push(RuntimeAssetLodLevel {
            level_index: level.level_index,
            level_id: level.level_id.clone(),
            index_buffer_path: path,
            index_buffer_bytes: level.index_buffer_bytes,
            index_buffer_sha256: level.index_buffer_sha256.clone(),
        });
    }
    Ok(levels)
}

impl AssetLodProduct {
    fn validate_metadata(&self) -> Result<(), EngineError> {
        if self.schema != ASSET_LOD_PRODUCT_SCHEMA
            || self.builder_id != ASSET_LOD_BUILDER_ID
            || self.builder_fingerprint != ASSET_LOD_BUILDER_FINGERPRINT
            || self.product_id.trim().is_empty()
            || self.source_relative_path.trim().is_empty()
            || !is_sha256_hex(&self.source_sha256)
            || self.source_vertex_count == 0
            || self.source_index_count == 0
            || !self.source_index_count.is_multiple_of(3)
            || self.source_triangle_count != self.source_index_count / 3
            || self.source_mesh_primitives == 0
            || self.levels.is_empty()
        {
            return asset_stream_invalid("asset_lod.product", "LOD product metadata is invalid");
        }
        for (expected_index, level) in self.levels.iter().enumerate() {
            validate_level_metadata(self, expected_index, level)?;
        }
        Ok(())
    }
}

fn validate_level_metadata(
    product: &AssetLodProduct,
    expected_index: usize,
    level: &AssetLodLevel,
) -> Result<(), EngineError> {
    if level.level_index != expected_index
        || level.level_id != format!("{}.lod{expected_index}", product.product_id)
        || !level.target_index_ratio.is_finite()
        || level.target_index_ratio <= 0.0
        || level.target_index_ratio > 1.0
        || level.target_index_count == 0
        || !level.target_index_count.is_multiple_of(3)
        || level.target_index_count > product.source_index_count
        || level.output_index_count == 0
        || !level.output_index_count.is_multiple_of(3)
        || level.output_index_count > product.source_index_count
        || level.output_index_count > level.target_index_count
        || level.output_triangle_count != level.output_index_count / 3
        || !level.result_error.is_finite()
        || level.index_buffer_path.trim().is_empty()
        || level.index_buffer_bytes != (level.output_index_count as u64) * 4
        || !is_sha256_hex(&level.index_buffer_sha256)
    {
        return asset_stream_invalid("asset_lod.level", "LOD level metadata is invalid");
    }
    Ok(())
}

fn verify_level_bytes(level: &AssetLodLevel, bytes: &[u8]) -> Result<(), EngineError> {
    if bytes.len() as u64 != level.index_buffer_bytes {
        return asset_stream_readback(
            "asset_lod.level.bytes",
            level.index_buffer_bytes.to_string(),
            bytes.len().to_string(),
        );
    }
    let actual = sha256_hex(bytes);
    if actual != level.index_buffer_sha256 {
        return asset_stream_readback(
            "asset_lod.level.sha256",
            level.index_buffer_sha256.clone(),
            actual,
        );
    }
    Ok(())
}

fn resolve_lod_path(manifest_dir: &Path, index_buffer_path: &str) -> PathBuf {
    let path = PathBuf::from(index_buffer_path);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

use litd_hash::is_sha256_hex;
