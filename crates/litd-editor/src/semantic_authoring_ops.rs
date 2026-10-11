use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::world_memory::{WorldMemoryIndex, WorldMemoryLensKind, WorldMemorySourceKind};

#[path = "semantic_authoring_ops_engine.rs"]
mod semantic_authoring_ops_engine;
#[path = "semantic_authoring_ops_store.rs"]
mod semantic_authoring_ops_store;

pub use semantic_authoring_ops_store::{
    SemanticAuthoringOpsWriteback, parse_semantic_authoring_ops_ledger,
    read_semantic_authoring_ops_ledger, write_semantic_authoring_ops_ledger,
};

use semantic_authoring_ops_engine::{
    invalid_semantic_authoring, run_semantic_authoring_request, validate_report,
};

pub const SEMANTIC_AUTHORING_OPS_SCHEMA: &str = "litd.semantic_authoring_ops.ledger.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAuthoringOperation {
    Define,
    Agree,
    Disagree,
    Neighbors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAuthoringStatus {
    Returned,
    NoNeighbors,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticAuthoringRequest {
    pub query_id: String,
    pub operation: SemanticAuthoringOperation,
    pub subject_constellation_id: String,
    pub lens: Option<WorldMemoryLensKind>,
    pub slot_filter: Vec<WorldMemoryLensKind>,
    pub k: usize,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticAuthoringSubject {
    pub constellation_id: String,
    pub calyx_cx_id: String,
    pub world_id: String,
    pub source_kind: WorldMemorySourceKind,
    pub source_id: String,
    pub title: String,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringHit {
    pub rank: usize,
    pub constellation_id: String,
    pub calyx_cx_id: String,
    pub world_id: String,
    pub source_kind: WorldMemorySourceKind,
    pub source_id: String,
    pub title: String,
    pub score: f32,
    pub mean_cosine: Option<f32>,
    pub min_cosine: Option<f32>,
    pub max_cosine: Option<f32>,
    pub spread: Option<f32>,
    pub per_slot: Vec<SemanticAuthoringSlotEvidence>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringSlotEvidence {
    pub lens: WorldMemoryLensKind,
    pub slot_id: u16,
    pub cosine: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringDefinitionSlot {
    pub lens: WorldMemoryLensKind,
    pub slot_id: u16,
    pub dimension: usize,
    pub centroid: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringDefinition {
    pub subject_constellation_id: String,
    pub calyx_cx_id: String,
    pub slots: Vec<SemanticAuthoringDefinitionSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringReport {
    pub schema: String,
    pub query_id: String,
    pub source_of_truth: String,
    pub operation: SemanticAuthoringOperation,
    pub status: SemanticAuthoringStatus,
    pub code: String,
    pub reasons: Vec<String>,
    pub requested_by: String,
    pub k: usize,
    pub lens: Option<WorldMemoryLensKind>,
    pub slot_filter: Vec<WorldMemoryLensKind>,
    pub subject: SemanticAuthoringSubject,
    pub hits: Vec<SemanticAuthoringHit>,
    pub definition: Option<SemanticAuthoringDefinition>,
    pub skipped_insufficient_overlap: Vec<SemanticAuthoringSubject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticAuthoringOpsLedger {
    pub schema: String,
    pub ledger_id: String,
    pub source_index_id: String,
    pub report_count: usize,
    pub reports: Vec<SemanticAuthoringReport>,
}

pub fn run_semantic_authoring_action(
    source_of_truth: impl Into<String>,
    index: &WorldMemoryIndex,
    request: &SemanticAuthoringRequest,
) -> Result<SemanticAuthoringReport, EngineError> {
    validate_request(request)?;
    let source_of_truth = source_of_truth.into();
    if source_of_truth.trim().is_empty() {
        return invalid_semantic_authoring("source_of_truth", "source of truth must be non-empty");
    }
    let report = run_semantic_authoring_request(source_of_truth, index, request)?;
    validate_report(&report)?;
    Ok(report)
}

pub fn build_semantic_authoring_ops_ledger(
    ledger_id: impl Into<String>,
    source_index_id: impl Into<String>,
    reports: Vec<SemanticAuthoringReport>,
) -> Result<SemanticAuthoringOpsLedger, EngineError> {
    if reports.is_empty() {
        return invalid_semantic_authoring("ledger.reports", "at least one report is required");
    }
    let ledger = SemanticAuthoringOpsLedger {
        schema: SEMANTIC_AUTHORING_OPS_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        source_index_id: source_index_id.into(),
        report_count: reports.len(),
        reports,
    };
    validate_semantic_authoring_ops_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_semantic_authoring_ops_ledger(
    ledger: &SemanticAuthoringOpsLedger,
) -> Result<(), EngineError> {
    if ledger.schema != SEMANTIC_AUTHORING_OPS_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.source_index_id.trim().is_empty()
        || ledger.report_count == 0
        || ledger.report_count != ledger.reports.len()
    {
        return invalid_semantic_authoring("ledger", "schema, id, and report count must be valid");
    }
    let mut seen = BTreeSet::new();
    for report in &ledger.reports {
        validate_report(report)?;
        if !seen.insert(report.query_id.clone()) {
            return invalid_semantic_authoring("ledger.duplicate", "duplicate report query id");
        }
    }
    Ok(())
}

fn validate_request(request: &SemanticAuthoringRequest) -> Result<(), EngineError> {
    if request.query_id.trim().is_empty()
        || request.query_id.contains(['\n', '\r', '\t'])
        || request.subject_constellation_id.trim().is_empty()
        || request
            .subject_constellation_id
            .contains(['\n', '\r', '\t'])
        || request.requested_by.trim().is_empty()
        || request.requested_by.contains(['\n', '\r', '\t'])
        || request.k == 0
        || request.k > 64
    {
        return invalid_semantic_authoring(
            "request",
            "query id, subject, requester, and k must be valid",
        );
    }
    match request.operation {
        SemanticAuthoringOperation::Define | SemanticAuthoringOperation::Neighbors => {
            if request.lens.is_none() || !request.slot_filter.is_empty() {
                return invalid_semantic_authoring(
                    "request.contradictory",
                    "define/neighbors require exactly one lens and no slot filter",
                );
            }
        }
        SemanticAuthoringOperation::Agree | SemanticAuthoringOperation::Disagree => {
            if request.lens.is_some() || request.slot_filter.len() == 1 {
                return invalid_semantic_authoring(
                    "request.contradictory",
                    "agree/disagree use zero or at least two slot-filter lenses and no single lens",
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "semantic_authoring_ops_tests.rs"]
mod tests;
