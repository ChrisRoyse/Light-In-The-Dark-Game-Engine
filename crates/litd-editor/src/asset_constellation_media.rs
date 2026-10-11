use std::path::{Path, PathBuf};

use image::GenericImageView;
use litd_engine::EngineError;

use crate::asset_vault::AssetVaultRecord;

use super::{invalid_constellation_error, search_key};

const PREVIEW_NAMES: &[&str] = &[
    "Preview.png",
    "Preview.jpg",
    "Preview.jpeg",
    "preview.png",
    "preview.jpg",
    "contents.png",
    "Contents.png",
];

#[derive(Debug)]
pub(crate) struct ImageInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_type: String,
}

pub(crate) fn image_info(path: &Path) -> Result<ImageInfo, EngineError> {
    let reader = image::ImageReader::open(path).map_err(|error| {
        invalid_constellation_error(
            "image.open",
            format!("failed to open {}: {error}", path.display()),
        )
    })?;
    let image = reader.with_guessed_format().map_err(|error| {
        invalid_constellation_error(
            "image.format",
            format!("failed to guess format for {}: {error}", path.display()),
        )
    })?;
    let decoded = image.decode().map_err(|error| {
        invalid_constellation_error(
            "image.decode",
            format!("failed to decode {}: {error}", path.display()),
        )
    })?;
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return Err(invalid_constellation_error(
            "image.dimensions",
            "image dimensions must be non-zero",
        ));
    }
    Ok(ImageInfo {
        width,
        height,
        color_type: format!("{:?}", decoded.color()),
    })
}

pub(crate) fn image_or_thumbnail_path(
    record: &AssetVaultRecord,
) -> Result<Option<PathBuf>, EngineError> {
    if is_image_path(&record.absolute_path) {
        return Ok(Some(record.absolute_path.clone()));
    }
    let vault_root = vault_root_for_record(record)?;
    let mut current = record.absolute_path.parent();
    while let Some(directory) = current {
        if !directory.starts_with(&vault_root) {
            break;
        }
        for name in PREVIEW_NAMES {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
        current = directory.parent();
    }
    Ok(None)
}

pub(crate) fn relative_to_vault(
    record: &AssetVaultRecord,
    path: &Path,
) -> Result<String, EngineError> {
    let root = vault_root_for_record(record)?;
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|error| {
            invalid_constellation_error(
                "path.relative",
                format!("failed to relativize {}: {error}", path.display()),
            )
        })
}

pub(crate) fn is_document_image(record: &AssetVaultRecord) -> bool {
    matches!(
        record.asset_type.as_str(),
        "material" | "surface" | "terrain" | "texture"
    ) || search_key(record).contains("sheet")
        || search_key(record).contains("texture")
}

pub(crate) fn is_image_path(path: &Path) -> bool {
    extension(path)
        .map(|extension| matches!(extension.as_str(), "png" | "jpg" | "jpeg"))
        .unwrap_or(false)
}

pub(crate) fn is_mesh_path(path: &Path) -> bool {
    extension(path)
        .map(|extension| matches!(extension.as_str(), "gltf" | "glb"))
        .unwrap_or(false)
}

fn vault_root_for_record(record: &AssetVaultRecord) -> Result<PathBuf, EngineError> {
    let mut root = record.absolute_path.clone();
    for _ in Path::new(&record.relative_path).components() {
        if !root.pop() {
            return Err(invalid_constellation_error(
                "path.vault_root",
                format!("could not derive vault root for {}", record.relative_path),
            ));
        }
    }
    Ok(root)
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}
