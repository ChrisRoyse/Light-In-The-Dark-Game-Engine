use std::collections::BTreeSet;

use calyx_core::{
    BoostConfig, DecayFunction, FusionWeights, PeriodicOptions, SequenceOptions, TemporalPolicy,
};
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[path = "temporal_content_store.rs"]
mod temporal_content_store;
#[path = "temporal_content_support.rs"]
mod temporal_content_support;

pub use temporal_content_store::{
    TemporalContentWriteback, parse_temporal_content_ledger, read_temporal_content_ledger,
    write_temporal_content_ledger,
};
use temporal_content_support::{
    invalid_temporal_content, score_item, validate_item, validate_query, validate_report,
};

pub const TEMPORAL_CONTENT_SCHEMA: &str = "litd.temporal_content.ledger.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalContentItem {
    pub item_id: String,
    pub title: String,
    pub base_score_milli: u16,
    pub event_time_secs: Option<i64>,
    pub sequence_position: u64,
    pub sequence_total: u64,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalContentQuery {
    pub query_id: String,
    pub query_time_secs: i64,
    pub tz_offset_secs: i32,
    pub policy: TemporalPolicy,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalLensEvidence {
    pub e2_recency: f32,
    pub e3_periodic_hour: f32,
    pub e3_periodic_day: f32,
    pub e3_periodic: f32,
    pub e4_sequence: f32,
    pub e4_positional: [f32; 4],
    pub fused_temporal: f32,
    pub post_retrieval_alpha: f32,
    pub temporal_multiplier: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalContentScore {
    pub item_id: String,
    pub title: String,
    pub base_score_milli: u16,
    pub final_score_milli: u32,
    pub event_time_secs: Option<i64>,
    pub sequence_position: u64,
    pub sequence_total: u64,
    pub temporal_active: bool,
    pub temporal_inactive_reason: Option<String>,
    pub lens: TemporalLensEvidence,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalContentReport {
    pub schema: String,
    pub query_id: String,
    pub source_of_truth: String,
    pub query_time_secs: i64,
    pub tz_offset_secs: i32,
    pub requested_by: String,
    pub policy: TemporalPolicy,
    pub item_count: usize,
    pub scores: Vec<TemporalContentScore>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalContentLedger {
    pub schema: String,
    pub ledger_id: String,
    pub report_count: usize,
    pub reports: Vec<TemporalContentReport>,
}

pub fn score_temporal_content(
    source_of_truth: impl Into<String>,
    items: &[TemporalContentItem],
    query: &TemporalContentQuery,
) -> Result<TemporalContentReport, EngineError> {
    validate_query(query)?;
    if items.is_empty() {
        return invalid_temporal_content("items", "temporal content item list must not be empty");
    }
    let source_of_truth = source_of_truth.into();
    if source_of_truth.trim().is_empty() {
        return invalid_temporal_content("source_of_truth", "source of truth must be non-empty");
    }
    let mut scores = Vec::with_capacity(items.len());
    for item in items {
        validate_item(item)?;
        scores.push(score_item(item, query)?);
    }
    scores.sort_by(|left, right| {
        right
            .final_score_milli
            .cmp(&left.final_score_milli)
            .then_with(|| left.item_id.cmp(&right.item_id))
    });
    let report = TemporalContentReport {
        schema: TEMPORAL_CONTENT_SCHEMA.to_string(),
        query_id: query.query_id.clone(),
        source_of_truth,
        query_time_secs: query.query_time_secs,
        tz_offset_secs: query.tz_offset_secs,
        requested_by: query.requested_by.clone(),
        policy: query.policy,
        item_count: items.len(),
        scores,
    };
    validate_report(&report)?;
    Ok(report)
}

pub fn build_temporal_content_ledger(
    ledger_id: impl Into<String>,
    reports: Vec<TemporalContentReport>,
) -> Result<TemporalContentLedger, EngineError> {
    if reports.is_empty() {
        return invalid_temporal_content("ledger.reports", "at least one report is required");
    }
    let ledger = TemporalContentLedger {
        schema: TEMPORAL_CONTENT_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        report_count: reports.len(),
        reports,
    };
    validate_temporal_content_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_temporal_content_ledger(ledger: &TemporalContentLedger) -> Result<(), EngineError> {
    if ledger.schema != TEMPORAL_CONTENT_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.report_count == 0
        || ledger.report_count != ledger.reports.len()
    {
        return invalid_temporal_content("ledger", "schema, id, and report count must be valid");
    }
    let mut seen = BTreeSet::new();
    for report in &ledger.reports {
        validate_report(report)?;
        if !seen.insert(report.query_id.clone()) {
            return invalid_temporal_content("ledger.duplicate", "duplicate report query id");
        }
    }
    Ok(())
}

pub fn default_temporal_authoring_policy(
    query_time_secs: i64,
) -> Result<TemporalPolicy, EngineError> {
    TemporalPolicy::new(
        true,
        DecayFunction::Exponential {
            half_life_secs: 3_600,
        },
        PeriodicOptions {
            target_hour: Some(14),
            target_day_of_week: Some(1),
            use_now: false,
        },
        SequenceOptions::default(),
        FusionWeights::default(),
        BoostConfig {
            post_retrieval_alpha: 0.10,
            causal_high_mult: 1.10,
            causal_low_mult: 0.85,
        },
        true,
    )
    .map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.temporal_content.policy".to_string(),
        reason: format!("{}: {}", error.code, error.message),
    })
    .map(|mut policy| {
        if policy.periodic.use_now {
            policy.periodic.use_now = false;
        }
        let _ = query_time_secs;
        policy
    })
}

#[cfg(test)]
#[path = "temporal_content_tests.rs"]
mod tests;
