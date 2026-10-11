//! #907: crash/anomaly recurrence forecasting (Oracle) over opt-in telemetry.
//!
//! Crash telemetry events for one crash signature are lowered into a real
//! Aster recurrence series via the engine Oracle service (`Base` +
//! `Recurrence` + `Online` + `Ledger` CFs), then the next occurrence is
//! forecast. The forecast summary is additionally persisted as a provenance
//! row in the Calyx-backed engine database and read back from it, so the
//! analytics loop has a single queryable surface.

use calyx_core::Clock;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    calyx_observability::{EngineTelemetryEvent, EngineTelemetryEventKind},
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_replay_analytics::{
        EngineReplayAnalyticsError, codec_failed, expect_row_id, expect_schema, validate_id,
    },
    live_world_forecast::{
        EngineOracleService, ForecastOutcome, ForecastRequest, LiveWorldMetricKind,
        LiveWorldObservation,
    },
};

pub const CRASH_RECURRENCE_SCHEMA: &str = "litd.engine.replay_analytics.crash_recurrence.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrashRecurrencePayload {
    pub schema: String,
    pub forecast_id: String,
    pub crash_signature: String,
    pub stream_id: String,
    pub ingested_event_count: usize,
    pub outcome: String,
    pub rejection_reason: Option<String>,
    pub support: usize,
    pub predicted_next_secs: Option<i64>,
    pub predicted_confidence: Option<f32>,
    /// Full Oracle forecast record (typed prediction or typed rejection) as
    /// persisted in the Oracle vault's Online CF.
    pub forecast_record: serde_json::Value,
    pub oracle_vault_dir: String,
    pub oracle_forecast_key_hex: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrashRecurrenceReadback {
    pub database: EngineCalyxDatabaseReadback,
    pub payload: CrashRecurrencePayload,
}

pub fn crash_recurrence_row_id(forecast_id: &str) -> String {
    format!("crash_recurrence/{forecast_id}")
}

pub fn crash_stream_id(crash_signature: &str) -> String {
    format!("crash_{crash_signature}")
}

/// Ingest one crash signature's telemetry occurrences into the Oracle service
/// and forecast the next recurrence. Insufficient history or non-stationary
/// series produce a typed, persisted rejection — never a silent skip.
pub fn forecast_crash_recurrence<C: Clock + Copy>(
    oracle: &mut EngineOracleService<C>,
    database: &EngineCalyxDatabase,
    forecast_id: &str,
    crash_signature: &str,
    events: &[EngineTelemetryEvent],
    now_secs: i64,
) -> Result<CrashRecurrenceReadback, EngineReplayAnalyticsError> {
    validate_id("forecast_id", forecast_id)?;
    validate_id("crash_signature", crash_signature)?;
    let occurrences = crash_occurrences(forecast_id, crash_signature, events)?;
    let stream_id = crash_stream_id(crash_signature);
    for t_secs in &occurrences {
        oracle
            .ingest_observation(LiveWorldObservation {
                stream_id: stream_id.clone(),
                kind: LiveWorldMetricKind::Event,
                t_secs: *t_secs,
                value: 1.0,
            })
            .map_err(|error| oracle_error_value(forecast_id, format!("ingest failed: {error}")))?;
    }
    // A typed rejection (insufficient history / non-stationary) is a valid,
    // persisted forecast outcome: the service writes the rejection record to
    // its Online CF before returning the loud error. Read it back from the
    // vault; any other failure stays fatal.
    let (record, vault_dir, forecast_key_hex) = match oracle.forecast_stream(ForecastRequest {
        stream_id: stream_id.clone(),
        kind: LiveWorldMetricKind::Event,
        now_secs,
    }) {
        Ok(report) => (
            report.record,
            report.source_truth.vault_dir,
            report.source_truth.forecast_key_hex,
        ),
        Err(
            crate::EngineError::OracleInsufficientHistory { .. }
            | crate::EngineError::OracleNonStationary { .. },
        ) => {
            let truth = oracle
                .read_source_truth(&stream_id, LiveWorldMetricKind::Event)
                .map_err(|error| {
                    oracle_error_value(forecast_id, format!("rejection readback failed: {error}"))
                })?;
            let record = truth.forecast_record.ok_or_else(|| {
                oracle_error_value(
                    forecast_id,
                    "forecast was rejected but no rejection record was persisted",
                )
            })?;
            (record, truth.vault_dir, truth.forecast_key_hex)
        }
        Err(error) => {
            return Err(oracle_error_value(
                forecast_id,
                format!("forecast failed: {error}"),
            ));
        }
    };
    let (outcome, rejection_reason, support, predicted_next_secs, predicted_confidence) =
        match &record.outcome {
            ForecastOutcome::Predicted(prediction) => (
                "predicted".to_string(),
                None,
                prediction.source_support,
                Some(prediction.oracle_time.t_hat.0),
                Some(prediction.oracle_time.confidence),
            ),
            ForecastOutcome::Rejected(rejection) => (
                "rejected".to_string(),
                Some(format!("{:?}", rejection.reason)),
                rejection.support,
                None,
                None,
            ),
        };
    let payload = CrashRecurrencePayload {
        schema: CRASH_RECURRENCE_SCHEMA.to_string(),
        forecast_id: forecast_id.to_string(),
        crash_signature: crash_signature.to_string(),
        stream_id,
        ingested_event_count: occurrences.len(),
        outcome,
        rejection_reason,
        support,
        predicted_next_secs,
        predicted_confidence,
        forecast_record: serde_json::to_value(&record).map_err(codec_failed)?,
        oracle_vault_dir: vault_dir,
        oracle_forecast_key_hex: forecast_key_hex,
    };
    let row_id = crash_recurrence_row_id(forecast_id);
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::Provenance,
        row_id.clone(),
        serde_json::to_value(&payload).map_err(codec_failed)?,
        json!({
            "source_issue": 907,
            "forecast_id": forecast_id,
            "crash_signature": crash_signature,
            "outcome": payload.outcome,
            "support": payload.support,
            "predicted_next_secs": payload.predicted_next_secs
        }),
    ))?;
    parse_crash_readback(stored, &row_id)
}

