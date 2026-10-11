use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    asset_vault::AssetVaultRecord,
    gltf_validation::validate_gltf_source,
    history::{EditorDocument, stable_fingerprint},
};

pub const MODEL_PRODUCT_ID_PROPERTY: &str = "model_product_id";
pub const MODEL_SOURCE_SHA256_PROPERTY: &str = "model_source_sha256";
pub const MODEL_SOURCE_PATH_PROPERTY: &str = "model_source_path";
const BUILDER_ID: &str = "litd.asset.vault_product_builder";
const BUILDER_FINGERPRINT: &str = "asset-products-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetProductKind {
    Mesh,
    Texture,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProduct {
    pub product_id: String,
    pub product_kind: AssetProductKind,
    pub source: String,
    pub asset_id: String,
    pub title: String,
    pub license: String,
    pub license_url: String,
    pub source_url: String,
    pub source_relative_path: String,
    pub source_absolute_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub builder_id: String,
    pub builder_fingerprint: String,
    pub deterministic_sub_id: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetAssignmentReceipt {
    pub object_id: String,
    pub product_id: String,
    pub product_kind: AssetProductKind,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub document_fingerprint: String,
}

impl AssetProduct {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.product_id.trim().is_empty() || self.deterministic_sub_id.trim().is_empty() {
            return invalid_asset_product("id", "product_id and deterministic_sub_id are required");
        }
        if self.builder_id != BUILDER_ID || self.builder_fingerprint != BUILDER_FINGERPRINT {
            return invalid_asset_product("builder", "unexpected asset product builder identity");
        }
        if self.license != "CC0-1.0" {
            return invalid_asset_product("license", "asset product must carry CC0-1.0 provenance");
        }
        if !is_sha256_hex(&self.source_sha256) {
            return invalid_asset_product("source_sha256", "must be 64 lowercase hex chars");
        }
        let path = Path::new(&self.source_absolute_path);
        if !path.is_absolute() {
            return invalid_asset_product("source_absolute_path", "must be absolute");
        }
        let metadata = fs::metadata(path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_product.file.exists".to_string(),
            reason: format!("failed to inspect {}: {error}", path.display()),
        })?;
        if !metadata.is_file() || metadata.len() != self.source_bytes {
            return invalid_asset_product(
                "source_bytes",
                format!(
                    "{} expected {} bytes, found {}",
                    path.display(),
                    self.source_bytes,
                    metadata.len()
                ),
            );
        }
        validate_product_dependencies(self)?;
        Ok(())
    }
}

pub fn product_from_vault_record(record: &AssetVaultRecord) -> Result<AssetProduct, EngineError> {
    let product_kind = product_kind_for_path(&record.relative_path)?;
    let sub_id_input = format!(
        "{}|{}|{}",
        record.relative_path, record.sha256, BUILDER_FINGERPRINT
    );
    let deterministic_sub_id = litd_hash::fnv1a64_hex(sub_id_input.as_bytes());
    let product_id = format!(
        "litd.asset.{}.{}.{}.{}",
        clean_segment(&record.source),
        clean_segment(&record.asset_id),
        kind_segment(product_kind),
        deterministic_sub_id
    );
    let dependencies = if matches!(
        extension(&record.relative_path).as_deref(),
        Some("gltf" | "glb")
    ) {
        validate_gltf_source(&record.absolute_path, &record.relative_path)?.dependencies
    } else {
        Vec::new()
    };
    let product = AssetProduct {
        product_id,
        product_kind,
        source: record.source.clone(),
        asset_id: record.asset_id.clone(),
        title: record.title.clone(),
        license: record.license.clone(),
        license_url: record.license_url.clone(),
        source_url: record.source_url.clone(),
        source_relative_path: record.relative_path.clone(),
        source_absolute_path: record.absolute_path.display().to_string(),
        source_sha256: record.sha256.clone(),
        source_bytes: record.bytes_on_disk,
        builder_id: BUILDER_ID.to_string(),
        builder_fingerprint: BUILDER_FINGERPRINT.to_string(),
        deterministic_sub_id,
        dependencies,
    };
    product.validate()?;
    Ok(product)
}

