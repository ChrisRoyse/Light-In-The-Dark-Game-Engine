use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::EngineError;

use crate::asset_vault::AssetVaultRecord;

mod artifacts;
mod atlas;
mod import;
mod ktx2_container;
mod roles;

pub use artifacts::{
    TextureArtifactFile, TextureImportArtifactReceipt, TextureImportManifest,
    read_texture_import_manifest, write_texture_import_artifacts,
};
pub use atlas::{
    TextureAtlasBuild, TextureAtlasPlacement, TextureAtlasReceipt, TextureAtlasSettings,
    build_texture_atlas,
};
pub use import::{
    ProcessedTextureAsset, ProcessedTextureGpuMipLevel, TextureGpuMipLevelReceipt,
    TextureImportReceipt, import_texture_asset,
};
pub use ktx2_container::{
    Ktx2Rgba8EncodeReceipt, Ktx2Rgba8Texture, Ktx2TextureReceipt, encode_rgba8_ktx2,
    validate_ktx2_texture_record,
};
pub use roles::{TextureColorSpace, TextureRole};

#[cfg(test)]
fn expected_bc7_bytes(width: u32, height: u32) -> Result<usize, EngineError> {
    import::expected_bc7_bytes(width, height)
}

pub(super) fn verified_record_bytes(
    record: &AssetVaultRecord,
    field: &str,
) -> Result<Vec<u8>, EngineError> {
    if record.bytes_on_disk == 0 {
        return texture_import_error("source.bytes", "texture source must be non-empty");
    }
    let bytes =
        fs::read(&record.absolute_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: format!("{field}.read"),
            reason: format!("failed to read {}: {error}", record.absolute_path.display()),
        })?;
    if bytes.len() as u64 != record.bytes_on_disk {
        return texture_import_error(
            "source.bytes",
            format!(
                "{} manifest_bytes={} actual_bytes={}",
                record.relative_path,
                record.bytes_on_disk,
                bytes.len()
            ),
        );
    }
    let actual_sha256 = sha256_hex(&bytes);
    if actual_sha256 != record.sha256 {
        return texture_import_error(
            "source.sha256",
            format!(
                "{} manifest_sha256={} actual_sha256={actual_sha256}",
                record.relative_path, record.sha256
            ),
        );
    }
    Ok(bytes)
}

pub(super) fn texture_source_format(relative_path: &str) -> Result<String, EngineError> {
    match Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Ok("png".to_string()),
        Some("jpg" | "jpeg") => Ok("jpeg".to_string()),
        Some("ktx2") => Ok("ktx2".to_string()),
        Some("basis") => Ok("basis".to_string()),
        _ => texture_import_error(
            "source.extension",
            format!("{relative_path} is not a supported texture source extension"),
        ),
    }
}

pub(super) fn validate_texture_dimensions(
    width: u32,
    height: u32,
    source: &str,
) -> Result<(), EngineError> {
    if width == 0 || height == 0 {
        return texture_import_error("image.dimensions", format!("{source} has zero dimensions"));
    }
    if width > basis_universal::IMAGE_DIMENSION_MAX || height > basis_universal::IMAGE_DIMENSION_MAX
    {
        return texture_import_error(
            "image.dimensions",
            format!(
                "{source} dimensions {width}x{height} exceed Basis Universal maximum {}",
                basis_universal::IMAGE_DIMENSION_MAX
            ),
        );
    }
    Ok(())
}

pub(super) fn validate_rgba_len(
    width: u32,
    height: u32,
    rgba8: &[u8],
    source: &str,
) -> Result<(), EngineError> {
    let expected = rgba_len(width, height, source)?;
    if rgba8.len() != expected {
        return texture_import_error(
            "image.rgba8",
            format!(
                "{source} decoded RGBA byte count {} != expected {expected}",
                rgba8.len()
            ),
        );
    }
    Ok(())
}

pub(super) fn rgba_len(width: u32, height: u32, source: &str) -> Result<usize, EngineError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.image.rgba8".to_string(),
            reason: format!("{source} dimensions overflow RGBA byte count"),
        })
}

pub(super) fn texture_import_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.texture_import.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "../texture_import_mip_artifact_tests.rs"]
mod mip_artifact_tests;

#[cfg(test)]
#[path = "../texture_import_tests.rs"]
mod tests;
