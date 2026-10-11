use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::{AssetProduct, AssetProductKind},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    history::{EditorDocument, stable_fingerprint},
};

pub const EDITOR_ASSET_MANIFEST_SCHEMA: &str = "litd.editor.asset_manifest.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorAssetManifest {
    pub schema: String,
    pub source_document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub asset_count: usize,
    pub assets: Vec<EditorAssetManifestEntry>,
    pub manifest_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorAssetManifestEntry {
    pub product_id: String,
    pub product_kind: AssetProductKind,
    pub source: String,
    pub asset_id: String,
    pub title: String,
    pub license: String,
    pub license_url: String,
    pub source_url: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorAssetManifestWriteback {
    pub manifest_path: String,
    pub bytes: usize,
    pub sha256: String,
    pub manifest_fingerprint: String,
    pub asset_count: usize,
}

#[derive(Debug, Serialize)]
struct ManifestFingerprintPayload<'a> {
    schema: &'a str,
    source_document_id: &'a str,
    source_revision_id: &'a str,
    source_document_fingerprint: &'a str,
    asset_count: usize,
    assets: &'a [EditorAssetManifestEntry],
}

pub fn build_editor_asset_manifest(
    document: &EditorDocument,
    max_asset_bytes: u64,
) -> Result<EditorAssetManifest, EngineError> {
    if max_asset_bytes == 0 {
        return asset_manifest_error("asset.size_limit", "max_asset_bytes must be non-zero");
    }
    document.validate()?;
    if document.asset_products.is_empty() {
        return asset_manifest_error(
            "assets",
            "editor asset manifest requires at least one imported asset product",
        );
    }

    let catalog = AssetVaultCatalog::load_default_cached()?;
    let mut assets = Vec::new();
    for product in document.asset_products.values() {
        product.validate()?;
        if product.source_bytes > max_asset_bytes {
            return asset_manifest_error(
                "asset.size",
                format!(
                    "asset product {} bytes {} exceed limit {}",
                    product.product_id, product.source_bytes, max_asset_bytes
                ),
            );
        }
        validate_product_against_vault(product, &catalog)?;
        assets.push(EditorAssetManifestEntry::from_product(product));
    }

    let mut manifest = EditorAssetManifest {
        schema: EDITOR_ASSET_MANIFEST_SCHEMA.to_string(),
        source_document_id: document.calyx_document_id.clone(),
        source_revision_id: document.revision_id.clone(),
        source_document_fingerprint: stable_fingerprint(document)?,
        asset_count: assets.len(),
        assets,
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = compute_editor_asset_manifest_fingerprint(&manifest)?;
    validate_editor_asset_manifest(&manifest)?;
    Ok(manifest)
}

pub fn write_editor_asset_manifest(
    path: &Path,
    manifest: &EditorAssetManifest,
    max_manifest_bytes: usize,
) -> Result<EditorAssetManifestWriteback, EngineError> {
    if max_manifest_bytes == 0 {
        return asset_manifest_error("manifest.size_limit", "max_manifest_bytes must be non-zero");
    }
    validate_editor_asset_manifest(manifest)?;
    let bytes =
        serde_json::to_vec_pretty(manifest).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    if bytes.len() > max_manifest_bytes {
        return asset_manifest_error(
            "manifest.size",
            format!(
                "manifest bytes {} exceed limit {}",
                bytes.len(),
                max_manifest_bytes
            ),
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_manifest.write.mkdir".to_string(),
            reason: format!("failed to create {}: {error}", parent.display()),
        })?;
    }
    fs::write(path, &bytes).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_manifest.write".to_string(),
        reason: format!("failed to write {}: {error}", path.display()),
    })?;
    let readback_bytes = fs::read(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_manifest.readback.read".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    if readback_bytes != bytes {
        return asset_manifest_error("readback.bytes", "manifest readback bytes changed");
    }
    let readback = read_editor_asset_manifest(path)?;
    if &readback != manifest {
        return asset_manifest_error("readback.manifest", "manifest readback did not match");
    }
    Ok(EditorAssetManifestWriteback {
        manifest_path: path.display().to_string(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        manifest_fingerprint: manifest.manifest_fingerprint.clone(),
        asset_count: manifest.asset_count,
    })
}

