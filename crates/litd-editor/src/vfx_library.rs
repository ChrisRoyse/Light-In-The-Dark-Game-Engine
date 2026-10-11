use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{
    EngineError,
    render::vfx::{
        VfxEffectDescriptor, read_effect_from_content_store, validate_effect_descriptor,
    },
};
use serde::{Deserialize, Serialize};

#[path = "vfx_library_engine.rs"]
mod vfx_library_engine;
#[path = "vfx_library_preview.rs"]
mod vfx_library_preview;
#[path = "vfx_library_store.rs"]
mod vfx_library_store;

use vfx_library_engine::{dedup_index, record_from_effect, search_index};
pub use vfx_library_preview::render_vfx_library_preview;
pub use vfx_library_store::{
    VfxLibraryLedgerReadback, parse_vfx_library_ledger, read_vfx_library_ledger,
    write_vfx_library_ledger,
};

pub const VFX_LIBRARY_SCHEMA: &str = "litd.vfx.library.ledger.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderedPreviewLens {
    pub image_path: String,
    pub image_sha256: String,
    pub image_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub drawn_particles: u32,
    pub draw_calls: u32,
    pub lit_pixel_ratio: f32,
    pub hot_pixel_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibraryAssetInput {
    pub asset_id: String,
    pub descriptor: VfxEffectDescriptor,
    pub preview: VfxRenderedPreviewLens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibraryAssetRecord {
    pub asset_id: String,
    pub effect_id: String,
    pub display_name: String,
    pub descriptor: VfxEffectDescriptor,
    pub content_object_path: String,
    pub content_sha256: String,
    pub content_bytes: u64,
    pub total_max_particles: u32,
    pub module_signature: Vec<String>,
    pub terms: BTreeMap<String, u32>,
    pub tags: Vec<String>,
    pub preview: VfxRenderedPreviewLens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibraryIndex {
    pub schema: String,
    pub index_id: String,
    pub source_of_truth: String,
    pub asset_count: usize,
    pub records: Vec<VfxLibraryAssetRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxLibrarySearchRequest {
    pub query_id: String,
    pub query: String,
    pub limit: usize,
    pub requested_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxLibrarySearchStatus {
    Matched,
    NoMatches,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibrarySearchHit {
    pub rank: usize,
    pub asset_id: String,
    pub effect_id: String,
    pub score: u32,
    pub matched_terms: Vec<String>,
    pub descriptor: VfxEffectDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibrarySearchReport {
    pub query_id: String,
    pub query: String,
    pub status: VfxLibrarySearchStatus,
    pub hits: Vec<VfxLibrarySearchHit>,
    pub source_index_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxDedupVerdict {
    ExactDuplicate,
    NearDuplicate,
    Distinct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxDedupPair {
    pub left_asset_id: String,
    pub right_asset_id: String,
    pub verdict: VfxDedupVerdict,
    pub semantic_score: u32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxLibraryLedger {
    pub schema: String,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub index: VfxLibraryIndex,
    pub search_reports: Vec<VfxLibrarySearchReport>,
    pub dedup_pairs: Vec<VfxDedupPair>,
}

pub fn build_vfx_library_index(
    store_root: impl AsRef<Path>,
    index_id: impl Into<String>,
    inputs: Vec<VfxLibraryAssetInput>,
) -> Result<VfxLibraryIndex, EngineError> {
    let index_id = index_id.into();
    validate_single_line("index_id", &index_id)?;
    if inputs.is_empty() {
        return invalid_vfx_library("index.inputs", "at least one effect asset is required");
    }
    let mut records = Vec::with_capacity(inputs.len());
    let mut seen = BTreeSet::new();
    for input in inputs {
        validate_single_line("asset_id", &input.asset_id)?;
        validate_effect_descriptor(&input.descriptor)?;
        if !seen.insert(input.asset_id.clone()) {
            return invalid_vfx_library("index.asset_id", "asset ids must be unique");
        }
        let readback = read_effect_from_content_store(&store_root, &input.descriptor)?;
        records.push(record_from_effect(
            input,
            readback.effect,
            &readback.object_path,
        )?);
    }
    records.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
    let index = VfxLibraryIndex {
        schema: VFX_LIBRARY_SCHEMA.to_string(),
        index_id,
        source_of_truth: "content-addressed VFX effect objects + rendered preview image lens"
            .to_string(),
        asset_count: records.len(),
        records,
    };
    validate_vfx_library_index(&index)?;
    Ok(index)
}

pub fn search_available_vfx_library(
    index: Option<&VfxLibraryIndex>,
    request: &VfxLibrarySearchRequest,
) -> Result<VfxLibrarySearchReport, EngineError> {
    let Some(index) = index else {
        return invalid_vfx_library(
            "offline",
            "VFX library index is unavailable; search is disabled",
        );
    };
    search_vfx_library(index, request)
}

pub fn search_vfx_library(
    index: &VfxLibraryIndex,
    request: &VfxLibrarySearchRequest,
) -> Result<VfxLibrarySearchReport, EngineError> {
    validate_vfx_library_index(index)?;
    validate_search_request(request)?;
    search_index(index, request)
}

pub fn dedup_vfx_library(index: &VfxLibraryIndex) -> Result<Vec<VfxDedupPair>, EngineError> {
    validate_vfx_library_index(index)?;
    dedup_index(index)
}

pub fn build_vfx_library_ledger(
    ledger_id: impl Into<String>,
    index: VfxLibraryIndex,
    search_reports: Vec<VfxLibrarySearchReport>,
    dedup_pairs: Vec<VfxDedupPair>,
) -> Result<VfxLibraryLedger, EngineError> {
    let ledger = VfxLibraryLedger {
        schema: VFX_LIBRARY_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        source_of_truth: "Aster VFX library ledger + content store readback".to_string(),
        index,
        search_reports,
        dedup_pairs,
    };
    validate_vfx_library_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_vfx_library_ledger(ledger: &VfxLibraryLedger) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != VFX_LIBRARY_SCHEMA || !ledger.source_of_truth.contains("Aster") {
        return invalid_vfx_library("ledger", "schema and source of truth are invalid");
    }
    validate_vfx_library_index(&ledger.index)?;
    for report in &ledger.search_reports {
        if report.source_index_id != ledger.index.index_id {
            return invalid_vfx_library("ledger.search", "search report index mismatch");
        }
    }
    Ok(())
}

pub fn validate_vfx_library_index(index: &VfxLibraryIndex) -> Result<(), EngineError> {
    validate_single_line("index_id", &index.index_id)?;
    if index.schema != VFX_LIBRARY_SCHEMA
        || index.asset_count == 0
        || index.asset_count != index.records.len()
        || !index.source_of_truth.contains("content-addressed")
    {
        return invalid_vfx_library("index", "schema, source, and asset count are invalid");
    }
    let mut seen = BTreeSet::new();
    for record in &index.records {
        validate_record(record)?;
        if !seen.insert(record.asset_id.as_str()) {
            return invalid_vfx_library("index.asset_id", "duplicate asset id");
        }
    }
    Ok(())
}

fn validate_record(record: &VfxLibraryAssetRecord) -> Result<(), EngineError> {
    validate_single_line("record.asset_id", &record.asset_id)?;
    validate_single_line("record.effect_id", &record.effect_id)?;
    validate_effect_descriptor(&record.descriptor)?;
    if record.content_sha256 != record.descriptor.sha256()?
        || record.content_bytes != record.descriptor.size
        || record.module_signature.is_empty()
        || record.terms.is_empty()
        || record.tags.is_empty()
        || record.preview.image_sha256.trim().is_empty()
    {
        return invalid_vfx_library(
            "record",
            "descriptor, terms, tags, preview, and content readback must be populated",
        );
    }
    Ok(())
}

fn validate_search_request(request: &VfxLibrarySearchRequest) -> Result<(), EngineError> {
    validate_single_line("query_id", &request.query_id)?;
    validate_single_line("requested_by", &request.requested_by)?;
    validate_non_empty("query", &request.query)?;
    if request.limit == 0 || request.limit > 64 {
        return invalid_vfx_library("search.limit", "limit must be 1..=64");
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    validate_non_empty(field, value)?;
    if value.contains(['\n', '\r', '\t']) {
        return invalid_vfx_library(field, "value must be single-line");
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx_library(field, "value must be non-empty");
    }
    Ok(())
}

fn invalid_vfx_library<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(library_error(field, reason))
}

fn library_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.vfx_library.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "vfx_library_tests.rs"]
mod tests;
