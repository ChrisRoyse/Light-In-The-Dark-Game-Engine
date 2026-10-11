use litd_hash::sha256_hex;
use std::{
    marker::PhantomData,
    path::Path,
    sync::Arc,
    thread::{self, JoinHandle},
};

use litd_engine::EngineError;

use crate::{
    asset_products::AssetProductKind,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};

#[derive(Debug, Clone)]
pub struct ContentAddressedAssetStore {
    catalog: Arc<AssetVaultCatalog>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshAsset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureAsset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioAsset;

pub trait AssetKind: Send + Sync + 'static {
    const PRODUCT_KIND: AssetProductKind;
    const LABEL: &'static str;
}

impl AssetKind for MeshAsset {
    const PRODUCT_KIND: AssetProductKind = AssetProductKind::Mesh;
    const LABEL: &'static str = "mesh";
}

impl AssetKind for TextureAsset {
    const PRODUCT_KIND: AssetProductKind = AssetProductKind::Texture;
    const LABEL: &'static str = "texture";
}

impl AssetKind for AudioAsset {
    const PRODUCT_KIND: AssetProductKind = AssetProductKind::Audio;
    const LABEL: &'static str = "audio";
}

#[derive(Debug, PartialEq, Eq)]
pub struct AssetHandle<K: AssetKind> {
    handle_id: String,
    relative_path: String,
    content_sha256: String,
    bytes_on_disk: u64,
    _kind: PhantomData<fn() -> K>,
}

#[derive(Debug)]
pub struct AssetLoadReceipt<K: AssetKind> {
    pub handle: AssetHandle<K>,
    pub bytes: Arc<[u8]>,
    pub verified_sha256: String,
    pub bytes_read: u64,
    pub source_absolute_path: String,
    pub loaded_on_background_thread: bool,
}

#[derive(Debug)]
pub struct AssetLoadJob<K: AssetKind> {
    join: JoinHandle<Result<AssetLoadReceipt<K>, EngineError>>,
}

impl ContentAddressedAssetStore {
    pub fn load_default_cached() -> Result<Self, EngineError> {
        Ok(Self::from_catalog(AssetVaultCatalog::load_default_cached()?))
    }

    pub fn from_catalog(catalog: Arc<AssetVaultCatalog>) -> Self {
        Self { catalog }
    }

    pub fn catalog(&self) -> &AssetVaultCatalog {
        &self.catalog
    }

    pub fn handle_for_record<K: AssetKind>(
        &self,
        record: &AssetVaultRecord,
    ) -> Result<AssetHandle<K>, EngineError> {
        let actual = product_kind_for_path(&record.relative_path)?;
        if actual != K::PRODUCT_KIND {
            return asset_store_error(
                "handle.kind",
                format!(
                    "{} is {:?}, not requested {}",
                    record.relative_path,
                    actual,
                    K::LABEL
                ),
            );
        }
        AssetHandle::new(
            record.relative_path.clone(),
            record.sha256.clone(),
            record.bytes_on_disk,
        )
    }

    pub fn load<K: AssetKind>(
        &self,
        handle: AssetHandle<K>,
    ) -> Result<AssetLoadReceipt<K>, EngineError> {
        self.load_internal(handle, false)
    }

    pub fn load_background<K: AssetKind>(&self, handle: AssetHandle<K>) -> AssetLoadJob<K> {
        let store = self.clone();
        AssetLoadJob {
            join: thread::spawn(move || store.load_internal(handle, true)),
        }
    }

    fn load_internal<K: AssetKind>(
        &self,
        handle: AssetHandle<K>,
        loaded_on_background_thread: bool,
    ) -> Result<AssetLoadReceipt<K>, EngineError> {
        let record = self.record_for_handle(&handle)?;
        let bytes = std::fs::read(&record.absolute_path).map_err(|error| {
            EngineError::UiRenderInvalidConfig {
                field: "editor.asset_store.file.read".to_string(),
                reason: format!("failed to read {}: {error}", record.absolute_path.display()),
            }
        })?;
        if bytes.len() as u64 != handle.bytes_on_disk {
            return asset_store_error(
                "file.bytes",
                format!(
                    "{} handle_bytes={} actual_bytes={}",
                    record.relative_path,
                    handle.bytes_on_disk,
                    bytes.len()
                ),
            );
        }
        let actual_sha256 = sha256_hex(&bytes);
        if actual_sha256 != handle.content_sha256 {
            return asset_store_error(
                "file.sha256",
                format!(
                    "{} handle_sha256={} actual_sha256={}",
                    record.relative_path, handle.content_sha256, actual_sha256
                ),
            );
        }
        let bytes_read = bytes.len() as u64;
        Ok(AssetLoadReceipt {
            handle,
            bytes: Arc::from(bytes),
            verified_sha256: actual_sha256,
            bytes_read,
            source_absolute_path: record.absolute_path.display().to_string(),
            loaded_on_background_thread,
        })
    }

