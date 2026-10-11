use basis_universal::{
    BasisTextureFormat, Compressor, CompressorParams, TranscodeParameters, Transcoder,
    TranscoderTextureFormat,
};
use image::ImageFormat;
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::asset_vault::AssetVaultRecord;

use super::{
    TextureColorSpace, TextureRole, sha256_hex, texture_import_error, texture_source_format,
    validate_rgba_len, validate_texture_dimensions, verified_record_bytes,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureGpuMipLevelReceipt {
    pub level_index: u32,
    pub width: u32,
    pub height: u32,
    pub gpu_transcode_format: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureImportReceipt {
    pub source_relative_path: String,
    pub source_absolute_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub source_format: String,
    pub role: TextureRole,
    pub color_space: TextureColorSpace,
    pub width: u32,
    pub height: u32,
    pub decoded_rgba8_bytes: usize,
    pub decoded_rgba8_sha256: String,
    pub alpha_present: bool,
    pub basis_container_format: String,
    pub basis_bytes: usize,
    pub basis_sha256: String,
    pub basis_bits_per_texel: f64,
    pub gpu_transcode_format: String,
    pub gpu_transcode_bytes: usize,
    pub gpu_transcode_sha256: String,
    pub mipmaps_generated: bool,
    pub gpu_mip_levels: Vec<TextureGpuMipLevelReceipt>,
}

#[derive(Debug, Clone)]
pub struct ProcessedTextureGpuMipLevel {
    pub receipt: TextureGpuMipLevelReceipt,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ProcessedTextureAsset {
    pub receipt: TextureImportReceipt,
    pub decoded_rgba8: Vec<u8>,
    pub basis_bytes: Vec<u8>,
    pub gpu_transcode_bytes: Vec<u8>,
    pub gpu_mip_levels: Vec<ProcessedTextureGpuMipLevel>,
}

pub fn import_texture_asset(
    record: &AssetVaultRecord,
    role: TextureRole,
) -> Result<ProcessedTextureAsset, EngineError> {
    let bytes = verified_record_bytes(record, "editor.texture_import.source")?;
    let source_format = texture_source_format(&record.relative_path)?;
    if source_format == "ktx2" {
        return texture_import_error(
            "ktx2.transcode",
            format!(
                "{} is a KTX2 container; validate it with validate_ktx2_texture_record and transcode before atlas import",
                record.relative_path
            ),
        );
    }
    if source_format == "basis" {
        return texture_import_error(
            "basis.transcode",
            format!(
                "{} is already Basis Universal; runtime transcode import is not wired for atlas source pixels",
                record.relative_path
            ),
        );
    }

    let decoded_format =
        image::guess_format(&bytes).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.image.format".to_string(),
            reason: format!(
                "{} failed image format detection: {error}",
                record.relative_path
            ),
        })?;
    validate_extension_matches_image_format(&source_format, decoded_format, record)?;
    let image = image::load_from_memory_with_format(&bytes, decoded_format).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.image.decode".to_string(),
            reason: format!("{} failed image decode: {error}", record.relative_path),
        }
    })?;
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    validate_texture_dimensions(width, height, &record.relative_path)?;
    let decoded_rgba8 = rgba.into_raw();
    validate_rgba_len(width, height, &decoded_rgba8, &record.relative_path)?;
    let decoded_rgba8_sha256 = sha256_hex(&decoded_rgba8);
    let alpha_present = decoded_rgba8
        .chunks_exact(4)
        .any(|pixel| pixel[3] != u8::MAX);

    let (basis_bytes, basis_bits_per_texel) =
        encode_basis_universal(&decoded_rgba8, width, height, role, &record.relative_path)?;
    let basis_sha256 = sha256_hex(&basis_bytes);
    let (gpu_transcode_format, gpu_mip_levels) =
        transcode_basis_to_gpu_blocks(&basis_bytes, width, height, &record.relative_path)?;
    let mut gpu_transcode_bytes = Vec::new();
    for mip in &gpu_mip_levels {
        gpu_transcode_bytes.extend_from_slice(&mip.bytes);
    }
    let gpu_transcode_sha256 = sha256_hex(&gpu_transcode_bytes);
    let gpu_mip_level_receipts = gpu_mip_levels
        .iter()
        .map(|mip| mip.receipt.clone())
        .collect::<Vec<_>>();

    let receipt = TextureImportReceipt {
        source_relative_path: record.relative_path.clone(),
        source_absolute_path: record.absolute_path.display().to_string(),
        source_sha256: record.sha256.clone(),
        source_bytes: record.bytes_on_disk,
        source_format,
        role,
        color_space: role.color_space(),
        width,
        height,
        decoded_rgba8_bytes: decoded_rgba8.len(),
        decoded_rgba8_sha256,
        alpha_present,
        basis_container_format: role.basis_format_label().to_string(),
        basis_bytes: basis_bytes.len(),
        basis_sha256,
        basis_bits_per_texel,
        gpu_transcode_format,
        gpu_transcode_bytes: gpu_transcode_bytes.len(),
        gpu_transcode_sha256,
        mipmaps_generated: gpu_mip_level_receipts.len() > 1,
        gpu_mip_levels: gpu_mip_level_receipts,
    };
    Ok(ProcessedTextureAsset {
        receipt,
        decoded_rgba8,
        basis_bytes,
        gpu_transcode_bytes,
        gpu_mip_levels,
    })
}