pub fn read_editor_asset_manifest(path: &Path) -> Result<EditorAssetManifest, EngineError> {
    let bytes = fs::read(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_manifest.read".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    let manifest: EditorAssetManifest =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    validate_editor_asset_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_editor_asset_manifest(manifest: &EditorAssetManifest) -> Result<(), EngineError> {
    if manifest.schema != EDITOR_ASSET_MANIFEST_SCHEMA {
        return asset_manifest_error("schema", "unexpected editor asset manifest schema");
    }
    if manifest.source_document_id.trim().is_empty()
        || manifest.source_revision_id.trim().is_empty()
        || manifest.source_document_fingerprint.trim().is_empty()
    {
        return asset_manifest_error(
            "source",
            "source document id, revision, and fingerprint are required",
        );
    }
    if manifest.asset_count == 0 || manifest.asset_count != manifest.assets.len() {
        return asset_manifest_error("asset_count", "asset_count must match non-empty assets");
    }
    let mut seen_products = BTreeSet::new();
    for entry in &manifest.assets {
        entry.validate()?;
        if !seen_products.insert(entry.product_id.clone()) {
            return asset_manifest_error(
                "asset.duplicate",
                format!("duplicate product id {}", entry.product_id),
            );
        }
    }
    let expected = compute_editor_asset_manifest_fingerprint(manifest)?;
    if manifest.manifest_fingerprint != expected {
        return asset_manifest_error(
            "manifest_fingerprint",
            format!(
                "manifest fingerprint mismatch expected {} got {}",
                expected, manifest.manifest_fingerprint
            ),
        );
    }
    Ok(())
}

pub fn compute_editor_asset_manifest_fingerprint(
    manifest: &EditorAssetManifest,
) -> Result<String, EngineError> {
    stable_fingerprint(&ManifestFingerprintPayload {
        schema: &manifest.schema,
        source_document_id: &manifest.source_document_id,
        source_revision_id: &manifest.source_revision_id,
        source_document_fingerprint: &manifest.source_document_fingerprint,
        asset_count: manifest.asset_count,
        assets: &manifest.assets,
    })
}

impl EditorAssetManifestEntry {
    fn from_product(product: &AssetProduct) -> Self {
        Self {
            product_id: product.product_id.clone(),
            product_kind: product.product_kind,
            source: product.source.clone(),
            asset_id: product.asset_id.clone(),
            title: product.title.clone(),
            license: product.license.clone(),
            license_url: product.license_url.clone(),
            source_url: product.source_url.clone(),
            source_relative_path: product.source_relative_path.clone(),
            source_sha256: product.source_sha256.clone(),
            source_bytes: product.source_bytes,
            dependencies: product.dependencies.clone(),
        }
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.product_id.trim().is_empty()
            || self.source.trim().is_empty()
            || self.asset_id.trim().is_empty()
            || self.source_relative_path.trim().is_empty()
        {
            return asset_manifest_error("asset.identity", "asset identity fields are required");
        }
        if self.license != "CC0-1.0" {
            return asset_manifest_error("asset.license", "asset manifest requires CC0-1.0");
        }
        validate_sha256("asset.source_sha256", &self.source_sha256)?;
        if self.source_bytes == 0 {
            return asset_manifest_error("asset.source_bytes", "asset bytes must be non-zero");
        }
        reject_escaping_path(&self.source_relative_path, "asset.source_relative_path")?;
        for dependency in &self.dependencies {
            reject_escaping_path(dependency, "asset.dependency")?;
        }
        Ok(())
    }
}

fn validate_product_against_vault(
    product: &AssetProduct,
    catalog: &AssetVaultCatalog,
) -> Result<(), EngineError> {
    let record = catalog
        .records()
        .iter()
        .find(|record| record.relative_path == product.source_relative_path)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_manifest.vault.record".to_string(),
            reason: format!("missing vault record {}", product.source_relative_path),
        })?;
    validate_record_matches_product(record, product)
}

fn validate_record_matches_product(
    record: &AssetVaultRecord,
    product: &AssetProduct,
) -> Result<(), EngineError> {
    if record.sha256 != product.source_sha256
        || record.bytes_on_disk != product.source_bytes
        || record.license != product.license
        || record.source != product.source
        || record.asset_id != product.asset_id
    {
        return asset_manifest_error(
            "vault.provenance",
            format!(
                "asset product {} does not match vault manifest",
                product.product_id
            ),
        );
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), EngineError> {
    if litd_hash::is_sha256_hex(value) {
        return Ok(());
    }
    asset_manifest_error(field, "expected lowercase SHA-256 hex")
}

fn reject_escaping_path(value: &str, field: &'static str) -> Result<(), EngineError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return asset_manifest_error(field, format!("path escapes asset manifest: {value}"));
    }
    Ok(())
}

fn asset_manifest_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_manifest.{}", field.into()),
        reason: reason.into(),
    })
}