    fn record_for_handle<K: AssetKind>(
        &self,
        handle: &AssetHandle<K>,
    ) -> Result<AssetVaultRecord, EngineError> {
        let Some(record) = self
            .catalog
            .records()
            .iter()
            .find(|record| record.relative_path == handle.relative_path)
        else {
            return asset_store_error(
                "handle.relative_path",
                format!("missing asset record {}", handle.relative_path),
            );
        };
        let actual = product_kind_for_path(&record.relative_path)?;
        if actual != K::PRODUCT_KIND {
            return asset_store_error(
                "handle.kind",
                format!(
                    "{} resolved as {:?}, not {}",
                    record.relative_path,
                    actual,
                    K::LABEL
                ),
            );
        }
        if record.sha256 != handle.content_sha256 {
            return asset_store_error(
                "handle.sha256",
                format!(
                    "{} manifest_sha256={} handle_sha256={}",
                    record.relative_path, record.sha256, handle.content_sha256
                ),
            );
        }
        if record.bytes_on_disk != handle.bytes_on_disk {
            return asset_store_error(
                "handle.bytes",
                format!(
                    "{} manifest_bytes={} handle_bytes={}",
                    record.relative_path, record.bytes_on_disk, handle.bytes_on_disk
                ),
            );
        }
        Ok(record.clone())
    }
}

impl<K: AssetKind> AssetHandle<K> {
    pub fn new(
        relative_path: impl Into<String>,
        content_sha256: impl Into<String>,
        bytes_on_disk: u64,
    ) -> Result<Self, EngineError> {
        let relative_path = relative_path.into();
        let content_sha256 = content_sha256.into().to_ascii_lowercase();
        if relative_path.trim().is_empty() || Path::new(&relative_path).is_absolute() {
            return asset_store_error("handle.relative_path", "relative path is required");
        }
        if !is_sha256_hex(&content_sha256) {
            return asset_store_error("handle.sha256", "content SHA-256 must be 64 lowercase hex");
        }
        if bytes_on_disk == 0 {
            return asset_store_error("handle.bytes", "bytes_on_disk must be non-zero");
        }
        Ok(Self {
            handle_id: format!("litd.asset.{}.sha256.{}", K::LABEL, content_sha256),
            relative_path,
            content_sha256,
            bytes_on_disk,
            _kind: PhantomData,
        })
    }

    pub fn handle_id(&self) -> &str {
        &self.handle_id
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }

    pub fn bytes_on_disk(&self) -> u64 {
        self.bytes_on_disk
    }

    pub fn asset_kind_label(&self) -> &'static str {
        K::LABEL
    }
}

impl<K: AssetKind> Clone for AssetHandle<K> {
    fn clone(&self) -> Self {
        Self {
            handle_id: self.handle_id.clone(),
            relative_path: self.relative_path.clone(),
            content_sha256: self.content_sha256.clone(),
            bytes_on_disk: self.bytes_on_disk,
            _kind: PhantomData,
        }
    }
}

impl<K: AssetKind> AssetLoadJob<K> {
    pub fn join(self) -> Result<AssetLoadReceipt<K>, EngineError> {
        self.join
            .join()
            .map_err(|_| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_store.background.join".to_string(),
                reason: "asset load worker panicked".to_string(),
            })?
    }
}

fn product_kind_for_path(relative_path: &str) -> Result<AssetProductKind, EngineError> {
    match Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("fbx" | "obj" | "glb" | "gltf") => Ok(AssetProductKind::Mesh),
        Some("png" | "jpg" | "jpeg" | "ktx2" | "basis") => Ok(AssetProductKind::Texture),
        Some("wav" | "ogg" | "flac") => Ok(AssetProductKind::Audio),
        _ => asset_store_error("source.extension", "unsupported asset handle extension"),
    }
}

use litd_hash::is_sha256_hex;

fn asset_store_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_store.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "asset_store_tests.rs"]
mod tests;
