//! Editor-facing Calyx asset search and similarity readbacks.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    calyx_asset_pipeline::{
        EngineAssetMediaType, EngineCalyxAssetPipelineError, EngineCalyxAssetPipelineReadback,
        EngineCalyxAssetRecord, read_calyx_asset_pipeline,
    },
    calyx_content::EngineContentKind,
    calyx_persistence::EngineCalyxDatabase,
    calyx_semantic::{
        EngineCalyxProvenanceRef, EngineCalyxSemanticHit, query_engine_calyx_semantic_index,
        read_engine_calyx_semantic_index,
    },
};

pub const ENGINE_CALYX_ASSET_INTELLIGENCE_SCHEMA: &str = "litd.engine.calyx_asset_intelligence.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAssetSearchHit {
    pub asset_id: String,
    pub display_name: String,
    pub media_type: EngineAssetMediaType,
    pub score: u32,
    pub matched_terms: Vec<String>,
    pub match_reason: String,
    pub source_uri: String,
    pub bytes_sha256: String,
    pub content_cx_id: String,
    pub canonical_asset_id: String,
    pub deduplicated: bool,
    pub provenance_refs: Vec<EngineCalyxProvenanceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAssetIntelligenceReadback {
    pub schema: String,
    pub pipeline_id: String,
    pub operation: String,
    pub query: String,
    pub source_asset_id: Option<String>,
    pub hit_count: usize,
    pub hits: Vec<EngineCalyxAssetSearchHit>,
    pub fused_result_fingerprint: String,
}

pub fn search_calyx_assets(
    database: &EngineCalyxDatabase,
    pipeline_id: &str,
    query: &str,
    limit: usize,
) -> Result<EngineCalyxAssetIntelligenceReadback, EngineCalyxAssetPipelineError> {
    if limit == 0 {
        return invalid("limit", "asset search limit must be > 0");
    }
    let pipeline = read_calyx_asset_pipeline(database, pipeline_id)?.pipeline;
    let semantic = read_engine_calyx_semantic_index(database, &pipeline.semantic_index_id)?.index;
    let semantic_query = query_engine_calyx_semantic_index(&semantic, query, limit)?;
    let hits = semantic_query
        .hits
        .iter()
        .map(|hit| asset_hit(&pipeline, hit, "semantic"))
        .collect::<Result<Vec<_>, _>>()?;
    asset_intelligence_readback(&pipeline.pipeline_id, "search", query.trim(), None, hits)
}

pub fn find_similar_calyx_assets(
    database: &EngineCalyxDatabase,
    pipeline_id: &str,
    asset_id: &str,
    limit: usize,
) -> Result<EngineCalyxAssetIntelligenceReadback, EngineCalyxAssetPipelineError> {
    if limit == 0 {
        return invalid("limit", "asset similarity limit must be > 0");
    }
    let pipeline = read_calyx_asset_pipeline(database, pipeline_id)?.pipeline;
    let source = pipeline
        .records
        .iter()
        .find(|record| record.asset_id == asset_id)
        .ok_or_else(|| EngineCalyxAssetPipelineError::InvalidInput {
            field: "asset_id".to_string(),
            reason: format!("asset {asset_id} is not present in pipeline {pipeline_id}"),
        })?;
    let semantic = read_engine_calyx_semantic_index(database, &pipeline.semantic_index_id)?.index;
    let query = similar_asset_query(source);
    let semantic_query =
        query_engine_calyx_semantic_index(&semantic, &query, pipeline.asset_count)?;
    let mut hits = semantic_query
        .hits
        .iter()
        .filter(|hit| hit.id != asset_id)
        .map(|hit| asset_hit(&pipeline, hit, similarity_reason(&pipeline, hit, source)))
        .collect::<Result<Vec<_>, _>>()?;
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.asset_id.cmp(&right.asset_id))
    });
    hits.truncate(limit);
    asset_intelligence_readback(
        &pipeline.pipeline_id,
        "find_similar",
        &query,
        Some(asset_id.to_string()),
        hits,
    )
}

fn similarity_reason<'a>(
    pipeline: &'a EngineCalyxAssetPipelineReadback,
    hit: &EngineCalyxSemanticHit,
    source: &EngineCalyxAssetRecord,
) -> &'a str {
    pipeline
        .records
        .iter()
        .find(|record| record.asset_id == hit.id && record.bytes_sha256 == source.bytes_sha256)
        .map(|_| "dedup+semantic")
        .unwrap_or("semantic")
}

fn asset_hit(
    pipeline: &EngineCalyxAssetPipelineReadback,
    hit: &EngineCalyxSemanticHit,
    match_reason: &str,
) -> Result<EngineCalyxAssetSearchHit, EngineCalyxAssetPipelineError> {
    if hit.kind != EngineContentKind::Asset {
        return invalid(
            "semantic_hit.kind",
            "asset pipeline search returned non-asset content",
        );
    }
    let record = pipeline
        .records
        .iter()
        .find(|record| record.asset_id == hit.id)
        .ok_or_else(|| EngineCalyxAssetPipelineError::InvalidInput {
            field: "semantic_hit.id".to_string(),
            reason: format!("semantic hit {} is missing from asset pipeline", hit.id),
        })?;
    Ok(EngineCalyxAssetSearchHit {
        asset_id: record.asset_id.clone(),
        display_name: record.display_name.clone(),
        media_type: record.media_type,
        score: hit.score,
        matched_terms: hit.matched_terms.clone(),
        match_reason: match_reason.to_string(),
        source_uri: record.source_uri.clone(),
        bytes_sha256: record.bytes_sha256.clone(),
        content_cx_id: record.content_cx_id.clone(),
        canonical_asset_id: record.canonical_asset_id.clone(),
        deduplicated: record.deduplicated,
        provenance_refs: hit.provenance_refs.clone(),
    })
}

fn similar_asset_query(record: &EngineCalyxAssetRecord) -> String {
    format!(
        "{} {} {} {}",
        record.display_name,
        record.media_type.as_str(),
        record.tags.join(" "),
        record.bytes_sha256
    )
}

fn asset_intelligence_readback(
    pipeline_id: &str,
    operation: &str,
    query: &str,
    source_asset_id: Option<String>,
    hits: Vec<EngineCalyxAssetSearchHit>,
) -> Result<EngineCalyxAssetIntelligenceReadback, EngineCalyxAssetPipelineError> {
    let fused_result_fingerprint =
        asset_intelligence_fingerprint(pipeline_id, operation, query, &source_asset_id, &hits)?;
    Ok(EngineCalyxAssetIntelligenceReadback {
        schema: ENGINE_CALYX_ASSET_INTELLIGENCE_SCHEMA.to_string(),
        pipeline_id: pipeline_id.to_string(),
        operation: operation.to_string(),
        query: query.to_string(),
        source_asset_id,
        hit_count: hits.len(),
        hits,
        fused_result_fingerprint,
    })
}

fn asset_intelligence_fingerprint(
    pipeline_id: &str,
    operation: &str,
    query: &str,
    source_asset_id: &Option<String>,
    hits: &[EngineCalyxAssetSearchHit],
) -> Result<String, EngineCalyxAssetPipelineError> {
    serde_json::to_vec(&json!({
        "schema": ENGINE_CALYX_ASSET_INTELLIGENCE_SCHEMA,
        "pipeline_id": pipeline_id,
        "operation": operation,
        "query": query,
        "source_asset_id": source_asset_id,
        "hits": hits,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
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
