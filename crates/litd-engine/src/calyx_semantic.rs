//! Engine-side deterministic keyword search, dedup, and provenance readbacks.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::calyx_semantic_support::{
    codec_failed, duplicate_groups, hex_bytes, index_fingerprint, per_perspective_weighted_terms,
    perspective_text, query_fingerprint, semantic_signature, token_counts,
};
use crate::{
    calyx_content::{
        ENGINE_CALYX_CONTENT_READBACK_SCHEMA, EngineContentConstellationReadback, EngineContentKind,
    },
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
};

pub const ENGINE_CALYX_SEMANTIC_INDEX_SCHEMA: &str = "litd.engine.calyx_keyword_index.v2";
pub const ENGINE_CALYX_SEMANTIC_QUERY_SCHEMA: &str = "litd.engine.calyx_keyword_query.v2";
pub const ENGINE_CALYX_SEMANTIC_READBACK_SCHEMA: &str =
    "litd.engine.calyx_keyword_index_readback.v2";
pub const ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT: usize = 50;
const PERSPECTIVE_VARIANTS_PER_FAMILY: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCalyxKeywordPerspectiveFamily {
    Identity,
    Taxonomy,
    Lexical,
    Structured,
    Provenance,
    Gameplay,
    Narrative,
    Asset,
    Relationship,
    Lowering,
}

impl EngineCalyxKeywordPerspectiveFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Taxonomy => "taxonomy",
            Self::Lexical => "lexical",
            Self::Structured => "structured",
            Self::Provenance => "provenance",
            Self::Gameplay => "gameplay",
            Self::Narrative => "narrative",
            Self::Asset => "asset",
            Self::Relationship => "relationship",
            Self::Lowering => "lowering",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxKeywordPerspective {
    pub slot: u16,
    pub name: String,
    pub family: EngineCalyxKeywordPerspectiveFamily,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxProvenanceRef {
    pub cx_id: String,
    pub source_path: String,
    pub input_sha256: String,
    pub database_seq: u64,
    pub database_sha256: String,
    pub ledger_seq: u64,
    pub ledger_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxSemanticRecord {
    pub key: String,
    pub cx_id: String,
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub source_fingerprint: String,
    pub semantic_signature: String,
    pub provenance_refs: Vec<EngineCalyxProvenanceRef>,
    pub perspective_terms: BTreeMap<String, BTreeMap<String, u32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxDuplicateGroup {
    pub semantic_signature: String,
    pub record_keys: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxSemanticIndex {
    pub schema: String,
    pub index_id: String,
    pub panel_template: String,
    pub perspective_count: usize,
    pub perspective_roster: Vec<EngineCalyxKeywordPerspective>,
    pub records: Vec<EngineCalyxSemanticRecord>,
    pub duplicate_groups: Vec<EngineCalyxDuplicateGroup>,
    pub index_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxSemanticHit {
    pub key: String,
    pub cx_id: String,
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub score: u32,
    pub matched_terms: Vec<String>,
    pub perspective_scores: BTreeMap<String, u32>,
    pub provenance_refs: Vec<EngineCalyxProvenanceRef>,
    pub source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxSemanticQueryReadback {
    pub schema: String,
    pub query: String,
    pub hit_count: usize,
    pub hits: Vec<EngineCalyxSemanticHit>,
    pub perspective_roster: Vec<EngineCalyxKeywordPerspective>,
    pub per_perspective_weighted_terms: BTreeMap<String, u32>,
    pub fused_result_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxSemanticIndexReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub index: EngineCalyxSemanticIndex,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxSemanticError {
    #[error("engine Calyx semantic field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx semantic persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx semantic codec failed: {0}")]
    Codec(String),
}

pub fn engine_calyx_keyword_perspective_roster() -> Vec<EngineCalyxKeywordPerspective> {
    let families = [
        (EngineCalyxKeywordPerspectiveFamily::Identity, 11),
        (EngineCalyxKeywordPerspectiveFamily::Taxonomy, 13),
        (EngineCalyxKeywordPerspectiveFamily::Lexical, 17),
        (EngineCalyxKeywordPerspectiveFamily::Structured, 19),
        (EngineCalyxKeywordPerspectiveFamily::Provenance, 7),
        (EngineCalyxKeywordPerspectiveFamily::Gameplay, 23),
        (EngineCalyxKeywordPerspectiveFamily::Narrative, 21),
        (EngineCalyxKeywordPerspectiveFamily::Asset, 15),
        (EngineCalyxKeywordPerspectiveFamily::Relationship, 9),
        (EngineCalyxKeywordPerspectiveFamily::Lowering, 25),
    ];
    families
        .iter()
        .enumerate()
        .flat_map(|(family_index, (family, base_weight))| {
            (0..PERSPECTIVE_VARIANTS_PER_FAMILY).map(move |variant| {
                let slot = (family_index * PERSPECTIVE_VARIANTS_PER_FAMILY + variant) as u16;
                EngineCalyxKeywordPerspective {
                    slot,
                    name: format!("{}-{}", family.as_str(), variant + 1),
                    family: *family,
                    weight: base_weight + variant as u32,
                }
            })
        })
        .collect()
}

pub fn build_engine_calyx_semantic_index(
    index_id: impl Into<String>,
    readbacks: &[EngineContentConstellationReadback],
) -> Result<EngineCalyxSemanticIndex, EngineCalyxSemanticError> {
    let index_id = index_id.into();
    if index_id.trim().is_empty() {
        return invalid("index_id", "index id must be non-empty");
    }
    if readbacks.is_empty() {
        return invalid(
            "readbacks",
            "semantic index requires at least one content readback",
        );
    }
    let perspective_roster = engine_calyx_keyword_perspective_roster();
    validate_perspective_roster(&perspective_roster)?;
    let mut records = readbacks
        .iter()
        .map(|readback| semantic_record(readback, &perspective_roster))
        .collect::<Result<Vec<_>, _>>()?;
    records.sort_by(|left, right| left.key.cmp(&right.key));
    let duplicate_groups = duplicate_groups(&records);
    let mut index = EngineCalyxSemanticIndex {
        schema: ENGINE_CALYX_SEMANTIC_INDEX_SCHEMA.to_string(),
        index_id,
        panel_template: "deterministic-keyword-perspective-50-engine-editor".to_string(),
        perspective_count: perspective_roster.len(),
        perspective_roster,
        records,
        duplicate_groups,
        index_fingerprint: String::new(),
    };
    index.index_fingerprint = index_fingerprint(&index)?;
    validate_semantic_index(&index)?;
    Ok(index)
}

pub fn query_engine_calyx_semantic_index(
    index: &EngineCalyxSemanticIndex,
    query: &str,
    limit: usize,
) -> Result<EngineCalyxSemanticQueryReadback, EngineCalyxSemanticError> {
    validate_semantic_index(index)?;
    if limit == 0 {
        return invalid("limit", "search limit must be > 0");
    }
    let query_terms = token_counts(query);
    if query_terms.is_empty() {
        return invalid("query", "query must contain searchable terms");
    }
    let mut hits = index
        .records
        .iter()
        .filter_map(|record| score_record(record, &index.perspective_roster, &query_terms))
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.key.cmp(&right.key))
    });
    hits.truncate(limit);
    let fused_result_fingerprint = query_fingerprint(query, &hits)?;
    Ok(EngineCalyxSemanticQueryReadback {
        schema: ENGINE_CALYX_SEMANTIC_QUERY_SCHEMA.to_string(),
        query: query.trim().to_string(),
        hit_count: hits.len(),
        hits,
        perspective_roster: index.perspective_roster.clone(),
        per_perspective_weighted_terms: per_perspective_weighted_terms(index),
        fused_result_fingerprint,
    })
}

pub fn store_engine_calyx_semantic_index(
    database: &EngineCalyxDatabase,
    index: &EngineCalyxSemanticIndex,
) -> Result<EngineCalyxSemanticIndexReadback, EngineCalyxSemanticError> {
    validate_semantic_index(index)?;
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::SemanticIndex,
        index.index_id.clone(),
        serde_json::to_value(index).map_err(codec_failed)?,
        json!({
            "source_issue": 1119,
            "panel_template": index.panel_template,
            "perspective_count": index.perspective_count,
            "index_fingerprint": index.index_fingerprint
        }),
    );
    parse_index_readback(database.put_record(&record)?)
}

pub fn read_engine_calyx_semantic_index(
    database: &EngineCalyxDatabase,
    index_id: &str,
) -> Result<EngineCalyxSemanticIndexReadback, EngineCalyxSemanticError> {
    if index_id.trim().is_empty() {
        return invalid("index_id", "index id must be non-empty");
    }
    parse_index_readback(database.get_record(EngineCalyxRecordKind::SemanticIndex, index_id)?)
}

pub fn validate_semantic_index(
    index: &EngineCalyxSemanticIndex,
) -> Result<(), EngineCalyxSemanticError> {
    if index.schema != ENGINE_CALYX_SEMANTIC_INDEX_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_SEMANTIC_INDEX_SCHEMA}"),
        );
    }
    validate_perspective_roster(&index.perspective_roster)?;
    if index.perspective_count != ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        || index.perspective_count != index.perspective_roster.len()
    {
        return invalid(
            "perspective_count",
            "keyword index must expose exactly 50 deterministic perspectives",
        );
    }
    if index.records.is_empty() {
        return invalid("records", "semantic index must contain records");
    }
    if index.index_fingerprint != index_fingerprint(index)? {
        return invalid("index_fingerprint", "semantic index fingerprint mismatch");
    }
    Ok(())
}

