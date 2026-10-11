use litd_engine::EngineError;

use crate::play::PlayCameraRig;

use super::{
    HarnessOracleKind, HarnessOracleRunRecord, HarnessRunRecord,
    mcp_harness_tools_build::build_headless_run_evidence,
    mcp_harness_tools_oracles::build_oracle_record,
};

const PROJECT_PLAYTEST_ORACLES: [HarnessOracleKind; 9] = [
    HarnessOracleKind::DeterministicReplay,
    HarnessOracleKind::PresentationNonEmpty,
    HarnessOracleKind::NoErrorVerdict,
    HarnessOracleKind::CrashFree,
    HarnessOracleKind::SoftLockFree,
    HarnessOracleKind::PhysicsBounds,
    HarnessOracleKind::PerfBudget,
    HarnessOracleKind::EconomyRewardSanity,
    HarnessOracleKind::SoakStable,
];

pub(crate) struct ProjectHeadlessPlaytestOracleEvaluation {
    pub run: HarnessRunRecord,
    pub player_id: String,
    pub oracle_verdict: HarnessOracleRunRecord,
    pub verdict_reason: String,
}

pub(crate) fn build_project_headless_playtest_verdict(
    run_id: &str,
    oracle_run_id: &str,
    project_path: &str,
    camera_rig: PlayCameraRig,
    ticks: u64,
) -> Result<ProjectHeadlessPlaytestOracleEvaluation, EngineError> {
    let evidence = build_headless_run_evidence(run_id, project_path, camera_rig, ticks, None)?;
    let oracle_verdict =
        build_oracle_record(oracle_run_id, &evidence.run, &PROJECT_PLAYTEST_ORACLES)?;
    let verdict_reason = summarize_oracle_verdict(&oracle_verdict);
    Ok(ProjectHeadlessPlaytestOracleEvaluation {
        run: evidence.run,
        player_id: evidence.player_id,
        oracle_verdict,
        verdict_reason,
    })
}

fn summarize_oracle_verdict(record: &HarnessOracleRunRecord) -> String {
    if record.failed_count == 0 {
        return format!(
            "all {} headless playtest harness oracles passed",
            record.check_count
        );
    }
    let failures = record
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| {
            format!(
                "{} actual=[{}] expected=[{}]",
                oracle_label(check.oracle),
                check.actual,
                check.reason
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{} of {} headless playtest harness oracles failed: {}",
        record.failed_count, record.check_count, failures
    )
}

fn oracle_label(oracle: HarnessOracleKind) -> &'static str {
    match oracle {
        HarnessOracleKind::DeterministicReplay => "deterministic_replay",
        HarnessOracleKind::PresentationNonEmpty => "presentation_non_empty",
        HarnessOracleKind::NoErrorVerdict => "no_error_verdict",
        HarnessOracleKind::CrashFree => "crash_free",
        HarnessOracleKind::SoftLockFree => "soft_lock_free",
        HarnessOracleKind::PhysicsBounds => "physics_bounds",
        HarnessOracleKind::PerfBudget => "perf_budget",
        HarnessOracleKind::EconomyRewardSanity => "economy_reward_sanity",
        HarnessOracleKind::SoakStable => "soak_stable",
    }
}
