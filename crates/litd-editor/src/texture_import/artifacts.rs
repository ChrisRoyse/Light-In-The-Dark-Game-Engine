use std::{collections::BTreeSet, fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::ProcessedTextureAsset;
use super::{
    TextureAtlasBuild, TextureAtlasReceipt, TextureImportReceipt, sha256_hex, texture_import_error,
};

pub fn write_texture_import_artifacts(
    root: impl AsRef<Path>,
    textures: &[ProcessedTextureAsset],
    atlas: &TextureAtlasBuild,
) -> Result<TextureImportArtifactReceipt, EngineError> {
    let root = root.as_ref();
    fs::create_dir_all(root).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.texture_import.artifacts.create_dir".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;
    let gpu_mip_count = textures
        .iter()
        .map(|texture| texture.gpu_mip_levels.len())
        .sum::<usize>();
    let mut texture_files = Vec::with_capacity(textures.len() + gpu_mip_count);
    for texture in textures {
        let stem = artifact_stem(&texture.receipt.source_relative_path);
        let basis_path = root.join(format!("{stem}.basis"));
        write_bytes(&basis_path, &texture.basis_bytes, "basis")?;
        texture_files.push(TextureArtifactFile {
            source_relative_path: texture.receipt.source_relative_path.clone(),
            kind: "basis".to_string(),
            level_index: None,
            width: None,
            height: None,
            format: None,
            path: basis_path.display().to_string(),
            bytes: texture.basis_bytes.len(),
            sha256: sha256_file(&basis_path)?,
        });
        for mip in &texture.gpu_mip_levels {
            let level_index = mip.receipt.level_index;
            let gpu_path = root.join(format!("{stem}.mip{level_index:02}.bc7"));
            write_bytes(&gpu_path, &mip.bytes, "bc7")?;
            texture_files.push(TextureArtifactFile {
                source_relative_path: texture.receipt.source_relative_path.clone(),
                kind: "bc7".to_string(),
                level_index: Some(level_index),
                width: Some(mip.receipt.width),
                height: Some(mip.receipt.height),
                format: Some(mip.receipt.gpu_transcode_format.clone()),
                path: gpu_path.display().to_string(),
                bytes: mip.bytes.len(),
                sha256: sha256_file(&gpu_path)?,
            });
        }
    }
    let atlas_path = root.join("texture_atlas.rgba8");
    write_bytes(&atlas_path, &atlas.rgba8, "atlas")?;
    let manifest_path = root.join("texture_import_manifest.json");
    let manifest = TextureImportManifest {
        textures: textures
            .iter()
            .map(|texture| texture.receipt.clone())
            .collect(),
        atlas: atlas.receipt.clone(),
        texture_files: texture_files.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.artifacts.manifest".to_string(),
            reason: format!("failed to serialize texture import manifest: {error}"),
        }
    })?;
    write_bytes(&manifest_path, &manifest_bytes, "manifest")?;
    Ok(TextureImportArtifactReceipt {
        manifest_path: manifest_path.display().to_string(),
        manifest_bytes: manifest_bytes.len(),
        manifest_sha256: sha256_file(&manifest_path)?,
        atlas_path: atlas_path.display().to_string(),
        atlas_bytes: atlas.rgba8.len(),
        atlas_sha256: sha256_file(&atlas_path)?,
        texture_files,
    })
}

