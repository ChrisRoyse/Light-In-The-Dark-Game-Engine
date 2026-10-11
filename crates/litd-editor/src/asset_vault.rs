use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use litd_engine::EngineError;

#[path = "asset_vault_manifest.rs"]
mod manifest;
#[path = "asset_vault_p0_floors.rs"]
mod p0_floors;
#[path = "asset_vault_semantics.rs"]
mod semantics;
#[path = "asset_vault_verify_cache.rs"]
mod verify_cache;

pub use p0_floors::ISSUE1242_PROGRESSIVE_STATUS;

#[cfg(test)]
use manifest::is_verified_download_status;
use manifest::{asset_vault_error, load_manifest_records};
pub use semantics::{
    AssetRoleInput, AssetSemanticProfile, AssetVaultSemanticCoverage, AssetVaultSemanticIndex,
    ISSUE1242_SEMANTIC_STATUS, ROLE_NAMES, classify_roles, required_role_floors,
};

static DEFAULT_CATALOG: OnceLock<Result<Arc<AssetVaultCatalog>, String>> = OnceLock::new();

// #1242 floor constants + progressive residual evaluation live in asset_vault_p0_floors.rs
// (#1878 editor-boot path).

#[derive(Debug, Clone)]
pub struct AssetVaultCatalog {
    manifest_paths: Vec<PathBuf>,
    records: Vec<AssetVaultRecord>,
    download_source_counts: BTreeMap<String, usize>,
    semantic_index_cache: Arc<OnceLock<Result<Arc<AssetVaultSemanticIndex>, String>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVaultCoverageSummary {
    pub record_types: String,
    pub asset_types: String,
    pub licenses: String,
    pub semantic_role_counts: String,
    pub semantic_profile_count: usize,
    pub semantic_classified_profile_count: usize,
    pub semantic_unclassified_profile_count: usize,
    pub semantic_tag_count: usize,
    pub download_bytes_on_disk: u64,
    pub extraction_bytes_on_disk: u64,
    pub issue1242_p0_status: &'static str,
}

#[derive(Debug, Clone)]
pub struct AssetVaultRecord {
    pub record_type: String,
    pub source: String,
    pub asset_id: String,
    pub asset_type: String,
    pub title: String,
    pub license: String,
    pub license_url: String,
    pub source_url: String,
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub bytes_on_disk: u64,
    pub sha256: String,
    pub status: Option<String>,
}

impl AssetVaultCatalog {
    pub fn load_default_cached() -> Result<Arc<Self>, EngineError> {
        let result = DEFAULT_CATALOG.get_or_init(|| {
            Self::load_default()
                .map(Arc::new)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(catalog) => Ok(catalog.clone()),
            Err(reason) => asset_vault_error("default_catalog", reason.clone()),
        }
    }

    /// Load the vault catalog from the download intake manifest, plus the extraction
    /// manifest when the extract phase has already written it.
    ///
    /// Download is always required. Extraction is optional during progressive rematerialization
    /// (#1878): kenney/zip extract may not have produced `asset_extraction_manifest.jsonl` yet.
    /// A present-but-corrupt extraction file still fails loud via [`load_manifest_records`].
    pub fn load_default() -> Result<Self, EngineError> {
        let download = Self::download_manifest_path()?;
        if !download.exists() {
            return asset_vault_error(
                "manifest.path",
                format!("download manifest does not exist: {}", download.display()),
            );
        }
        let mut paths = vec![download.clone()];
        let mut records = load_manifest_records(&download, "download")?;
        let extraction = Self::extraction_manifest_path()?;
        if extraction.exists() {
            records.extend(load_manifest_records(&extraction, "extraction")?);
            paths.push(extraction);
        } else {
            // Loud diagnostic (not silent fallback): extract phase has not run yet.
            eprintln!(
                "ASSET_VAULT_EXTRACTION_PENDING path={} detail=extraction manifest absent; \
                 catalog loaded from download intake only (#1878 progressive rematerialization)",
                extraction.display()
            );
        }
        Self::from_records(paths, records)
    }

    pub fn load_from_manifest(
        path: impl AsRef<Path>,
        expected_record_type: &str,
    ) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        let records = load_manifest_records(&path, expected_record_type)?;
        Self::from_records(vec![path], records)
    }

    // #1543: the vault root is resolved at RUNTIME through the one authoritative resolver.
    // The old `env!("CARGO_MANIFEST_DIR")` arithmetic baked the compiling checkout's path into
    // the binary, so linked git worktrees looked for a vault that does not exist there.
    pub fn default_manifest_paths() -> Result<Vec<PathBuf>, EngineError> {
        Ok(vec![
            Self::download_manifest_path()?,
            Self::extraction_manifest_path()?,
        ])
    }

    fn manifests_dir() -> Result<PathBuf, EngineError> {
        Ok(litd_engine::asset_vault_root::local_asset_vault_root()
            .map_err(|error| EngineError::AssetVaultUnavailable(error.to_string()))?
            .join("manifests"))
    }

    fn download_manifest_path() -> Result<PathBuf, EngineError> {
        Ok(Self::manifests_dir()?.join("asset_intake_manifest.jsonl"))
    }

    fn extraction_manifest_path() -> Result<PathBuf, EngineError> {
        Ok(Self::manifests_dir()?.join("asset_extraction_manifest.jsonl"))
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn records(&self) -> &[AssetVaultRecord] {
        &self.records
    }

    pub fn download_source_count(&self, source: &str) -> usize {
        self.download_source_counts
            .get(source)
            .copied()
            .unwrap_or(0)
    }

    pub fn usable_record_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.is_editor_usable())
            .count()
    }

    pub fn download_source_summary_line(&self) -> String {
        summary_line(&self.download_source_counts)
    }

