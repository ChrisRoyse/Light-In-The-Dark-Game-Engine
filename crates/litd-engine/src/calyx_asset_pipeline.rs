//! Calyx-backed asset ingest, dedup, provenance, and editor intelligence.

use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentKind, EngineContentRecord,
        EngineContentSource,
    },
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_semantic::{
        EngineCalyxSemanticError, build_engine_calyx_semantic_index,
        store_engine_calyx_semantic_index,
    },
};

pub const ENGINE_CALYX_ASSET_PIPELINE_SCHEMA: &str = "litd.engine.calyx_asset_pipeline.v1";
pub const ENGINE_CALYX_ASSET_PIPELINE_READBACK_SCHEMA: &str =
    "litd.engine.calyx_asset_pipeline_readback.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineAssetMediaType {
    Mesh,
    Texture,
    Audio,
    Vfx,
    Material,
    Document,
}

impl EngineAssetMediaType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Texture => "texture",
            Self::Audio => "audio",
            Self::Vfx => "vfx",
            Self::Material => "material",
            Self::Document => "document",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAssetIngestRequest {
    pub asset_id: String,
    pub display_name: String,
    pub media_type: EngineAssetMediaType,
    pub source_uri: String,
    pub bytes_sha256: String,
    pub byte_len: u64,
    pub tags: Vec<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAssetRecord {
    pub asset_id: String,
    pub display_name: String,
    pub media_type: EngineAssetMediaType,
    pub source_uri: String,
    pub bytes_sha256: String,
    pub byte_len: u64,
    pub tags: Vec<String>,
    pub provenance_refs: Vec<String>,
    pub content_cx_id: String,
    pub content_fingerprint: String,
    pub metadata_sha256: String,
    pub deduplicated: bool,
    pub canonical_asset_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAssetDedupGroup {
    pub bytes_sha256: String,
    pub canonical_asset_id: String,
    pub duplicate_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAssetPipelineReadback {
    pub schema: String,
    pub pipeline_id: String,
    pub asset_count: usize,
    pub deduplicated_count: usize,
    pub dedup_groups: Vec<EngineCalyxAssetDedupGroup>,
    pub records: Vec<EngineCalyxAssetRecord>,
    pub semantic_index_id: String,
    pub semantic_index_fingerprint: String,
    pub pipeline_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxAssetPipelineStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub pipeline: EngineCalyxAssetPipelineReadback,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxAssetPipelineError {
    #[error("engine Calyx asset pipeline field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx asset content failed: {0}")]
    Content(#[from] EngineCalyxContentError),
    #[error("engine Calyx asset persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx asset semantic index failed: {0}")]
    Semantic(#[from] EngineCalyxSemanticError),
    #[error("engine Calyx asset codec failed: {0}")]
    Codec(String),
}

pub fn ingest_assets_into_calyx(
    database: &EngineCalyxDatabase,
    pipeline_id: impl Into<String>,
    requests: &[EngineAssetIngestRequest],
) -> Result<EngineCalyxAssetPipelineStoredReadback, EngineCalyxAssetPipelineError> {
    let pipeline_id = pipeline_id.into();
    validate_requests(&pipeline_id, requests)?;
    let content_store = EngineCalyxContentStore::new(database.clone());
    let canonical_by_hash = canonical_assets(requests);
    let dedup_groups = dedup_groups(requests, &canonical_by_hash);
    let mut content_readbacks = Vec::new();
    let mut records = Vec::new();
    for request in requests {
        let canonical_asset_id = canonical_by_hash
            .get(&request.bytes_sha256)
            .cloned()
            .unwrap_or_else(|| request.asset_id.clone());
        let deduplicated = canonical_asset_id != request.asset_id;
        let payload = asset_payload(request, deduplicated, &canonical_asset_id);
        let content = EngineContentRecord {
            kind: EngineContentKind::Asset,
            id: request.asset_id.clone(),
            display_name: request.display_name.clone(),
            payload: payload.clone(),
            source: EngineContentSource {
                path: request.source_uri.clone(),
                author: "calyx_asset_pipeline".to_string(),
                source_hash: request.bytes_sha256.clone(),
            },
            anchors: Vec::new(),
        };
        let content_readback = content_store.put_content(&content)?;
        let metadata = database.put_record(&EngineCalyxRecord::new(
            EngineCalyxRecordKind::AssetMetadata,
            request.asset_id.clone(),
            payload,
            json!({
                "source_issue": 1122,
                "source_uri": request.source_uri,
                "bytes_sha256": request.bytes_sha256,
                "provenance_refs": request.provenance_refs
            }),
        ))?;
        records.push(EngineCalyxAssetRecord {
            asset_id: request.asset_id.clone(),
            display_name: request.display_name.clone(),
            media_type: request.media_type,
            source_uri: request.source_uri.clone(),
            bytes_sha256: request.bytes_sha256.clone(),
            byte_len: request.byte_len,
            tags: normalize_list(request.tags.clone())?,
            provenance_refs: normalize_list(request.provenance_refs.clone())?,
            content_cx_id: content_readback.cx_id.clone(),
            content_fingerprint: content_readback.input_sha256.clone(),
            metadata_sha256: metadata.sha256,
            deduplicated,
            canonical_asset_id,
        });
        content_readbacks.push(content_readback);
    }
    records.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
    let semantic_index_id = format!("{pipeline_id}/asset-semantic-index");
    let semantic_index = build_engine_calyx_semantic_index(&semantic_index_id, &content_readbacks)?;
    let semantic_readback = store_engine_calyx_semantic_index(database, &semantic_index)?;
    let mut pipeline = EngineCalyxAssetPipelineReadback {
        schema: ENGINE_CALYX_ASSET_PIPELINE_SCHEMA.to_string(),
        pipeline_id,
        asset_count: records.len(),
        deduplicated_count: records.iter().filter(|record| record.deduplicated).count(),
        dedup_groups,
        records,
        semantic_index_id,
        semantic_index_fingerprint: semantic_readback.index.index_fingerprint,
        pipeline_fingerprint: String::new(),
    };
    pipeline.pipeline_fingerprint = pipeline_fingerprint(&pipeline)?;
    validate_pipeline(&pipeline)?;
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::AssetPipelineLedger,
        pipeline.pipeline_id.clone(),
        serde_json::to_value(&pipeline).map_err(codec_failed)?,
        json!({
            "source_issue": 1122,
            "asset_count": pipeline.asset_count,
            "deduplicated_count": pipeline.deduplicated_count,
            "pipeline_fingerprint": pipeline.pipeline_fingerprint
        }),
    ))?;
    parse_stored_pipeline(stored)
}

pub fn read_calyx_asset_pipeline(
    database: &EngineCalyxDatabase,
    pipeline_id: &str,
) -> Result<EngineCalyxAssetPipelineStoredReadback, EngineCalyxAssetPipelineError> {
    if pipeline_id.trim().is_empty() {
        return invalid("pipeline_id", "pipeline id must be non-empty");
    }
    parse_stored_pipeline(
        database.get_record(EngineCalyxRecordKind::AssetPipelineLedger, pipeline_id)?,
    )
}

pub fn validate_pipeline(
    pipeline: &EngineCalyxAssetPipelineReadback,
) -> Result<(), EngineCalyxAssetPipelineError> {
    if pipeline.schema != ENGINE_CALYX_ASSET_PIPELINE_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_ASSET_PIPELINE_SCHEMA}"),
        );
    }
    if pipeline.asset_count != pipeline.records.len()
        || pipeline.deduplicated_count
            != pipeline
                .records
                .iter()
                .filter(|record| record.deduplicated)
                .count()
    {
        return invalid("counts", "asset and dedup counts must match records");
    }
    if pipeline.semantic_index_fingerprint.trim().is_empty()
        || pipeline.pipeline_fingerprint != pipeline_fingerprint(pipeline)?
    {
        return invalid("fingerprint", "pipeline fingerprints must be valid");
    }
    Ok(())
}

fn validate_requests(
    pipeline_id: &str,
    requests: &[EngineAssetIngestRequest],
) -> Result<(), EngineCalyxAssetPipelineError> {
    if pipeline_id.trim().is_empty() {
        return invalid("pipeline_id", "pipeline id must be non-empty");
    }
    if requests.is_empty() {
        return invalid("requests", "asset ingest requires at least one asset");
    }
    let mut seen = BTreeSet::new();
    for request in requests {
        if request.asset_id.trim().is_empty()
            || request.display_name.trim().is_empty()
            || request.source_uri.trim().is_empty()
            || request.bytes_sha256.trim().is_empty()
            || request.byte_len == 0
        {
            return invalid(
                "asset",
                "asset id, display, source, hash, and size are required",
            );
        }
        if !seen.insert(request.asset_id.as_str()) {
            return invalid("asset_id", "asset ids must be unique");
        }
        normalize_list(request.tags.clone())?;
        normalize_list(request.provenance_refs.clone())?;
    }
    Ok(())
}

fn canonical_assets(requests: &[EngineAssetIngestRequest]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for request in requests {
        map.entry(request.bytes_sha256.clone())
            .or_insert_with(|| request.asset_id.clone());
    }
    map
}

fn dedup_groups(
    requests: &[EngineAssetIngestRequest],
    canonical_by_hash: &BTreeMap<String, String>,
) -> Vec<EngineCalyxAssetDedupGroup> {
    let mut by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for request in requests {
        by_hash
            .entry(request.bytes_sha256.clone())
            .or_default()
            .push(request.asset_id.clone());
    }
    by_hash
        .into_iter()
        .filter_map(|(bytes_sha256, mut ids)| {
            ids.sort();
            let canonical_asset_id = canonical_by_hash.get(&bytes_sha256)?.clone();
            let duplicate_asset_ids = ids
                .into_iter()
                .filter(|asset_id| asset_id != &canonical_asset_id)
                .collect::<Vec<_>>();
            (!duplicate_asset_ids.is_empty()).then_some(EngineCalyxAssetDedupGroup {
                bytes_sha256,
                canonical_asset_id,
                duplicate_asset_ids,
            })
        })
        .collect()
}

fn asset_payload(
    request: &EngineAssetIngestRequest,
    deduplicated: bool,
    canonical_asset_id: &str,
) -> serde_json::Value {
    json!({
        "asset_id": request.asset_id,
        "display_name": request.display_name,
        "media_type": request.media_type.as_str(),
        "source_uri": request.source_uri,
        "bytes_sha256": request.bytes_sha256,
        "byte_len": request.byte_len,
        "tags": request.tags,
        "provenance_refs": request.provenance_refs,
        "deduplicated": deduplicated,
        "canonical_asset_id": canonical_asset_id
    })
}

fn pipeline_fingerprint(
    pipeline: &EngineCalyxAssetPipelineReadback,
) -> Result<String, EngineCalyxAssetPipelineError> {
    serde_json::to_vec(&json!({
        "schema": pipeline.schema,
        "pipeline_id": pipeline.pipeline_id,
        "records": pipeline.records,
        "dedup_groups": pipeline.dedup_groups,
        "semantic_index_fingerprint": pipeline.semantic_index_fingerprint,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
}

fn parse_stored_pipeline(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineCalyxAssetPipelineStoredReadback, EngineCalyxAssetPipelineError> {
    let pipeline =
        serde_json::from_value::<EngineCalyxAssetPipelineReadback>(database.payload.clone())
            .map_err(codec_failed)?;
    validate_pipeline(&pipeline)?;
    Ok(EngineCalyxAssetPipelineStoredReadback {
        schema: ENGINE_CALYX_ASSET_PIPELINE_READBACK_SCHEMA.to_string(),
        database,
        pipeline,
    })
}

fn normalize_list(values: Vec<String>) -> Result<Vec<String>, EngineCalyxAssetPipelineError> {
    let normalized = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        return invalid("list", "tags/provenance refs must be non-empty");
    }
    Ok(normalized)
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxAssetPipelineError> {
    Err(EngineCalyxAssetPipelineError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxAssetPipelineError {
    EngineCalyxAssetPipelineError::Codec(error.to_string())
}