fn validate_product_dependencies(product: &AssetProduct) -> Result<(), EngineError> {
    let root = vault_root_for_product(product)?;
    for dependency in &product.dependencies {
        let relative = Path::new(dependency);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        {
            return invalid_asset_product(
                "dependencies.path",
                format!("dependency escapes vault: {dependency}"),
            );
        }
        let path = root.join(relative);
        if !fs::metadata(&path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
        {
            return invalid_asset_product(
                "dependencies.exists",
                format!("missing dependency {}", path.display()),
            );
        }
    }
    Ok(())
}

fn vault_root_for_product(product: &AssetProduct) -> Result<PathBuf, EngineError> {
    let mut root = PathBuf::from(&product.source_absolute_path);
    for _ in Path::new(&product.source_relative_path).components() {
        if !root.pop() {
            return invalid_asset_product("source_root", "could not derive vault root");
        }
    }
    Ok(root)
}

pub fn assign_vault_model_to_unit(
    document: &mut EditorDocument,
    object_id: &str,
    record: &AssetVaultRecord,
) -> Result<AssetAssignmentReceipt, EngineError> {
    let product = product_from_vault_record(record)?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_asset_product(
            "assignment.kind",
            "unit model assignment requires a mesh product",
        );
    }
    let mut next = document.clone();
    let Some(object) = next.objects.get_mut(object_id) else {
        return invalid_asset_product("assignment.object", format!("missing object {object_id}"));
    };
    if object.kind != "Unit" {
        return invalid_asset_product(
            "assignment.object_kind",
            "model assignment target must be a Unit",
        );
    }
    object.properties.insert(
        MODEL_PRODUCT_ID_PROPERTY.to_string(),
        product.product_id.clone(),
    );
    object.properties.insert(
        MODEL_SOURCE_SHA256_PROPERTY.to_string(),
        product.source_sha256.clone(),
    );
    object.properties.insert(
        MODEL_SOURCE_PATH_PROPERTY.to_string(),
        product.source_relative_path.clone(),
    );
    next.asset_products
        .insert(product.product_id.clone(), product.clone());
    next.validate()?;
    let document_fingerprint = stable_fingerprint(&next)?;
    *document = next;
    Ok(AssetAssignmentReceipt {
        object_id: object_id.to_string(),
        product_id: product.product_id,
        product_kind: product.product_kind,
        source_relative_path: product.source_relative_path,
        source_sha256: product.source_sha256,
        document_fingerprint,
    })
}

pub fn validate_model_assignment(
    object_id: &str,
    properties: &BTreeMap<String, String>,
    products: &BTreeMap<String, AssetProduct>,
) -> Result<(), EngineError> {
    let Some(product_id) = properties.get(MODEL_PRODUCT_ID_PROPERTY) else {
        return Ok(());
    };
    let product = products
        .get(product_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_product.assignment.product".to_string(),
            reason: format!("object {object_id} references missing product {product_id}"),
        })?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_asset_product(
            "assignment.product_kind",
            "model_product_id must reference Mesh",
        );
    }
    if properties.get(MODEL_SOURCE_SHA256_PROPERTY) != Some(&product.source_sha256) {
        return invalid_asset_product("assignment.source_sha256", "unit model provenance mismatch");
    }
    product.validate()
}

fn product_kind_for_path(relative_path: &str) -> Result<AssetProductKind, EngineError> {
    match extension(relative_path).as_deref() {
        Some("fbx" | "obj" | "glb" | "gltf") => Ok(AssetProductKind::Mesh),
        Some("png" | "jpg" | "jpeg" | "ktx2" | "basis") => Ok(AssetProductKind::Texture),
        Some("wav" | "ogg" | "flac") => Ok(AssetProductKind::Audio),
        _ => invalid_asset_product("source.extension", "unsupported asset product extension"),
    }
}

fn extension(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn kind_segment(kind: AssetProductKind) -> &'static str {
    match kind {
        AssetProductKind::Mesh => "mesh",
        AssetProductKind::Texture => "texture",
        AssetProductKind::Audio => "audio",
    }
}

fn clean_segment(value: &str) -> String {
    value
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

use litd_hash::is_sha256_hex;

fn invalid_asset_product<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_product.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "asset_products_tests.rs"]
mod tests;
