use std::collections::BTreeSet;
#[cfg(test)]
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use litd_editor::{
    asset_placement::ContentBrowserAssetRef,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};
use litd_engine::EngineError;

use crate::main_editor_assets::AssetIcon;
use crate::main_editor_thumbnails::AssetThumbnail;

use super::invalid_shell_runtime;

#[derive(Debug, Clone)]
pub(super) struct AssetEntry {
    pub(super) label: String,
    pub(super) icon: AssetIcon,
    pub(super) source: String,
    pub(super) asset_id: String,
    pub(super) asset_type: String,
    pub(super) title: String,
    pub(super) license: String,
    pub(super) license_url: String,
    pub(super) source_url: String,
    pub(super) relative_path: String,
    pub(super) bytes_on_disk: u64,
    pub(super) sha256: String,
    pub(super) status: String,
    pub(super) thumbnail: AssetThumbnail,
}

impl AssetEntry {
    pub(super) fn supports_model_previewer(&self) -> bool {
        let path = self.relative_path.to_ascii_lowercase();
        path.ends_with(".gltf") || path.ends_with(".glb")
    }

    pub(super) fn placement_ref(&self) -> ContentBrowserAssetRef {
        ContentBrowserAssetRef {
            source: self.source.clone(),
            asset_id: self.asset_id.clone(),
            asset_type: self.asset_type.clone(),
            title: self.title.clone(),
            relative_path: self.relative_path.clone(),
            bytes_on_disk: self.bytes_on_disk,
            sha256: self.sha256.clone(),
        }
    }
}

pub(super) fn build_asset_entries(
    catalog: &AssetVaultCatalog,
) -> Result<Vec<AssetEntry>, EngineError> {
    let mut entries = Vec::new();
    let mut thumbnail_paths = BTreeSet::new();
    for record in catalog.editor_palette_records(catalog.usable_record_count()) {
        let Some(thumbnail) = AssetThumbnail::resolve(record)? else {
            continue;
        };
        if !thumbnail_paths.insert(thumbnail.path().to_path_buf()) {
            continue;
        }
        entries.push(AssetEntry {
            label: asset_label(record),
            icon: icon_for_asset(record),
            source: record.source.clone(),
            asset_id: record.asset_id.clone(),
            asset_type: record.asset_type.clone(),
            title: record.title.clone(),
            license: record.license.clone(),
            license_url: record.license_url.clone(),
            source_url: record.source_url.clone(),
            relative_path: record.relative_path.clone(),
            bytes_on_disk: record.bytes_on_disk,
            sha256: record.sha256.clone(),
            status: record
                .status
                .clone()
                .unwrap_or_else(|| "extracted".to_string()),
            thumbnail,
        });
        if entries.len() == 96 {
            break;
        }
    }
    if entries.len() < 96 {
        return invalid_shell_runtime(
            "asset_catalog.unique_thumbnails",
            format!(
                "vault has {} usable records but only {} unique real thumbnail/previews",
                catalog.usable_record_count(),
                entries.len()
            ),
        );
    }
    if entries.is_empty() {
        return invalid_shell_runtime(
            "asset_catalog",
            format!(
                "vault has {} records but no editor-usable FBX/OBJ/glTF/image assets",
                catalog.record_count()
            ),
        );
    }
    Ok(entries)
}

pub(super) fn build_runtime_asset_entries(
    catalog: &AssetVaultCatalog,
) -> Result<Vec<AssetEntry>, EngineError> {
    #[cfg(test)]
    {
        build_asset_entries_cached_for_tests(catalog)
    }
    #[cfg(not(test))]
    {
        build_asset_entries(catalog)
    }
}

/// Memoized per-catalog asset-entry build results for the test path, keyed by catalog
/// cache key. `Result<_, String>` because the cached error is stringified for reuse.
#[cfg(test)]
type AssetEntriesCache = OnceLock<Mutex<BTreeMap<String, Result<Vec<AssetEntry>, String>>>>;

#[cfg(test)]
fn build_asset_entries_cached_for_tests(
    catalog: &AssetVaultCatalog,
) -> Result<Vec<AssetEntry>, EngineError> {
    static ENTRIES: AssetEntriesCache = OnceLock::new();

    let catalog_key = catalog_cache_key_for_tests(catalog);
    let cache = ENTRIES.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.shell.runtime.asset_catalog.cache".to_string(),
            reason: format!("test asset entry cache lock poisoned: {error}"),
        })?;
    if let Some(cached) = cache.get(&catalog_key) {
        return match cached.clone() {
            Ok(entries) => Ok(entries),
            Err(reason) => invalid_shell_runtime("asset_catalog.cache", reason),
        };
    }
    let built = build_asset_entries(catalog).map_err(|error| {
        format!("failed to build verified test asset entries for catalog {catalog_key}: {error}")
    });
    cache.insert(catalog_key, built.clone());
    match built {
        Ok(entries) => Ok(entries),
        Err(reason) => invalid_shell_runtime("asset_catalog.cache", reason),
    }
}

#[cfg(test)]
pub(super) fn catalog_cache_key_for_tests(catalog: &AssetVaultCatalog) -> String {
    format!(
        "ptr={:p};records={};usable={};manifests={};sources={}",
        catalog,
        catalog.record_count(),
        catalog.usable_record_count(),
        catalog.manifest_summary_line(),
        catalog.download_source_summary_line()
    )
}

pub(super) fn default_model_preview_asset_index(
    entries: &[AssetEntry],
) -> Result<usize, EngineError> {
    entries
        .iter()
        .position(AssetEntry::supports_model_previewer)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.shell.runtime.asset_catalog.model_preview_default".to_string(),
            reason: "editor palette must include at least one real glTF/GLB asset for the startup model previewer"
                .to_string(),
        })
}

pub(super) fn compact_asset_label(label: &str) -> String {
    const MAX_CHARS: usize = 18;
    if label.chars().count() <= MAX_CHARS {
        label.to_string()
    } else {
        let mut compact = label.chars().take(MAX_CHARS - 3).collect::<String>();
        compact.push_str("...");
        compact
    }
}

fn asset_label(record: &AssetVaultRecord) -> String {
    format!("{} {}", record.source, record.file_stem())
}

fn icon_for_asset(record: &AssetVaultRecord) -> AssetIcon {
    let key = format!(
        "{} {} {} {}",
        record.source, record.asset_id, record.title, record.relative_path
    )
    .to_ascii_lowercase();
    if key.contains("wizard") || key.contains("mage") || key.contains("magic") {
        AssetIcon::Wizard
    } else if key.contains("knight")
        || key.contains("character")
        || key.contains("unit")
        || key.contains("warrior")
    {
        AssetIcon::Knight
    } else if key.contains("fire") || key.contains("torch") || key.contains("candle") {
        AssetIcon::Fire
    } else if key.contains("rune") || key.contains("spell") {
        AssetIcon::Rune
    } else if key.contains("tomb") || key.contains("grave") || key.contains("skull") {
        AssetIcon::Tombstone
    } else if key.contains("tree") || key.contains("bush") || key.contains("leaf") {
        AssetIcon::Tree
    } else if key.contains("rock") || key.contains("stone") || key.contains("cliff") {
        AssetIcon::Rocks
    } else {
        AssetIcon::Tower
    }
}