fn validate_extension_matches_image_format(
    source_format: &str,
    decoded_format: ImageFormat,
    record: &AssetVaultRecord,
) -> Result<(), EngineError> {
    let expected = match source_format {
        "png" => ImageFormat::Png,
        "jpeg" => ImageFormat::Jpeg,
        _ => {
            return texture_import_error(
                "image.format",
                format!(
                    "{} cannot be decoded as a raster image",
                    record.relative_path
                ),
            );
        }
    };
    if decoded_format != expected {
        return texture_import_error(
            "image.format",
            format!(
                "{} extension says {source_format}, but bytes decode as {decoded_format:?}",
                record.relative_path
            ),
        );
    }
    Ok(())
}

fn encode_basis_universal(
    rgba8: &[u8],
    width: u32,
    height: u32,
    role: TextureRole,
    source: &str,
) -> Result<(Vec<u8>, f64), EngineError> {
    let mut params = CompressorParams::new();
    params.set_basis_format(role.basis_format());
    params.set_color_space(role.color_space().basis_color_space());
    params.set_mip_color_space(role.color_space().basis_color_space());
    params.set_generate_mipmaps(true);
    match role.basis_format() {
        BasisTextureFormat::ETC1S => params.set_etc1s_quality_level(role.basis_quality()),
        BasisTextureFormat::UASTC4x4 => {
            params.set_uastc_quality_level(role.basis_quality());
            params.set_rdo_uastc(None);
        }
    }
    if role == TextureRole::Normal {
        params.tune_for_normal_maps();
    }
    {
        let mut image = params.source_image_mut(0);
        image.init(rgba8, width, height, 4);
    }
    let mut compressor = Compressor::new(1);
    let initialized = unsafe { compressor.init(&params) };
    if !initialized {
        return texture_import_error(
            "basis.init",
            format!("{source} failed Basis Universal compressor initialization"),
        );
    }
    unsafe {
        compressor
            .process()
            .map_err(|error| EngineError::UiRenderInvalidConfig {
                field: "editor.texture_import.basis.process".to_string(),
                reason: format!("{source} failed Basis Universal compression: {error:?}"),
            })?;
    }
    let basis = compressor.basis_file().to_vec();
    if basis.is_empty() {
        return texture_import_error(
            "basis.bytes",
            format!("{source} produced an empty Basis Universal payload"),
        );
    }
    Ok((basis, compressor.bits_per_texel()))
}