    // coverage_summary / coverage_summary_for_editor_boot / issue1242_p0_residual_report
    // are implemented in asset_vault_p0_floors.rs (#1242 / #1878).

    pub fn semantic_index(&self) -> Result<AssetVaultSemanticIndex, EngineError> {
        Ok((*self.semantic_index_cached()?).clone())
    }

    pub(super) fn semantic_index_cached(
        &self,
    ) -> Result<Arc<AssetVaultSemanticIndex>, EngineError> {
        let result = self.semantic_index_cache.get_or_init(|| {
            AssetVaultSemanticIndex::from_records(&self.records)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(index) => Ok(index.clone()),
            Err(reason) => asset_vault_error("semantic.index", reason.clone()),
        }
    }

    pub fn semantic_coverage_summary(&self) -> Result<AssetVaultSemanticCoverage, EngineError> {
        self.semantic_index()?.coverage()
    }

    pub fn record_type_count(&self, record_type: &str) -> usize {
        self.records
            .iter()
            .filter(|record| record.record_type == record_type)
            .count()
    }

    pub fn asset_type_count(&self, asset_type: &str) -> usize {
        self.records
            .iter()
            .filter(|record| record.asset_type == asset_type)
            .count()
    }

    pub fn manifest_summary_line(&self) -> String {
        self.manifest_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }

    pub fn editor_palette_records(&self, limit: usize) -> Vec<&AssetVaultRecord> {
        let mut records = self
            .records
            .iter()
            .filter(|record| record.is_editor_usable())
            .collect::<Vec<_>>();
        records.sort_by(|a, b| {
            record_score(b)
                .cmp(&record_score(a))
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        let mut selected = Vec::new();
        let mut selected_paths = BTreeSet::new();
        for source in ["quaternius", "kaykit", "kenney", "polyhaven", "ambientcg"] {
            for record in records
                .iter()
                .copied()
                .filter(|record| record.source == source)
                .take(2)
            {
                push_unique_record(limit, record, &mut selected, &mut selected_paths);
            }
        }
        for record in records {
            push_unique_record(limit, record, &mut selected, &mut selected_paths);
        }
        selected
    }

    pub(super) fn record_type_summary_line(&self) -> String {
        summary_line(&self.counts_by(|record| record.record_type.as_str()))
    }

    pub(super) fn asset_type_summary_line(&self) -> String {
        summary_line(&self.counts_by(|record| record.asset_type.as_str()))
    }

    pub(super) fn license_summary_line(&self) -> String {
        summary_line(&self.counts_by(|record| record.license.as_str()))
    }

    fn counts_by(&self, key: impl Fn(&AssetVaultRecord) -> &str) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for record in &self.records {
            *counts.entry(key(record).to_string()).or_insert(0) += 1;
        }
        counts
    }

    pub(super) fn bytes_by_record_type(&self, record_type: &str) -> u64 {
        self.records
            .iter()
            .filter(|record| record.record_type == record_type)
            .map(|record| record.bytes_on_disk)
            .sum()
    }

    fn from_records(
        manifest_paths: Vec<PathBuf>,
        records: Vec<AssetVaultRecord>,
    ) -> Result<Self, EngineError> {
        if records.is_empty() {
            return asset_vault_error("manifest.records", "no asset records were loaded");
        }
        let mut download_source_counts = BTreeMap::new();
        for record in &records {
            match record.record_type.as_str() {
                "download" => {
                    *download_source_counts
                        .entry(record.source.clone())
                        .or_insert(0) += 1;
                }
                "extraction" => {}
                other => {
                    return asset_vault_error(
                        "manifest.record_type",
                        format!("unsupported record_type '{other}'"),
                    );
                }
            }
        }
        Ok(Self {
            manifest_paths,
            records,
            download_source_counts,
            semantic_index_cache: Arc::new(OnceLock::new()),
        })
    }
}

fn push_unique_record<'a>(
    limit: usize,
    record: &'a AssetVaultRecord,
    selected: &mut Vec<&'a AssetVaultRecord>,
    selected_paths: &mut BTreeSet<String>,
) {
    if selected.len() < limit && selected_paths.insert(record.relative_path.clone()) {
        selected.push(record);
    }
}

impl AssetVaultRecord {
    pub fn is_editor_usable(&self) -> bool {
        let extension = Path::new(&self.relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());
        matches!(
            extension.as_deref(),
            Some(
                "fbx"
                    | "obj"
                    | "glb"
                    | "gltf"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "ktx2"
                    | "basis"
                    | "wav"
                    | "ogg"
                    | "flac",
            )
        )
    }

    pub fn file_stem(&self) -> String {
        Path::new(&self.relative_path)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(&self.asset_id)
            .to_string()
    }
}

fn record_score(record: &AssetVaultRecord) -> i32 {
    let key = format!(
        "{} {} {} {}",
        record.source, record.asset_id, record.title, record.relative_path
    )
    .to_ascii_lowercase();
    let mut score = match record.source.as_str() {
        "kaykit" => 400,
        "quaternius" => 360,
        "kenney" => 320,
        "polyhaven" => 240,
        _ => 100,
    };
    for (needle, weight) in [
        ("fbx", 80),
        ("glb", 75),
        ("gltf", 75),
        ("dungeon", 70),
        ("medieval", 65),
        ("castle", 60),
        ("ruin", 55),
        ("building", 45),
        ("character", 45),
        ("tree", 35),
        ("rock", 30),
        ("texture", 20),
    ] {
        if key.contains(needle) {
            score += weight;
        }
    }
    score
}

fn summary_line(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(source, count)| format!("{source}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "asset_vault_tests.rs"]
mod tests;
