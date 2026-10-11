use std::collections::BTreeSet;

use calyx_loom::AbundanceReport;
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::world_memory::{WorldMemoryIndex, WorldMemoryLensKind, WorldMemorySourceKind};

#[path = "world_content_coverage_engine.rs"]
mod world_content_coverage_engine;
#[path = "world_content_coverage_store.rs"]
mod world_content_coverage_store;

pub use world_content_coverage_store::{
    WorldContentCoverageWriteback, parse_world_content_coverage_ledger,
    read_world_content_coverage_ledger, write_world_content_coverage_ledger,
};

use world_content_coverage_engine::{
    invalid_world_content_coverage, run_world_content_coverage_analysis,
    validate_world_content_coverage_report,
};

pub const WORLD_CONTENT_COVERAGE_SCHEMA: &str = "litd.world_content_coverage.ledger.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldContentCoverageSpec {
    pub analysis_id: String,
    pub source_index_id: String,
    pub requested_by: String,
    pub buckets: Vec<WorldCoverageBucketSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCoverageBucketSpec {
    pub bucket_id: String,
    pub label: String,
    pub lens: WorldMemoryLensKind,
    pub source_kinds: Vec<WorldMemorySourceKind>,
    pub required_terms: Vec<String>,
    pub min_count: usize,
    pub max_count: Option<usize>,
    pub min_share_milli: u16,
    pub max_share_milli: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCoverageStatus {
    Balanced,
    Findings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCoverageBucketStatus {
    Balanced,
    BlindSpot,
    Underrepresented,
    Overrepresented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCoverageFindingKind {
    BlindSpot,
    Underrepresented,
    Overrepresented,
    LensMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCoverageSeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCoverageBucketReport {
    pub bucket_id: String,
    pub label: String,
    pub lens: WorldMemoryLensKind,
    pub eligible_count: usize,
    pub match_count: usize,
    pub share_milli: u16,
    pub status: WorldCoverageBucketStatus,
    pub matched_constellation_ids: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCoverageFinding {
    pub kind: WorldCoverageFindingKind,
    pub severity: WorldCoverageSeverity,
    pub bucket_id: String,
    pub reason: String,
    pub actual_count: usize,
    pub expected_min_count: Option<usize>,
    pub expected_max_count: Option<usize>,
    pub actual_share_milli: u16,
    pub threshold_milli: Option<u16>,
    pub evidence_constellation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldCoverageLensBlindSpot {
    pub bucket_id: String,
    pub constellation_id: String,
    pub calyx_cx_id: String,
    pub text_slot_id: u16,
    pub target_slot_id: u16,
    pub delta: f32,
    pub severity: WorldCoverageSeverity,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldCoverageAgreementEdge {
    pub a_slot_id: u16,
    pub b_slot_id: u16,
    pub raw_mean_agreement: f32,
    pub mean_agreement: f32,
    pub agreement_weight: f32,
    pub n: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldContentCoverageReport {
    pub schema: String,
    pub analysis_id: String,
    pub source_index_id: String,
    pub source_of_truth: String,
    pub requested_by: String,
    pub status: WorldCoverageStatus,
    pub abundance: AbundanceReport,
    pub measured_count: usize,
    pub derived_count: usize,
    pub materialized_count: usize,
    pub bucket_reports: Vec<WorldCoverageBucketReport>,
    pub findings: Vec<WorldCoverageFinding>,
    pub lens_blind_spots: Vec<WorldCoverageLensBlindSpot>,
    pub agreement_edges: Vec<WorldCoverageAgreementEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldContentCoverageLedger {
    pub schema: String,
    pub ledger_id: String,
    pub report_count: usize,
    pub reports: Vec<WorldContentCoverageReport>,
}

pub fn analyze_world_content_coverage(
    source_of_truth: impl Into<String>,
    index: &WorldMemoryIndex,
    spec: &WorldContentCoverageSpec,
) -> Result<WorldContentCoverageReport, EngineError> {
    validate_spec(spec)?;
    let source_of_truth = source_of_truth.into();
    if source_of_truth.trim().is_empty() {
        return invalid_world_content_coverage(
            "source_of_truth",
            "source of truth must be non-empty",
        );
    }
    let report = run_world_content_coverage_analysis(source_of_truth, index, spec)?;
    validate_world_content_coverage_report(&report)?;
    Ok(report)
}

pub fn build_world_content_coverage_ledger(
    ledger_id: impl Into<String>,
    reports: Vec<WorldContentCoverageReport>,
) -> Result<WorldContentCoverageLedger, EngineError> {
    if reports.is_empty() {
        return invalid_world_content_coverage("ledger.reports", "at least one report is required");
    }
    let ledger = WorldContentCoverageLedger {
        schema: WORLD_CONTENT_COVERAGE_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        report_count: reports.len(),
        reports,
    };
    validate_world_content_coverage_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_world_content_coverage_ledger(
    ledger: &WorldContentCoverageLedger,
) -> Result<(), EngineError> {
    if ledger.schema != WORLD_CONTENT_COVERAGE_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.report_count == 0
        || ledger.report_count != ledger.reports.len()
    {
        return invalid_world_content_coverage("ledger", "schema, id, and count must be valid");
    }
    let mut seen = BTreeSet::new();
    for report in &ledger.reports {
        validate_world_content_coverage_report(report)?;
        if !seen.insert(report.analysis_id.clone()) {
            return invalid_world_content_coverage("ledger.duplicate", "duplicate analysis id");
        }
    }
    Ok(())
}

fn validate_spec(spec: &WorldContentCoverageSpec) -> Result<(), EngineError> {
    if invalid_id(&spec.analysis_id)
        || invalid_id(&spec.source_index_id)
        || invalid_id(&spec.requested_by)
        || spec.buckets.is_empty()
    {
        return invalid_world_content_coverage(
            "spec",
            "analysis id, source index id, requester, and buckets are required",
        );
    }
    let mut seen = BTreeSet::new();
    for bucket in &spec.buckets {
        if invalid_id(&bucket.bucket_id)
            || bucket.label.trim().is_empty()
            || bucket.required_terms.is_empty()
            || bucket.required_terms.iter().any(|term| invalid_id(term))
            || !seen.insert(bucket.bucket_id.clone())
            || bucket.min_share_milli > 1000
            || bucket.max_share_milli.is_some_and(|value| value > 1000)
            || bucket
                .max_count
                .is_some_and(|max_count| max_count < bucket.min_count)
            || bucket
                .max_share_milli
                .is_some_and(|max_share| max_share < bucket.min_share_milli)
        {
            return invalid_world_content_coverage(
                "spec.bucket",
                "bucket ids, terms, counts, shares, and thresholds must be valid",
            );
        }
    }
    Ok(())
}

fn invalid_id(value: &str) -> bool {
    value.trim().is_empty() || value.contains(['\n', '\r', '\t'])
}

#[cfg(test)]
#[path = "world_content_coverage_tests.rs"]
mod tests;