pub fn read_texture_import_manifest(
    path: impl AsRef<Path>,
) -> Result<TextureImportManifest, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.texture_import.artifacts.manifest.read".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    let manifest: TextureImportManifest =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.artifacts.manifest.parse".to_string(),
            reason: format!("failed to parse {}: {error}", path.display()),
        })?;
    validate_texture_import_manifest_files(&manifest)?;
    Ok(manifest)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureImportManifest {
    pub textures: Vec<TextureImportReceipt>,
    pub atlas: TextureAtlasReceipt,
    pub texture_files: Vec<TextureArtifactFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureArtifactFile {
    pub source_relative_path: String,
    pub kind: String,
    pub level_index: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureImportArtifactReceipt {
    pub manifest_path: String,
    pub manifest_bytes: usize,
    pub manifest_sha256: String,
    pub atlas_path: String,
    pub atlas_bytes: usize,
    pub atlas_sha256: String,
    pub texture_files: Vec<TextureArtifactFile>,
}

fn artifact_stem(relative_path: &str) -> String {
    relative_path
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn write_bytes(path: &Path, bytes: &[u8], kind: &str) -> Result<(), EngineError> {
    fs::write(path, bytes).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: format!("editor.texture_import.artifacts.{kind}.write"),
        reason: format!("failed to write {}: {error}", path.display()),
    })
}

fn sha256_file(path: &Path) -> Result<String, EngineError> {
    let bytes = fs::read(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.texture_import.artifacts.readback".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    Ok(sha256_hex(&bytes))
}

fn validate_texture_import_manifest_files(
    manifest: &TextureImportManifest,
) -> Result<(), EngineError> {
    for file in &manifest.texture_files {
        let bytes = artifact_file_bytes(file)?;
        validate_artifact_file_bytes(file, &bytes)?;
    }
    for texture in &manifest.textures {
        validate_texture_artifact_contract(texture, &manifest.texture_files)?;
    }
    Ok(())
}

fn validate_texture_artifact_contract(
    texture: &TextureImportReceipt,
    files: &[TextureArtifactFile],
) -> Result<(), EngineError> {
    let basis_files = files
        .iter()
        .filter(|file| {
            file.source_relative_path == texture.source_relative_path && file.kind == "basis"
        })
        .collect::<Vec<_>>();
    if basis_files.len() != 1 {
        return texture_import_error(
            "artifacts.basis.file",
            format!(
                "{} expected exactly one basis artifact, found {}",
                texture.source_relative_path,
                basis_files.len()
            ),
        );
    }
    if basis_files[0].bytes != texture.basis_bytes || basis_files[0].sha256 != texture.basis_sha256
    {
        return texture_import_error(
            "artifacts.basis.receipt",
            format!(
                "{} basis artifact metadata does not match import receipt",
                texture.source_relative_path
            ),
        );
    }
    if texture.gpu_mip_levels.is_empty() {
        return texture_import_error(
            "artifacts.mip.empty",
            format!("{} has no GPU mip levels", texture.source_relative_path),
        );
    }
    if texture.mipmaps_generated != (texture.gpu_mip_levels.len() > 1) {
        return texture_import_error(
            "artifacts.mip.flag",
            format!(
                "{} mipmaps_generated={} but mip level count is {}",
                texture.source_relative_path,
                texture.mipmaps_generated,
                texture.gpu_mip_levels.len()
            ),
        );
    }
    let mut seen_levels = BTreeSet::new();
    let mut aggregate = Vec::new();
    for (expected_index, mip) in texture.gpu_mip_levels.iter().enumerate() {
        if mip.level_index != expected_index as u32 || !seen_levels.insert(mip.level_index) {
            return texture_import_error(
                "artifacts.mip.sequence",
                format!(
                    "{} has non-consecutive or duplicate mip level {}",
                    texture.source_relative_path, mip.level_index
                ),
            );
        }
        let matching = files
            .iter()
            .filter(|file| {
                file.source_relative_path == texture.source_relative_path
                    && file.kind == "bc7"
                    && file.level_index == Some(mip.level_index)
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return texture_import_error(
                "artifacts.mip.file",
                format!(
                    "{} mip {} expected exactly one BC7 artifact, found {}",
                    texture.source_relative_path,
                    mip.level_index,
                    matching.len()
                ),
            );
        }
        let file = matching[0];
        if file.width != Some(mip.width)
            || file.height != Some(mip.height)
            || file.format.as_deref() != Some(mip.gpu_transcode_format.as_str())
            || file.bytes != mip.bytes
            || file.sha256 != mip.sha256
        {
            return texture_import_error(
                "artifacts.mip.receipt",
                format!(
                    "{} mip {} artifact metadata does not match import receipt",
                    texture.source_relative_path, mip.level_index
                ),
            );
        }
        aggregate.extend_from_slice(&artifact_file_bytes(file)?);
    }
    if aggregate.len() != texture.gpu_transcode_bytes
        || sha256_hex(&aggregate) != texture.gpu_transcode_sha256
    {
        return texture_import_error(
            "artifacts.mip.aggregate",
            format!(
                "{} aggregate GPU mip bytes do not match import receipt",
                texture.source_relative_path
            ),
        );
    }
    Ok(())
}

fn artifact_file_bytes(file: &TextureArtifactFile) -> Result<Vec<u8>, EngineError> {
    fs::read(&file.path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.texture_import.artifacts.file.read".to_string(),
        reason: format!("failed to read {}: {error}", file.path),
    })
}

fn validate_artifact_file_bytes(
    file: &TextureArtifactFile,
    bytes: &[u8],
) -> Result<(), EngineError> {
    if file.bytes != bytes.len() {
        return texture_import_error(
            "artifacts.file.bytes",
            format!(
                "{} byte count {} != physical {}",
                file.path,
                file.bytes,
                bytes.len()
            ),
        );
    }
    let actual_sha256 = sha256_hex(bytes);
    if file.sha256 != actual_sha256 {
        return texture_import_error(
            "artifacts.file.sha256",
            format!(
                "{} sha256 {} != physical {actual_sha256}",
                file.path, file.sha256
            ),
        );
    }
    Ok(())
}
