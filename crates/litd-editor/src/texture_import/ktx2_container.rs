use ktx2::{Header, Index, LevelIndex};
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::asset_vault::AssetVaultRecord;

use super::{
    sha256_hex, texture_import_error, texture_source_format, validate_rgba_len,
    validate_texture_dimensions, verified_record_bytes,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ktx2TextureReceipt {
    pub source_relative_path: String,
    pub source_absolute_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub layer_count: u32,
    pub face_count: u32,
    pub level_count: u32,
    pub vk_format: String,
    pub supercompression_scheme: String,
    pub color_model: String,
    pub transfer_function: String,
    pub color_primaries: String,
    pub level_count_read: usize,
    pub level_bytes: u64,
    pub requires_basis_transcode: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ktx2Rgba8EncodeReceipt {
    pub source_relative_path: String,
    pub width: u32,
    pub height: u32,
    pub source_rgba8_bytes: usize,
    pub source_rgba8_sha256: String,
    pub ktx2_bytes: usize,
    pub ktx2_sha256: String,
    pub vk_format: String,
    pub type_size: u32,
    pub dfd_byte_offset: u32,
    pub dfd_byte_length: u32,
    pub kvd_byte_offset: u32,
    pub kvd_byte_length: u32,
    pub level_byte_offset: u64,
    pub level_byte_length: u64,
    pub writer: String,
}

#[derive(Debug, Clone)]
pub struct Ktx2Rgba8Texture {
    pub receipt: Ktx2Rgba8EncodeReceipt,
    pub bytes: Vec<u8>,
}

pub fn validate_ktx2_texture_record(
    record: &AssetVaultRecord,
) -> Result<Ktx2TextureReceipt, EngineError> {
    let bytes = verified_record_bytes(record, "editor.texture_import.ktx2.source")?;
    let source_format = texture_source_format(&record.relative_path)?;
    if source_format != "ktx2" {
        return texture_import_error(
            "ktx2.extension",
            format!("{} is not a .ktx2 texture source", record.relative_path),
        );
    }
    let reader = ktx2::Reader::new(bytes.as_slice()).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.ktx2.parse".to_string(),
            reason: format!("{} failed KTX2 validation: {error:?}", record.relative_path),
        }
    })?;
    let header = reader.header();
    if header.pixel_height == 0 {
        return texture_import_error(
            "ktx2.dimension",
            format!("{} is not a 2D texture", record.relative_path),
        );
    }
    let levels = reader.levels().collect::<Vec<_>>();
    let level_bytes = levels.iter().map(|level| level.data.len() as u64).sum();
    let requires_basis_transcode = header.format.is_none()
        || header
            .supercompression_scheme
            .map(|scheme| format!("{scheme:?}"))
            .unwrap_or_else(|| "None".to_string())
            .contains("Basis");
    Ok(Ktx2TextureReceipt {
        source_relative_path: record.relative_path.clone(),
        source_absolute_path: record.absolute_path.display().to_string(),
        source_sha256: record.sha256.clone(),
        source_bytes: record.bytes_on_disk,
        width: header.pixel_width,
        height: header.pixel_height,
        depth: header.pixel_depth,
        layer_count: header.layer_count.max(1),
        face_count: header.face_count,
        level_count: header.level_count.max(1),
        vk_format: header
            .format
            .map(|format| format!("{format:?}"))
            .unwrap_or_else(|| "VK_FORMAT_UNDEFINED".to_string()),
        supercompression_scheme: header
            .supercompression_scheme
            .map(|scheme| format!("{scheme:?}"))
            .unwrap_or_else(|| "None".to_string()),
        color_model: reader
            .color_model()
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "unknown".to_string()),
        transfer_function: reader
            .transfer_function()
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "unknown".to_string()),
        color_primaries: reader
            .color_primaries()
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "unknown".to_string()),
        level_count_read: levels.len(),
        level_bytes,
        requires_basis_transcode,
    })
}

