use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::Deserialize;

use crate::EngineError;

use super::{
    asset_stream_error::{
        invalid as asset_stream_invalid, persistence as asset_stream_persistence,
        readback as asset_stream_readback,
    },
    texture::TEXTURE_MIP_PRODUCT_SCHEMA,
};

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TextureMipProduct {
    pub(super) schema: String,
    pub(super) texture_id: String,
    pub(super) source_relative_path: String,
    pub(super) source_format: String,
    pub(super) gpu_format: String,
    pub(super) width: u32,
    pub(super) height: u32,
    levels: Vec<TextureMipProductLevel>,
}

#[derive(Debug, Clone)]
pub(super) struct TextureMipSourceLevel {
    pub(super) level_index: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) path: PathBuf,
    pub(super) bytes: u64,
    pub(super) sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TextureMipProductLevel {
    level_index: u32,
    width: u32,
    height: u32,
    path: String,
    bytes: u64,
    sha256: String,
}

pub(super) fn parse_texture_mip_product(
    manifest_bytes: &[u8],
) -> Result<TextureMipProduct, EngineError> {
    let product = serde_json::from_slice::<TextureMipProduct>(manifest_bytes).map_err(|error| {
        asset_stream_persistence("texture_mip.manifest.json", error.to_string())
    })?;
    product.validate_metadata()?;
    Ok(product)
}

pub(super) fn validate_texture_mip_level_files(
    product: &TextureMipProduct,
    manifest_dir: &Path,
) -> Result<Vec<TextureMipSourceLevel>, EngineError> {
    let mut levels = Vec::with_capacity(product.levels.len());
    for level in &product.levels {
        let path = resolve_mip_path(manifest_dir, &level.path);
        let bytes = fs::read(&path).map_err(|error| {
            asset_stream_persistence(
                "texture_mip.level.read",
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        verify_level_bytes(level, &bytes)?;
        let path = fs::canonicalize(&path).map_err(|error| {
            asset_stream_persistence(
                "texture_mip.level.canonicalize",
                format!("failed to canonicalize {}: {error}", path.display()),
            )
        })?;
        levels.push(TextureMipSourceLevel {
            level_index: level.level_index,
            width: level.width,
            height: level.height,
            path,
            bytes: level.bytes,
            sha256: level.sha256.clone(),
        });
    }
    Ok(levels)
}

impl TextureMipProduct {
    pub(super) fn level_count(&self) -> usize {
        self.levels.len()
    }

    fn validate_metadata(&self) -> Result<(), EngineError> {
        if self.schema != TEXTURE_MIP_PRODUCT_SCHEMA
            || self.texture_id.trim().is_empty()
            || self.source_relative_path.trim().is_empty()
            || self.source_format.trim().is_empty()
            || self.gpu_format.trim().is_empty()
            || self.width == 0
            || self.height == 0
            || self.levels.is_empty()
        {
            return asset_stream_invalid("texture_mip.product", "texture mip product is invalid");
        }
        for (expected, level) in self.levels.iter().enumerate() {
            validate_level_metadata(self, expected as u32, level)?;
        }
        Ok(())
    }
}

fn validate_level_metadata(
    product: &TextureMipProduct,
    expected_index: u32,
    level: &TextureMipProductLevel,
) -> Result<(), EngineError> {
    let expected_width = mip_size(product.width, expected_index);
    let expected_height = mip_size(product.height, expected_index);
    if level.level_index != expected_index
        || level.width != expected_width
        || level.height != expected_height
        || level.path.trim().is_empty()
        || level.bytes == 0
        || !is_sha256_hex(&level.sha256)
    {
        return asset_stream_invalid("texture_mip.level", "texture mip level metadata is invalid");
    }
    let expected_bytes = expected_level_bytes(&product.gpu_format, level.width, level.height)?;
    if level.bytes != expected_bytes {
        return asset_stream_readback(
            "texture_mip.level.bytes",
            expected_bytes.to_string(),
            level.bytes.to_string(),
        );
    }
    Ok(())
}

fn verify_level_bytes(level: &TextureMipProductLevel, bytes: &[u8]) -> Result<(), EngineError> {
    if bytes.len() as u64 != level.bytes {
        return asset_stream_readback(
            "texture_mip.level.file_bytes",
            level.bytes.to_string(),
            bytes.len().to_string(),
        );
    }
    let actual = sha256_hex(bytes);
    if actual != level.sha256 {
        return asset_stream_readback("texture_mip.level.sha256", level.sha256.clone(), actual);
    }
    Ok(())
}

fn expected_level_bytes(format: &str, width: u32, height: u32) -> Result<u64, EngineError> {
    match format {
        "BC7_RGBA" => {
            let blocks_x = width.div_ceil(4) as u64;
            let blocks_y = height.div_ceil(4) as u64;
            blocks_x
                .checked_mul(blocks_y)
                .and_then(|blocks| blocks.checked_mul(16))
                .ok_or_else(|| {
                    asset_stream_persistence("texture_mip.level.bytes", "BC7 byte count overflowed")
                })
        }
        "RGBA8" => (width as u64)
            .checked_mul(height as u64)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                asset_stream_persistence("texture_mip.level.bytes", "RGBA8 byte count overflowed")
            }),
        _ => asset_stream_invalid(
            "texture_mip.gpu_format",
            format!("unsupported streamable texture format {format}"),
        ),
    }
}

fn mip_size(size: u32, level_index: u32) -> u32 {
    (size >> level_index).max(1)
}

fn resolve_mip_path(manifest_dir: &Path, mip_path: &str) -> PathBuf {
    let path = PathBuf::from(mip_path);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

use litd_hash::is_sha256_hex;