fn transcode_basis_to_gpu_blocks(
    basis: &[u8],
    width: u32,
    height: u32,
    source: &str,
) -> Result<(String, Vec<ProcessedTextureGpuMipLevel>), EngineError> {
    let mut transcoder = Transcoder::new();
    if !transcoder.validate_header(basis) {
        return texture_import_error(
            "basis.header",
            format!("{source} Basis payload failed header validation"),
        );
    }
    transcoder
        .prepare_transcoding(basis)
        .map_err(|_| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.basis.prepare_transcoding".to_string(),
            reason: format!("{source} failed Basis transcode preparation"),
        })?;
    let format = TranscoderTextureFormat::BC7_RGBA;
    let level_count = transcoder.image_level_count(basis, 0);
    if level_count == 0 {
        transcoder.end_transcoding();
        return texture_import_error(
            "basis.mip_levels",
            format!("{source} Basis payload reported zero mip levels"),
        );
    }
    let expected_level_count = expected_mip_level_count(width, height);
    if level_count != expected_level_count {
        transcoder.end_transcoding();
        return texture_import_error(
            "basis.mip_levels",
            format!(
                "{source} Basis mip level count {level_count} != expected {expected_level_count}"
            ),
        );
    }
    let mips_result = (|| {
        let mut mips = Vec::with_capacity(level_count as usize);
        for level_index in 0..level_count {
            let description = transcoder
                .image_level_description(basis, 0, level_index)
                .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                    field: "editor.texture_import.basis.mip.description".to_string(),
                    reason: format!("{source} missing Basis mip level {level_index} description"),
                })?;
            let expected_width = expected_mip_dimension(width, level_index);
            let expected_height = expected_mip_dimension(height, level_index);
            if description.original_width != expected_width
                || description.original_height != expected_height
            {
                return texture_import_error(
                    "basis.mip.dimensions",
                    format!(
                        "{source} mip {level_index} dimensions {}x{} != expected {}x{}",
                        description.original_width,
                        description.original_height,
                        expected_width,
                        expected_height
                    ),
                );
            }
            let output = transcoder
                .transcode_image_level(
                    basis,
                    format,
                    TranscodeParameters {
                        image_index: 0,
                        level_index,
                        decode_flags: None,
                        output_row_pitch_in_blocks_or_pixels: None,
                        output_rows_in_pixels: None,
                    },
                )
                .map_err(|error| EngineError::UiRenderInvalidConfig {
                    field: "editor.texture_import.basis.transcode".to_string(),
                    reason: format!(
                        "{source} failed Basis -> BC7 transcode for mip {level_index}: {error:?}"
                    ),
                })?;
            let expected =
                expected_bc7_bytes(description.original_width, description.original_height)?;
            if output.len() != expected {
                return texture_import_error(
                    "basis.transcode.bytes",
                    format!(
                        "{source} mip {level_index} BC7 byte count {} != expected {expected}",
                        output.len()
                    ),
                );
            }
            let receipt = TextureGpuMipLevelReceipt {
                level_index,
                width: description.original_width,
                height: description.original_height,
                gpu_transcode_format: format.format_name().to_string(),
                bytes: output.len(),
                sha256: sha256_hex(&output),
            };
            mips.push(ProcessedTextureGpuMipLevel {
                receipt,
                bytes: output,
            });
        }
        Ok(mips)
    })();
    transcoder.end_transcoding();
    Ok((format.format_name().to_string(), mips_result?))
}

pub(super) fn expected_bc7_bytes(width: u32, height: u32) -> Result<usize, EngineError> {
    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    blocks_x
        .checked_mul(blocks_y)
        .and_then(|blocks| blocks.checked_mul(16))
        .map(|bytes| bytes as usize)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.basis.transcode.bytes".to_string(),
            reason: "BC7 byte count overflow".to_string(),
        })
}

fn expected_mip_dimension(size: u32, level_index: u32) -> u32 {
    size.checked_shr(level_index).unwrap_or(0).max(1)
}

fn expected_mip_level_count(width: u32, height: u32) -> u32 {
    let mut levels = 1;
    let mut width = width;
    let mut height = height;
    while width > 1 || height > 1 {
        width = (width / 2).max(1);
        height = (height / 2).max(1);
        levels += 1;
    }
    levels
}