fn semantic_record(
    readback: &EngineContentConstellationReadback,
    perspectives: &[EngineCalyxKeywordPerspective],
) -> Result<EngineCalyxSemanticRecord, EngineCalyxSemanticError> {
    validate_content_readback(readback)?;
    let mut perspective_terms = BTreeMap::new();
    for perspective in perspectives {
        perspective_terms.insert(
            perspective.name.clone(),
            token_counts(&perspective_text(readback, perspective)),
        );
    }
    Ok(EngineCalyxSemanticRecord {
        key: format!("{}/{}", readback.kind.as_str(), readback.id),
        cx_id: readback.cx_id.clone(),
        kind: readback.kind,
        id: readback.id.clone(),
        display_name: readback.display_name.clone(),
        source_fingerprint: readback.input_sha256.clone(),
        semantic_signature: semantic_signature(readback),
        provenance_refs: vec![provenance_ref(readback)],
        perspective_terms,
    })
}

fn validate_content_readback(
    readback: &EngineContentConstellationReadback,
) -> Result<(), EngineCalyxSemanticError> {
    if readback.schema != ENGINE_CALYX_CONTENT_READBACK_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {ENGINE_CALYX_CONTENT_READBACK_SCHEMA}"),
        );
    }
    if readback.lowering.source_cx_id != readback.cx_id {
        return invalid("readback.lowering.source_cx_id", "must match cx_id");
    }
    if readback.input_sha256.trim().is_empty() || readback.database.sha256.trim().is_empty() {
        return invalid("readback.provenance", "fingerprints must be non-empty");
    }
    if readback.source_path.trim().is_empty() {
        return invalid("readback.source_path", "source path must be non-empty");
    }
    Ok(())
}

fn validate_perspective_roster(
    perspectives: &[EngineCalyxKeywordPerspective],
) -> Result<(), EngineCalyxSemanticError> {
    if perspectives.len() != ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT {
        return invalid(
            "perspective_roster",
            "keyword panel must contain exactly 50 deterministic perspectives",
        );
    }
    let mut names = BTreeSet::new();
    let mut families = BTreeSet::new();
    for (expected_slot, perspective) in perspectives.iter().enumerate() {
        if perspective.slot as usize != expected_slot
            || perspective.name.trim().is_empty()
            || perspective.weight == 0
        {
            return invalid(
                "perspective_roster",
                "perspective slots, names, and weights must be valid",
            );
        }
        if !names.insert(perspective.name.as_str()) {
            return invalid("perspective_roster", "perspective names must be unique");
        }
        families.insert(perspective.family);
    }
    if families.len() < 10 {
        return invalid(
            "perspective_roster",
            "keyword panel must span at least 10 deterministic families",
        );
    }
    Ok(())
}

fn score_record(
    record: &EngineCalyxSemanticRecord,
    perspectives: &[EngineCalyxKeywordPerspective],
    query_terms: &BTreeMap<String, u32>,
) -> Option<EngineCalyxSemanticHit> {
    let mut score = 0_u32;
    let mut perspective_scores = BTreeMap::new();
    let mut matched_terms = BTreeSet::new();
    for perspective in perspectives {
        let terms = record.perspective_terms.get(&perspective.name)?;
        let matches = query_terms
            .keys()
            .filter(|term| terms.contains_key(*term))
            .cloned()
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        let perspective_score = matches.len() as u32 * perspective.weight;
        score = score.saturating_add(perspective_score);
        perspective_scores.insert(perspective.name.clone(), perspective_score);
        matched_terms.extend(matches);
    }
    (score > 0).then(|| EngineCalyxSemanticHit {
        key: record.key.clone(),
        cx_id: record.cx_id.clone(),
        kind: record.kind,
        id: record.id.clone(),
        display_name: record.display_name.clone(),
        score,
        matched_terms: matched_terms.into_iter().collect(),
        perspective_scores,
        provenance_refs: record.provenance_refs.clone(),
        source_fingerprint: record.source_fingerprint.clone(),
    })
}

fn provenance_ref(readback: &EngineContentConstellationReadback) -> EngineCalyxProvenanceRef {
    EngineCalyxProvenanceRef {
        cx_id: readback.cx_id.clone(),
        source_path: readback.source_path.clone(),
        input_sha256: readback.input_sha256.clone(),
        database_seq: readback.database.seq,
        database_sha256: readback.database.sha256.clone(),
        ledger_seq: readback.constellation.provenance.seq,
        ledger_hash: hex_bytes(&readback.constellation.provenance.hash),
    }
}

fn parse_index_readback(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineCalyxSemanticIndexReadback, EngineCalyxSemanticError> {
    let index = serde_json::from_value::<EngineCalyxSemanticIndex>(database.payload.clone())
        .map_err(codec_failed)?;
    validate_semantic_index(&index)?;
    Ok(EngineCalyxSemanticIndexReadback {
        schema: ENGINE_CALYX_SEMANTIC_READBACK_SCHEMA.to_string(),
        database,
        index,
    })
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxSemanticError> {
    Err(EngineCalyxSemanticError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}
