use std::path::{Path, PathBuf};

use calyx_core::FixedClock;

use crate::live_world_forecast::{
    EngineOracleService, EngineOracleServiceConfig, ForecastOutcome, ForecastRejectionReason,
    ForecastRequest, LiveWorldMetricKind, LiveWorldObservation,
};
use crate::{EngineError, fsv_artifacts::begin_issue_run};

const TEST_TS: u64 = 1_785_599_198;
const DAY: i64 = 86_400;

#[test]
fn oracle_forecast_persists_prediction_and_reopens_source_truth() {
    let root = test_root("happy");
    let mut service = open_service(&root);
    seed_linear_daily(&mut service, "economy.gold", 8);

    let report = service
        .forecast_stream(request(
            "economy.gold",
            LiveWorldMetricKind::Economy,
            7 * DAY + 3_600,
        ))
        .unwrap();
    let prediction = match &report.record.outcome {
        ForecastOutcome::Predicted(prediction) => prediction,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(prediction.source_support, 8);
    assert_eq!(prediction.oracle_time.t_hat.0, 8 * DAY);
    assert_eq!(prediction.oracle_time.cadence_secs, DAY as f64);
    assert!((prediction.predicted_value - 124.0).abs() < 1.0e-6);
    assert!(!prediction.anomaly.hazard.overdue);
    assert_eq!(prediction.anomaly.hazard.gamma_shape, None);
    assert!(report.source_truth.forecast_row_hash.is_some());
    assert_eq!(report.source_truth.recurrence_rows.len(), 8);
    let forecast_hash = report.source_truth.forecast_row_hash.as_deref().unwrap();
    assert!(report.source_truth.ledger_rows.iter().any(|row| {
        row.payload_json["outcome"] == "predicted"
            && row.payload_json["forecast_row_hash"] == forecast_hash
    }));

    drop(service);
    let reopened = open_service(&root);
    let readback = reopened
        .read_source_truth("economy.gold", LiveWorldMetricKind::Economy)
        .unwrap();
    assert_eq!(readback.forecast_record.as_ref(), Some(&report.record));
    let reopened_record = readback.forecast_record.as_ref().unwrap();
    let reopened_prediction = match &reopened_record.outcome {
        ForecastOutcome::Predicted(prediction) => prediction,
        other => panic!("unexpected reopened outcome: {other:?}"),
    };
    assert_eq!(reopened_prediction.anomaly.hazard.gamma_shape, None);
    assert_eq!(readback.recurrence_rows.len(), 8);
    assert_eq!(readback.ledger_rows.len(), 1);
}

#[test]
fn oracle_forecast_fails_closed_and_persists_insufficient_history() {
    let root = test_root("insufficient");
    let mut service = open_service(&root);
    seed_linear_daily(&mut service, "population.outpost", 3);

    let error = service
        .forecast_stream(request(
            "population.outpost",
            LiveWorldMetricKind::Population,
            2 * DAY,
        ))
        .expect_err("insufficient history must fail closed");
    assert!(matches!(
        error,
        EngineError::OracleInsufficientHistory { .. }
    ));
    assert_eq!(error.code(), "LITD-ORACLE-INSUFFICIENT-HISTORY");
    let readback = service
        .read_source_truth("population.outpost", LiveWorldMetricKind::Population)
        .unwrap();
    let record = readback.forecast_record.unwrap();
    match record.outcome {
        ForecastOutcome::Rejected(rejection) => {
            assert_eq!(
                rejection.reason,
                ForecastRejectionReason::InsufficientHistory
            );
            assert_eq!(rejection.support, 3);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
    assert_eq!(readback.recurrence_rows.len(), 3);
    assert_eq!(readback.ledger_rows.len(), 1);
}

#[test]
fn oracle_forecast_fails_closed_on_non_stationary_cadence() {
    let root = test_root("nonstationary");
    let mut service = open_service(&root);
    let mut time = 0_i64;
    for idx in 0..9 {
        service
            .ingest_observation(LiveWorldObservation {
                stream_id: "event.raid".to_string(),
                kind: LiveWorldMetricKind::Event,
                t_secs: time,
                value: idx as f64,
            })
            .unwrap();
        time += if idx < 4 { DAY } else { 2 * DAY };
    }

    let error = service
        .forecast_stream(request("event.raid", LiveWorldMetricKind::Event, time))
        .expect_err("regime shift must fail closed");
    assert!(matches!(error, EngineError::OracleNonStationary { .. }));
    assert_eq!(error.code(), "LITD-ORACLE-NON-STATIONARY");
    let readback = service
        .read_source_truth("event.raid", LiveWorldMetricKind::Event)
        .unwrap();
    let record = readback.forecast_record.unwrap();
    match record.outcome {
        ForecastOutcome::Rejected(rejection) => {
            assert_eq!(rejection.reason, ForecastRejectionReason::NonStationary);
            assert_eq!(rejection.support, 9);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn oracle_forecast_keeps_overdue_anomaly_separate_from_prediction() {
    let root = test_root("overdue");
    let mut service = open_service(&root);
    seed_linear_daily(&mut service, "event.market", 8);

    let report = service
        .forecast_stream(request("event.market", LiveWorldMetricKind::Event, 9 * DAY))
        .unwrap();
    let prediction = match report.record.outcome {
        ForecastOutcome::Predicted(prediction) => prediction,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(prediction.oracle_time.t_hat.0, 8 * DAY);
    assert!(prediction.anomaly.hazard.overdue);
    assert_eq!(prediction.anomaly.hazard.gamma_shape, None);
    assert!(
        report
            .source_truth
            .forecast_record
            .as_ref()
            .is_some_and(|record| matches!(record.outcome, ForecastOutcome::Predicted(_)))
    );
}

fn seed_linear_daily(service: &mut EngineOracleService<FixedClock>, stream_id: &str, count: usize) {
    for idx in 0..count {
        service
            .ingest_observation(LiveWorldObservation {
                stream_id: stream_id.to_string(),
                kind: if stream_id.starts_with("population") {
                    LiveWorldMetricKind::Population
                } else if stream_id.starts_with("event") {
                    LiveWorldMetricKind::Event
                } else {
                    LiveWorldMetricKind::Economy
                },
                t_secs: idx as i64 * DAY,
                value: 100.0 + idx as f64 * 3.0,
            })
            .unwrap();
    }
}

fn request(stream_id: &str, kind: LiveWorldMetricKind, now_secs: i64) -> ForecastRequest {
    ForecastRequest {
        stream_id: stream_id.to_string(),
        kind,
        now_secs,
    }
}

fn open_service(root: &Path) -> EngineOracleService<FixedClock> {
    EngineOracleService::open_with_clock(
        EngineOracleServiceConfig::for_vault(root.join("vault")),
        FixedClock::new(TEST_TS),
    )
    .unwrap()
}

fn test_root(name: &str) -> PathBuf {
    begin_issue_run("issue1198_live_world_forecast")
        .unwrap()
        .artifact_dir(name)
        .unwrap()
}
