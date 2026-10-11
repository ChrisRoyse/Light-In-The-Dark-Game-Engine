use std::path::{Path, PathBuf};

use calyx_anneal::{ShadowRevertReason, TripwireMetric};
use calyx_core::FixedClock;

use crate::anneal_optimization::{
    EngineAnnealOutcome, EngineAnnealRevertReason, EngineAnnealService, EngineAnnealServiceConfig,
    EngineTuneConfig, EngineTuneDomain, EngineTuneMetrics, EngineTuneParams,
};
use crate::{EngineError, fsv_artifacts::begin_issue_run};

const TEST_TS: u64 = 1_785_599_001;

#[test]
fn anneal_tuning_promotes_candidate_and_reopens_source_truth() {
    let root = test_root("promote");
    let mut anneal = service(&root);
    let incumbent = config("main", metrics(0.95, 0.004, 0.018, 120.0, 240.0));
    let candidate = config("main", metrics(0.97, 0.003, 0.016, 80.0, 210.0));

    anneal.install_live_config(&incumbent).unwrap();
    let report = anneal.run_tuning_pass(&incumbent, &candidate).unwrap();
    assert!(matches!(report.outcome, EngineAnnealOutcome::Promoted));
    assert_eq!(report.metrics.query_count, 8);
    assert_eq!(report.source_truth.live_config, Some(candidate.clone()));
    assert!(report.source_truth.ledger_rows.iter().any(|row| {
        row.payload_json["action"] == "autotune_promote"
            && row.payload_json["change_id"] == report.change_id
    }));

    drop(anneal);
    let reopened = service(&root);
    let state = reopened
        .read_source_truth(EngineTuneDomain::Index, "main")
        .unwrap();
    assert_eq!(state.live_config, Some(candidate));
    assert_eq!(state.rollback_rows.len(), 2);
}

#[test]
fn anneal_tuning_rolls_back_tripwire_breach() {
    let root = test_root("tripwire");
    let mut service = service(&root);
    let incumbent = config("guarded", metrics(0.95, 0.004, 0.018, 120.0, 240.0));
    let candidate = config("guarded", metrics(0.80, 0.003, 0.016, 80.0, 210.0));

    service.install_live_config(&incumbent).unwrap();
    let report = service.run_tuning_pass(&incumbent, &candidate).unwrap();
    match report.outcome {
        EngineAnnealOutcome::Reverted {
            reason:
                EngineAnnealRevertReason::Shadow {
                    shadow_reason: ShadowRevertReason::TripwireCrossed(TripwireMetric::RecallAtK),
                },
        } => {}
        other => panic!("unexpected outcome: {other:?}"),
    }
    assert_eq!(report.source_truth.live_config, Some(incumbent));
    assert!(report.source_truth.ledger_rows.iter().any(|row| {
        row.payload_json["action"] == "autotune_abandoned"
            && row.payload_json["details"]["engine_revert_reason"]["reason"] == "shadow"
    }));
}

#[test]
fn anneal_tuning_rolls_back_when_gain_hysteresis_is_not_met() {
    let root = test_root("gain");
    let mut service = service(&root);
    let incumbent = config("gain", metrics(0.95, 0.004, 0.018, 120.0, 240.0));
    let candidate = config("gain", metrics(0.95, 0.004, 0.018, 118.0, 240.0));

    service.install_live_config(&incumbent).unwrap();
    let report = service.run_tuning_pass(&incumbent, &candidate).unwrap();
    match report.outcome {
        EngineAnnealOutcome::Reverted {
            reason: EngineAnnealRevertReason::PromotionGate { metric, .. },
        } => assert_eq!(metric, TripwireMetric::SearchP99),
        other => panic!("unexpected outcome: {other:?}"),
    }
    assert_eq!(report.source_truth.live_config, Some(incumbent));
}

#[test]
fn anneal_tuning_fails_closed_when_live_pointer_is_missing() {
    let root = test_root("missing-live");
    let mut service = service(&root);
    let incumbent = config("missing", metrics(0.95, 0.004, 0.018, 120.0, 240.0));
    let candidate = config("missing", metrics(0.97, 0.003, 0.016, 80.0, 210.0));

    let error = service
        .run_tuning_pass(&incumbent, &candidate)
        .expect_err("missing live pointer must fail closed");
    assert!(matches!(error, EngineError::AnnealReadbackMismatch { .. }));
    assert_eq!(error.code(), "LITD-ANNEAL-READBACK-MISMATCH");
    let state = service
        .read_source_truth(EngineTuneDomain::Index, "missing")
        .unwrap();
    assert!(state.rollback_rows.is_empty());
    assert!(state.ledger_rows.is_empty());
}

fn service(root: &Path) -> EngineAnnealService<FixedClock> {
    EngineAnnealService::open_with_clock(
        EngineAnnealServiceConfig::for_vault(root.join("vault")),
        FixedClock::new(TEST_TS),
    )
    .unwrap()
}

fn config(profile_id: &str, metrics: EngineTuneMetrics) -> EngineTuneConfig {
    EngineTuneConfig {
        domain: EngineTuneDomain::Index,
        profile_id: profile_id.to_string(),
        params: EngineTuneParams::balanced(),
        metrics,
    }
}

fn metrics(
    recall_at_k: f64,
    guard_far: f64,
    guard_frr: f64,
    search_p99_ms: f64,
    ingest_p95_ms: f64,
) -> EngineTuneMetrics {
    EngineTuneMetrics {
        recall_at_k,
        guard_far,
        guard_frr,
        search_p99_ms,
        ingest_p95_ms,
    }
}

fn test_root(name: &str) -> PathBuf {
    begin_issue_run("issue1199_anneal_optimization")
        .unwrap()
        .artifact_dir(name)
        .unwrap()
}
