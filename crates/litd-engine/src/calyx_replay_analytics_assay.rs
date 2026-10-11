//! #907: measured balance over ingested replays — Assay which unit/ability
//! stats actually predict win/loss, in BITS, anchored on the executed match
//! outcome (a TestPass/Reward-style anchor), not intuition.
//!
//! The samples come from the source of truth: the persisted replay match rows
//! are read back from the Calyx-backed database before measuring, so the
//! assay can never disagree with what was stored.

use serde::{Deserialize, Serialize};
use serde_json::json;

use calyx_assay::logistic_probe_mi_multiseed;

use crate::calyx_persistence::{
    EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
};
use crate::calyx_replay_analytics::{
    EngineReplayAnalyticsError, ReplayBatchReadback, codec_failed, expect_row_id, expect_schema,
    read_replay_match_batch, validate_id,
};

pub const REPLAY_BALANCE_ASSAY_SCHEMA: &str = "litd.engine.replay_analytics.balance_assay.v1";
pub const REPLAY_BALANCE_ANCHOR: &str = "match_outcome_win_loss";

/// Per-stat mutual-information report against the win/loss anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayBalanceStatReport {
    pub stat_key: String,
    pub bits: f32,
    pub ci_low: f32,
    pub ci_high: f32,
    pub probe_accuracy: f32,
    pub n_samples: usize,
    pub estimator: String,
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayBalanceAssayPayload {
    pub schema: String,
    pub batch_id: String,
    pub anchor: String,
    pub sample_count: usize,
    pub win_count: usize,
    pub loss_count: usize,
    /// Stat reports ranked by measured bits, descending — the measured
    /// balance verdict: which stats actually predict the outcome.
    pub ranked: Vec<ReplayBalanceStatReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayBalanceAssayReadback {
    pub database: EngineCalyxDatabaseReadback,
    pub payload: ReplayBalanceAssayPayload,
}

pub fn replay_balance_assay_row_id(batch_id: &str) -> String {
    format!("replay_balance_assay/{batch_id}")
}

/// Measure, store, and read back the per-stat balance assay for an ingested
/// replay batch. Fails closed on single-class labels or insufficient samples
/// (Calyx enforces `MIN_ASSAY_SAMPLES`).
pub fn measure_replay_balance_assay(
    database: &EngineCalyxDatabase,
    batch_id: &str,
) -> Result<ReplayBalanceAssayReadback, EngineReplayAnalyticsError> {
    validate_id("batch_id", batch_id)?;
    let batch = read_replay_match_batch(database, batch_id)?;
    let payload = measure_batch(&batch)?;
    let row_id = replay_balance_assay_row_id(batch_id);
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::Provenance,
        row_id.clone(),
        serde_json::to_value(&payload).map_err(codec_failed)?,
        json!({
            "source_issue": 907,
            "batch_id": batch_id,
            "anchor": REPLAY_BALANCE_ANCHOR,
            "sample_count": payload.sample_count,
            "stat_count": payload.ranked.len(),
            "top_stat": payload.ranked.first().map(|report| report.stat_key.clone()),
            "top_bits": payload.ranked.first().map(|report| report.bits)
        }),
    ))?;
    parse_assay_readback(stored, &row_id)
}

/// Read a previously stored balance assay row back from the source of truth.
pub fn read_replay_balance_assay(
    database: &EngineCalyxDatabase,
    batch_id: &str,
) -> Result<ReplayBalanceAssayReadback, EngineReplayAnalyticsError> {
    validate_id("batch_id", batch_id)?;
    let row_id = replay_balance_assay_row_id(batch_id);
    let stored = database.get_record(EngineCalyxRecordKind::Provenance, &row_id)?;
    parse_assay_readback(stored, &row_id)
}

fn measure_batch(
    batch: &ReplayBatchReadback,
) -> Result<ReplayBalanceAssayPayload, EngineReplayAnalyticsError> {
    let labels: Vec<bool> = batch
        .matches
        .iter()
        .map(|readback| readback.payload.outcome.is_win())
        .collect();
    let win_count = labels.iter().filter(|label| **label).count();
    let loss_count = labels.len() - win_count;
    if win_count == 0 || loss_count == 0 {
        return Err(EngineReplayAnalyticsError::Assay(format!(
            "balance assay requires both outcome classes: win_count={win_count} loss_count={loss_count}"
        )));
    }
    let mut ranked = Vec::with_capacity(batch.payload.stat_keys.len());
    for stat_key in &batch.payload.stat_keys {
        let mut samples = Vec::with_capacity(batch.matches.len());
        for readback in &batch.matches {
            let value = readback.payload.stats_milli.get(stat_key).ok_or_else(|| {
                EngineReplayAnalyticsError::Assay(format!(
                    "stat {stat_key} missing from stored match {}",
                    readback.payload.match_id
                ))
            })?;
            samples.push(vec![*value as f32 / 1000.0]);
        }
        let report = logistic_probe_mi_multiseed(&samples, &labels, None).map_err(|error| {
            EngineReplayAnalyticsError::Assay(format!(
                "stat {stat_key} probe failed: {}: {}",
                error.code, error.message
            ))
        })?;
        ranked.push(ReplayBalanceStatReport {
            stat_key: stat_key.clone(),
            bits: report.estimate.bits,
            ci_low: report.estimate.ci_low,
            ci_high: report.estimate.ci_high,
            probe_accuracy: report.accuracy,
            n_samples: report.estimate.n_samples,
            estimator: format!("{:?}", report.estimate.estimator),
            trust: format!("{:?}", report.estimate.trust),
        });
    }
    ranked.sort_by(|left, right| {
        right
            .bits
            .partial_cmp(&left.bits)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.stat_key.cmp(&right.stat_key))
    });
    Ok(ReplayBalanceAssayPayload {
        schema: REPLAY_BALANCE_ASSAY_SCHEMA.to_string(),
        batch_id: batch.payload.batch_id.clone(),
        anchor: REPLAY_BALANCE_ANCHOR.to_string(),
        sample_count: labels.len(),
        win_count,
        loss_count,
        ranked,
    })
}

fn parse_assay_readback(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<ReplayBalanceAssayReadback, EngineReplayAnalyticsError> {
    expect_row_id(&database, expected_row_id)?;
    let payload: ReplayBalanceAssayPayload =
        serde_json::from_value(database.payload.clone()).map_err(codec_failed)?;
    expect_schema(&payload.schema, REPLAY_BALANCE_ASSAY_SCHEMA)?;
    if payload.win_count + payload.loss_count != payload.sample_count || payload.ranked.is_empty() {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "assay.counts".to_string(),
            expected: "win+loss==samples and at least one ranked stat".to_string(),
            actual: format!(
                "win={} loss={} samples={} ranked={}",
                payload.win_count,
                payload.loss_count,
                payload.sample_count,
                payload.ranked.len()
            ),
        });
    }
    Ok(ReplayBalanceAssayReadback { database, payload })
}
