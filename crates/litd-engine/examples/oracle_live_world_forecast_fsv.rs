use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use calyx_core::FixedClock;
use litd_engine::EngineError;
use litd_engine::live_world_forecast::{
    EngineOracleService, EngineOracleServiceConfig, EngineOracleSourceTruth, ForecastOutcome,
    ForecastRequest, LiveWorldMetricKind, LiveWorldObservation,
};
use serde::{Deserialize, Serialize};

const FSV_TS: u64 = 1_785_599_198;
const DAY: i64 = 86_400;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = fsv_root();
    reset_root(&root)?;
    let artifact_path = root.join("manual-fsv.json");

    let happy = run_happy(&root)?;
    let insufficient = run_insufficient(&root)?;
    let non_stationary = run_non_stationary(&root)?;
    let overdue = run_overdue(&root)?;

    let mut artifact = ManualFsvArtifact {
        issue: 1198,
        source_of_truth: "Aster Base CF, Recurrence CF, Online CF forecast row, and Ledger CF"
            .to_string(),
        cases: vec![happy, insufficient, non_stationary, overdue],
        final_readback: None,
    };
    write_artifact(&artifact_path, &artifact)?;
    artifact.final_readback = Some(final_readback(&root, &artifact_path)?);
    write_artifact(&artifact_path, &artifact)?;
    stabilize_artifact_size(&artifact_path)?;
    let persisted = read_artifact(&artifact_path)?;
    println!(
        "FSV_SOURCE_OF_TRUTH_READBACK path={} bytes={} cases={} online_files={} recurrence_files={} ledger_files={}",
        artifact_path.display(),
        fs::metadata(&artifact_path)?.len(),
        persisted.cases.len(),
        persisted
            .final_readback
            .as_ref()
            .map(|readback| readback.online_files.len())
            .unwrap_or_default(),
        persisted
            .final_readback
            .as_ref()
            .map(|readback| readback.recurrence_files.len())
            .unwrap_or_default(),
        persisted
            .final_readback
            .as_ref()
            .map(|readback| readback.ledger_files.len())
            .unwrap_or_default(),
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualFsvArtifact {
    issue: u32,
    source_of_truth: String,
    cases: Vec<CaseEvidence>,
    final_readback: Option<FinalReadback>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CaseEvidence {
    name: String,
    trigger: String,
    expected: String,
    before: StateSummary,
    action: ActionEvidence,
    after: StateSummary,
    source_truth_after: EngineOracleSourceTruth,
}

#[derive(Debug, Deserialize, Serialize)]
struct ActionEvidence {
    ok: bool,
    error_code: Option<String>,
    outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StateSummary {
    forecast_hash: Option<String>,
    recurrence_rows: usize,
    ledger_rows: usize,
    outcome: Option<String>,
    predicted_t_hat: Option<i64>,
    predicted_value: Option<f64>,
    overdue: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FinalReadback {
    artifact_bytes: u64,
    online_files: Vec<FileReadback>,
    recurrence_files: Vec<FileReadback>,
    ledger_files: Vec<FileReadback>,
    persisted_outcomes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileReadback {
    path: String,
    bytes: u64,
}

fn run_happy(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "happy")?;
    seed_linear_daily(
        &mut service,
        "economy.gold",
        LiveWorldMetricKind::Economy,
        8,
    )?;
    let before = service.read_source_truth("economy.gold", LiveWorldMetricKind::Economy)?;
    let report = service.forecast_stream(request(
        "economy.gold",
        LiveWorldMetricKind::Economy,
        7 * DAY + 3_600,
    ))?;
    let after = service.read_source_truth("economy.gold", LiveWorldMetricKind::Economy)?;
    print_case("happy_periodic_trend", &before, &after, Some("ok"));
    assert_predicted(&report.record.outcome, 8 * DAY, 124.0, false);
    Ok(case(
        "happy_periodic_trend",
        "8 daily economy observations, +3 value per day",
        "forecast t_hat=day 8, predicted_value=124, anomaly.overdue=false",
        before,
        ActionEvidence {
            ok: true,
            error_code: None,
            outcome: Some("predicted".to_string()),
        },
        after,
    ))
}

fn run_insufficient(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "insufficient")?;
    seed_linear_daily(
        &mut service,
        "population.outpost",
        LiveWorldMetricKind::Population,
        3,
    )?;
    let before =
        service.read_source_truth("population.outpost", LiveWorldMetricKind::Population)?;
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
    let after = service.read_source_truth("population.outpost", LiveWorldMetricKind::Population)?;
    print_case(
        "edge_insufficient_history",
        &before,
        &after,
        Some(error.code()),
    );
    Ok(case(
        "edge_insufficient_history",
        "3 observations with min_history=8",
        "fail closed with LITD-ORACLE-INSUFFICIENT-HISTORY and persisted rejection",
        before,
        action_error(error),
        after,
    ))
}

fn run_non_stationary(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "nonstationary")?;
    let mut time = 0_i64;
    for idx in 0..9 {
        service.ingest_observation(LiveWorldObservation {
            stream_id: "event.raid".to_string(),
            kind: LiveWorldMetricKind::Event,
            t_secs: time,
            value: idx as f64,
        })?;
        time += if idx < 4 { DAY } else { 2 * DAY };
    }
    let before = service.read_source_truth("event.raid", LiveWorldMetricKind::Event)?;
    let error = service
        .forecast_stream(request("event.raid", LiveWorldMetricKind::Event, time))
        .expect_err("non-stationary cadence must fail closed");
    assert!(matches!(error, EngineError::OracleNonStationary { .. }));
    let after = service.read_source_truth("event.raid", LiveWorldMetricKind::Event)?;
    print_case(
        "edge_non_stationary_cadence",
        &before,
        &after,
        Some(error.code()),
    );
    Ok(case(
        "edge_non_stationary_cadence",
        "daily cadence shifts to every two days",
        "fail closed with LITD-ORACLE-NON-STATIONARY and persisted rejection",
        before,
        action_error(error),
        after,
    ))
}

fn run_overdue(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "overdue")?;
    seed_linear_daily(&mut service, "event.market", LiveWorldMetricKind::Event, 8)?;
    let before = service.read_source_truth("event.market", LiveWorldMetricKind::Event)?;
    let report =
        service.forecast_stream(request("event.market", LiveWorldMetricKind::Event, 9 * DAY))?;
    let after = service.read_source_truth("event.market", LiveWorldMetricKind::Event)?;
    print_case(
        "edge_overdue_anomaly_separated",
        &before,
        &after,
        Some("ok"),
    );
    assert_predicted(&report.record.outcome, 8 * DAY, 124.0, true);
    Ok(case(
        "edge_overdue_anomaly_separated",
        "stable daily event stream checked after missing its next occurrence",
        "forecast remains predicted while anomaly.overdue=true is stored separately",
        before,
        ActionEvidence {
            ok: true,
            error_code: None,
            outcome: Some("predicted".to_string()),
        },
        after,
    ))
}

fn case(
    name: &str,
    trigger: &str,
    expected: &str,
    before: EngineOracleSourceTruth,
    action: ActionEvidence,
    after: EngineOracleSourceTruth,
) -> CaseEvidence {
    CaseEvidence {
        name: name.to_string(),
        trigger: trigger.to_string(),
        expected: expected.to_string(),
        before: summarize(&before),
        action,
        after: summarize(&after),
        source_truth_after: after,
    }
}

fn summarize(state: &EngineOracleSourceTruth) -> StateSummary {
    let (outcome, predicted_t_hat, predicted_value, overdue) = state
        .forecast_record
        .as_ref()
        .map(|record| match &record.outcome {
            ForecastOutcome::Predicted(prediction) => (
                Some("predicted".to_string()),
                Some(prediction.oracle_time.t_hat.0),
                Some(prediction.predicted_value),
                Some(prediction.anomaly.hazard.overdue),
            ),
            ForecastOutcome::Rejected(rejection) => (
                Some(format!("rejected:{:?}", rejection.reason)),
                None,
                None,
                None,
            ),
        })
        .unwrap_or((None, None, None, None));
    StateSummary {
        forecast_hash: state.forecast_row_hash.clone(),
        recurrence_rows: state.recurrence_rows.len(),
        ledger_rows: state.ledger_rows.len(),
        outcome,
        predicted_t_hat,
        predicted_value,
        overdue,
    }
}

fn action_error(error: EngineError) -> ActionEvidence {
    ActionEvidence {
        ok: false,
        error_code: Some(error.code().to_string()),
        outcome: None,
    }
}

fn assert_predicted(outcome: &ForecastOutcome, t_hat: i64, value: f64, overdue: bool) {
    match outcome {
        ForecastOutcome::Predicted(prediction) => {
            assert_eq!(prediction.oracle_time.t_hat.0, t_hat);
            assert!((prediction.predicted_value - value).abs() < 1.0e-6);
            assert_eq!(prediction.anomaly.hazard.overdue, overdue);
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

fn print_case(
    name: &str,
    before: &EngineOracleSourceTruth,
    after: &EngineOracleSourceTruth,
    action: Option<&str>,
) {
    println!(
        "FSV_CASE name={} before_hash={:?} before_recurrence={} before_ledger={} action={} after_hash={:?} after_recurrence={} after_ledger={} after_outcome={:?}",
        name,
        before.forecast_row_hash,
        before.recurrence_rows.len(),
        before.ledger_rows.len(),
        action.unwrap_or("unknown"),
        after.forecast_row_hash,
        after.recurrence_rows.len(),
        after.ledger_rows.len(),
        summarize(after).outcome,
    );
}

fn seed_linear_daily(
    service: &mut EngineOracleService<FixedClock>,
    stream_id: &str,
    kind: LiveWorldMetricKind,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    for idx in 0..count {
        service.ingest_observation(LiveWorldObservation {
            stream_id: stream_id.to_string(),
            kind,
            t_secs: idx as i64 * DAY,
            value: 100.0 + idx as f64 * 3.0,
        })?;
    }
    Ok(())
}

fn request(stream_id: &str, kind: LiveWorldMetricKind, now_secs: i64) -> ForecastRequest {
    ForecastRequest {
        stream_id: stream_id.to_string(),
        kind,
        now_secs,
    }
}

fn service(
    root: &Path,
    name: &str,
) -> Result<EngineOracleService<FixedClock>, Box<dyn std::error::Error>> {
    Ok(EngineOracleService::open_with_clock(
        EngineOracleServiceConfig::for_vault(root.join(name).join("vault")),
        FixedClock::new(FSV_TS),
    )?)
}

fn final_readback(
    root: &Path,
    artifact_path: &Path,
) -> Result<FinalReadback, Box<dyn std::error::Error>> {
    let mut persisted_outcomes = Vec::new();
    for (case_dir, stream_id, kind) in [
        ("happy", "economy.gold", LiveWorldMetricKind::Economy),
        (
            "insufficient",
            "population.outpost",
            LiveWorldMetricKind::Population,
        ),
        ("nonstationary", "event.raid", LiveWorldMetricKind::Event),
        ("overdue", "event.market", LiveWorldMetricKind::Event),
    ] {
        let service = service(root, case_dir)?;
        let state = service.read_source_truth(stream_id, kind)?;
        persisted_outcomes.push(format!("{}:{:?}", case_dir, summarize(&state).outcome));
    }
    Ok(FinalReadback {
        artifact_bytes: fs::metadata(artifact_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        online_files: files_under(root, "online")?,
        recurrence_files: files_under(root, "recurrence")?,
        ledger_files: files_under(root, "ledger")?,
        persisted_outcomes,
    })
}

fn files_under(
    root: &Path,
    cf_name: &str,
) -> Result<Vec<FileReadback>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let vault = entry?.path().join("vault").join("cf").join(cf_name);
        if !vault.exists() {
            continue;
        }
        for file in fs::read_dir(vault)? {
            let path = file?.path();
            if path.is_file() {
                files.push(FileReadback {
                    bytes: fs::metadata(&path)?.len(),
                    path: path.display().to_string(),
                });
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn fsv_root() -> PathBuf {
    env::var("LITD_ORACLE_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new("target/fsv/issue1198_engine_oracle").into())
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

fn write_artifact(
    path: &Path,
    artifact: &ManualFsvArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(artifact)?)?;
    Ok(())
}

fn read_artifact(path: &Path) -> Result<ManualFsvArtifact, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn stabilize_artifact_size(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..3 {
        let mut artifact = read_artifact(path)?;
        let bytes = fs::metadata(path)?.len();
        let Some(readback) = artifact.final_readback.as_mut() else {
            return Ok(());
        };
        if readback.artifact_bytes == bytes {
            return Ok(());
        }
        readback.artifact_bytes = bytes;
        write_artifact(path, &artifact)?;
    }
    Ok(())
}