pub fn encode_rgba8_ktx2(
    source_relative_path: impl Into<String>,
    rgba8: &[u8],
    width: u32,
    height: u32,
    writer: &str,
) -> Result<Ktx2Rgba8Texture, EngineError> {
    let source_relative_path = source_relative_path.into();
    validate_texture_dimensions(width, height, &source_relative_path)?;
    validate_rgba_len(width, height, rgba8, &source_relative_path)?;
    let writer = writer.trim();
    if writer.is_empty() {
        return texture_import_error("ktx2.writer", "KTX2 writer metadata is required");
    }

    let (basic_dfd, type_size) = ktx2::dfd::Basic::from_format(ktx2::Format::R8G8B8A8_SRGB)
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.ktx2.dfd".to_string(),
            reason: format!("failed to build KTX2 DFD: {error:?}"),
        })?;
    let dfd_block = ktx2::dfd::Block::Basic(basic_dfd);
    let dfd_payload_length = dfd_block.serialized_length();
    let dfd_byte_length = 4_usize.checked_add(dfd_payload_length).ok_or_else(|| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.ktx2.layout".to_string(),
            reason: "KTX2 DFD length overflowed".to_string(),
        }
    })?;
    let kvd = build_ktx2_key_value_data(&[
        ("KTXwriter", writer),
        ("KTXorientation", "S=r,T=d"),
        ("LITDsource", &source_relative_path),
    ])?;

    let dfd_byte_offset = Header::LENGTH + LevelIndex::LENGTH;
    let kvd_byte_offset = align_ktx2_offset(dfd_byte_offset + dfd_byte_length)?;
    let level_byte_offset = align_ktx2_offset(kvd_byte_offset + kvd.len())?;
    let header = Header {
        format: Some(ktx2::Format::R8G8B8A8_SRGB),
        type_size,
        pixel_width: width,
        pixel_height: height,
        pixel_depth: 0,
        layer_count: 0,
        face_count: 1,
        level_count: 1,
        supercompression_scheme: None,
        index: Index {
            dfd_byte_offset: checked_u32(dfd_byte_offset, "ktx2.dfd_offset")?,
            dfd_byte_length: checked_u32(dfd_byte_length, "ktx2.dfd_length")?,
            kvd_byte_offset: checked_u32(kvd_byte_offset, "ktx2.kvd_offset")?,
            kvd_byte_length: checked_u32(kvd.len(), "ktx2.kvd_length")?,
            sgd_byte_offset: 0,
            sgd_byte_length: 0,
        },
    };
    let level_index = LevelIndex {
        byte_offset: checked_u64(level_byte_offset, "ktx2.level_offset")?,
        byte_length: rgba8.len() as u64,
        uncompressed_byte_length: rgba8.len() as u64,
    };

    let mut bytes = Vec::with_capacity(level_byte_offset + rgba8.len());
    bytes.extend_from_slice(&header.as_bytes());
    bytes.extend_from_slice(&level_index.as_bytes());
    pad_vec_to_offset(&mut bytes, dfd_byte_offset);
    bytes.extend_from_slice(&(dfd_byte_length as u32).to_le_bytes());
    let dfd_start = bytes.len();
    bytes.resize(dfd_start + dfd_payload_length, 0);
    dfd_block.to_bytes(&mut bytes[dfd_start..dfd_start + dfd_payload_length]);
    pad_vec_to_offset(&mut bytes, kvd_byte_offset);
    bytes.extend_from_slice(&kvd);
    pad_vec_to_offset(&mut bytes, level_byte_offset);
    bytes.extend_from_slice(rgba8);

    let reader = ktx2::Reader::new(bytes.as_slice()).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.ktx2.generated_parse".to_string(),
            reason: format!(
                "generated KTX2 for {} failed validation: {error:?}",
                source_relative_path
            ),
        }
    })?;
    let header_readback = reader.header();
    let levels = reader.levels().collect::<Vec<_>>();
    if header_readback.pixel_width != width
        || header_readback.pixel_height != height
        || header_readback.format != Some(ktx2::Format::R8G8B8A8_SRGB)
        || levels.len() != 1
        || levels[0].data.len() != rgba8.len()
        || levels[0].data != rgba8
    {
        return texture_import_error(
            "ktx2.generated_readback",
            format!(
                "generated KTX2 readback mismatch for {}: header={header_readback:?} levels={}",
                source_relative_path,
                levels.len()
            ),
        );
    }

    let ktx2_sha256 = sha256_hex(&bytes);
    Ok(Ktx2Rgba8Texture {
        receipt: Ktx2Rgba8EncodeReceipt {
            source_relative_path,
            width,
            height,
            source_rgba8_bytes: rgba8.len(),
            source_rgba8_sha256: sha256_hex(rgba8),
            ktx2_bytes: bytes.len(),
            ktx2_sha256,
            vk_format: "R8G8B8A8_SRGB".to_string(),
            type_size,
            dfd_byte_offset: header.index.dfd_byte_offset,
            dfd_byte_length: header.index.dfd_byte_length,
            kvd_byte_offset: header.index.kvd_byte_offset,
            kvd_byte_length: header.index.kvd_byte_length,
            level_byte_offset: level_index.byte_offset,
            level_byte_length: level_index.byte_length,
            writer: writer.to_string(),
        },
        bytes,
    })
}

fn build_ktx2_key_value_data(pairs: &[(&str, &str)]) -> Result<Vec<u8>, EngineError> {
    let mut data = Vec::new();
    for (key, value) in pairs {
        if key.is_empty() || key.as_bytes().contains(&0) {
            return texture_import_error(
                "ktx2.kvd.key",
                format!("invalid KTX2 key/value key '{key}'"),
            );
        }
        let mut payload = Vec::with_capacity(key.len() + 1 + value.len());
        payload.extend_from_slice(key.as_bytes());
        payload.push(0);
        payload.extend_from_slice(value.as_bytes());
        let payload_len = checked_u32(payload.len(), "ktx2.kvd.entry_length")?;
        data.extend_from_slice(&payload_len.to_le_bytes());
        data.extend_from_slice(&payload);
        let aligned = align_ktx2_offset(data.len())?;
        pad_vec_to_offset(&mut data, aligned);
    }
    Ok(data)
}

fn align_ktx2_offset(value: usize) -> Result<usize, EngineError> {
    value
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.ktx2.layout".to_string(),
            reason: "KTX2 4-byte alignment overflowed".to_string(),
        })
}

fn pad_vec_to_offset(bytes: &mut Vec<u8>, offset: usize) {
    if bytes.len() < offset {
        bytes.resize(offset, 0);
    }
}

fn checked_u32(value: usize, field: &str) -> Result<u32, EngineError> {
    u32::try_from(value).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: format!("editor.texture_import.{field}"),
        reason: format!("value {value} does not fit in u32"),
    })
}

fn checked_u64(value: usize, field: &str) -> Result<u64, EngineError> {
    u64::try_from(value).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: format!("editor.texture_import.{field}"),
        reason: format!("value {value} does not fit in u64"),
    })
}