/// Read a previously stored crash-recurrence forecast row back from the
/// source of truth.
pub fn read_crash_recurrence(
    database: &EngineCalyxDatabase,
    forecast_id: &str,
) -> Result<CrashRecurrenceReadback, EngineReplayAnalyticsError> {
    validate_id("forecast_id", forecast_id)?;
    let row_id = crash_recurrence_row_id(forecast_id);
    let stored = database.get_record(EngineCalyxRecordKind::Provenance, &row_id)?;
    parse_crash_readback(stored, &row_id)
}

fn crash_occurrences(
    forecast_id: &str,
    crash_signature: &str,
    events: &[EngineTelemetryEvent],
) -> Result<Vec<i64>, EngineReplayAnalyticsError> {
    let mut occurrences = Vec::new();
    for event in events {
        if event.kind != EngineTelemetryEventKind::Crash {
            continue;
        }
        let signature = event.crash_signature.as_deref().ok_or_else(|| {
            oracle_error_value(
                forecast_id,
                format!(
                    "crash telemetry event {} has no crash_signature",
                    event.event_id
                ),
            )
        })?;
        if signature != crash_signature {
            continue;
        }
        let observed_secs = i64::try_from(event.observed_at_ms / 1000).map_err(|_| {
            oracle_error_value(
                forecast_id,
                format!("event {} observed_at_ms out of range", event.event_id),
            )
        })?;
        occurrences.push(observed_secs);
    }
    if occurrences.is_empty() {
        return Err(oracle_error_value(
            forecast_id,
            format!("no crash telemetry events match signature {crash_signature}"),
        ));
    }
    for window in occurrences.windows(2) {
        if window[1] <= window[0] {
            return Err(oracle_error_value(
                forecast_id,
                format!(
                    "crash occurrences must be strictly increasing in time: {} then {}",
                    window[0], window[1]
                ),
            ));
        }
    }
    Ok(occurrences)
}

fn parse_crash_readback(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<CrashRecurrenceReadback, EngineReplayAnalyticsError> {
    expect_row_id(&database, expected_row_id)?;
    let payload: CrashRecurrencePayload =
        serde_json::from_value(database.payload.clone()).map_err(codec_failed)?;
    expect_schema(&payload.schema, CRASH_RECURRENCE_SCHEMA)?;
    let predicted = payload.outcome == "predicted";
    if predicted && payload.predicted_next_secs.is_none() {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "crash_recurrence.predicted_next_secs".to_string(),
            expected: "present for a predicted outcome".to_string(),
            actual: "none".to_string(),
        });
    }
    if !predicted && payload.rejection_reason.is_none() {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "crash_recurrence.rejection_reason".to_string(),
            expected: "present for a rejected outcome".to_string(),
            actual: "none".to_string(),
        });
    }
    Ok(CrashRecurrenceReadback { database, payload })
}

fn oracle_error_value(forecast_id: &str, reason: impl ToString) -> EngineReplayAnalyticsError {
    EngineReplayAnalyticsError::Oracle {
        forecast_id: forecast_id.to_string(),
        reason: reason.to_string(),
    }
}
