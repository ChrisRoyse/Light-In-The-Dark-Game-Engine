use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use calyx_anneal::TripwireMetric;
use calyx_core::FixedClock;
use litd_engine::anneal_optimization::{
    EngineAnnealOutcome, EngineAnnealService, EngineAnnealServiceConfig, EngineAnnealSourceTruth,
    EngineAnnealTuningReport, EngineTuneConfig, EngineTuneDomain, EngineTuneMetrics,
    EngineTuneParams,
};
use serde::{Deserialize, Serialize};

const FSV_TS: u64 = 1_785_599_119;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = fsv_root();
    reset_fsv_root(&root)?;
    let artifact_path = root.join("manual-fsv.json");

    let mut happy_service = service(&root, 8)?;
    let happy = run_scenario(
        &mut happy_service,
        "happy_promote",
        "prepare -> shadow-test held-out replay -> ledger -> promote",
        "candidate promoted and live pointer changes to candidate hash",
        incumbent("issue1199-happy"),
        candidate_fast("issue1199-happy"),
    )?;
    drop(happy_service);

    let mut empty_service = service(&root, 0)?;
    let empty_replay = run_scenario(
        &mut empty_service,
        "edge_empty_replay",
        "prepare with replay_query_count=0",
        "rollback with InsufficientReplay and exact prior live pointer",
        incumbent("issue1199-empty"),
        candidate_fast("issue1199-empty"),
    )?;
    drop(empty_service);

    let mut guarded_service = service(&root, 8)?;
    guarded_service.set_tripwire(TripwireMetric::RecallAtK, 0.90, 0.05)?;
    let tripwire = run_scenario(
        &mut guarded_service,
        "edge_tripwire_breach",
        "candidate recall_at_k=0.85 below threshold 0.90",
        "rollback with TripwireCrossed(RecallAtK)",
        incumbent("issue1199-hysteresis"),
        candidate_low_recall("issue1199-hysteresis", 0.85),
    )?;
    let hysteresis = run_scenario_without_install(
        &mut guarded_service,
        "edge_hysteresis_band",
        "candidate recall_at_k=0.91 after prior breach with hysteresis 0.05",
        "rollback remains crossed until recall reaches 0.95",
        incumbent("issue1199-hysteresis"),
        candidate_low_recall("issue1199-hysteresis", 0.91),
    )?;
    drop(guarded_service);

    let mut gain_service = service(&root, 8)?;
    let gain_gate = run_scenario(
        &mut gain_service,
        "edge_gain_hysteresis",
        "candidate p99 improves only 1.7 percent with 5 percent required gain",
        "rollback with PromotionGate(SearchP99)",
        incumbent("issue1199-gain"),
        candidate_small_gain("issue1199-gain"),
    )?;
    drop(gain_service);

    let mut artifact = ManualFsvArtifact {
        issue: 1199,
        source_of_truth:
            "Aster AnnealRollback CF, Aster Ledger CF, and .anneal/engine-configs JSON files"
                .to_string(),
        vault_dir: root.join("vault").display().to_string(),
        happy_path: happy,
        edges: vec![empty_replay, tripwire, hysteresis, gain_gate],
        source_truth_readback: None,
    };
    write_artifact(&artifact_path, &artifact)?;

    let readback_service = service(&root, 8)?;
    artifact.source_truth_readback = Some(final_readback(&readback_service, &artifact_path)?);
    write_artifact(&artifact_path, &artifact)?;
    stabilize_artifact_size(&artifact_path)?;
    let persisted = read_artifact(&artifact_path)?;
    println!(
        "FSV_SOURCE_OF_TRUTH_READBACK path={} bytes={} ledger_actions={:?} live_hashes={:?}",
        artifact_path.display(),
        fs::metadata(&artifact_path)?.len(),
        persisted
            .source_truth_readback
            .as_ref()
            .map(|readback| readback.ledger_actions.clone())
            .unwrap_or_default(),
        persisted
            .source_truth_readback
            .as_ref()
            .map(|readback| readback.live_hashes.clone())
            .unwrap_or_default(),
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualFsvArtifact {
    issue: u32,
    source_of_truth: String,
    vault_dir: String,
    happy_path: ScenarioEvidence,
    edges: Vec<ScenarioEvidence>,
    source_truth_readback: Option<FinalReadback>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ScenarioEvidence {
    name: String,
    trigger: String,
    expected: String,
    before: EngineAnnealSourceTruth,
    report: EngineAnnealTuningReport,
    after: EngineAnnealSourceTruth,
}

#[derive(Debug, Deserialize, Serialize)]
struct FinalReadback {
    artifact_bytes: u64,
    ledger_actions: Vec<String>,
    live_hashes: Vec<String>,
    rollback_row_count: usize,
    config_file_count: usize,
}

fn run_scenario(
    service: &mut EngineAnnealService<FixedClock>,
    name: &str,
    trigger: &str,
    expected: &str,
    incumbent: EngineTuneConfig,
    candidate: EngineTuneConfig,
) -> Result<ScenarioEvidence, Box<dyn std::error::Error>> {
    service.install_live_config(&incumbent)?;
    run_scenario_without_install(service, name, trigger, expected, incumbent, candidate)
}

fn run_scenario_without_install(
    service: &mut EngineAnnealService<FixedClock>,
    name: &str,
    trigger: &str,
    expected: &str,
    incumbent: EngineTuneConfig,
    candidate: EngineTuneConfig,
) -> Result<ScenarioEvidence, Box<dyn std::error::Error>> {
    let before = service.read_source_truth(incumbent.domain, &incumbent.profile_id)?;
    let report = service.run_tuning_pass(&incumbent, &candidate)?;
    let after = service.read_source_truth(incumbent.domain, &incumbent.profile_id)?;
    print_state(name, &before, &report.outcome, &after);
    Ok(ScenarioEvidence {
        name: name.to_string(),
        trigger: trigger.to_string(),
        expected: expected.to_string(),
        before,
        report,
        after,
    })
}

fn print_state(
    name: &str,
    before: &EngineAnnealSourceTruth,
    outcome: &EngineAnnealOutcome,
    after: &EngineAnnealSourceTruth,
) {
    println!(
        "FSV_CASE name={} before_live={:?} before_rollback_rows={} before_ledger_rows={} outcome={:?} after_live={:?} after_rollback_rows={} after_ledger_rows={}",
        name,
        before.live_config_hash,
        before.rollback_rows.len(),
        before.ledger_rows.len(),
        outcome,
        after.live_config_hash,
        after.rollback_rows.len(),
        after.ledger_rows.len()
    );
}

fn final_readback(
    service: &EngineAnnealService<FixedClock>,
    artifact_path: &Path,
) -> Result<FinalReadback, Box<dyn std::error::Error>> {
    let state = service.read_source_truth(EngineTuneDomain::Index, "issue1199-happy")?;
    let mut ledger_actions = state
        .ledger_rows
        .iter()
        .filter_map(|row| row.payload_json["action"].as_str().map(ToString::to_string))
        .collect::<Vec<_>>();
    ledger_actions.sort();
    let mut live_hashes = Vec::new();
    for profile in [
        "issue1199-happy",
        "issue1199-empty",
        "issue1199-hysteresis",
        "issue1199-gain",
    ] {
        if let Some(hash) = service
            .read_source_truth(EngineTuneDomain::Index, profile)?
            .live_config_hash
        {
            live_hashes.push(format!("{profile}:{hash}"));
        }
    }
    live_hashes.sort();
    Ok(FinalReadback {
        artifact_bytes: fs::metadata(artifact_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        ledger_actions,
        live_hashes,
        rollback_row_count: state.rollback_rows.len(),
        config_file_count: state.config_files.len(),
    })
}

fn service(
    root: &Path,
    replay_query_count: usize,
) -> Result<EngineAnnealService<FixedClock>, Box<dyn std::error::Error>> {
    let mut config = EngineAnnealServiceConfig::for_vault(root.join("vault"));
    config.replay_query_count = replay_query_count;
    Ok(EngineAnnealService::open_with_clock(
        config,
        FixedClock::new(FSV_TS),
    )?)
}

fn incumbent(profile_id: &str) -> EngineTuneConfig {
    EngineTuneConfig {
        domain: EngineTuneDomain::Index,
        profile_id: profile_id.to_string(),
        params: EngineTuneParams::balanced(),
        metrics: metrics(0.95, 0.004, 0.018, 120.0, 240.0),
    }
}

fn candidate_fast(profile_id: &str) -> EngineTuneConfig {
    EngineTuneConfig {
        domain: EngineTuneDomain::Index,
        profile_id: profile_id.to_string(),
        params: EngineTuneParams {
            index_ef_search: 96,
            render_max_draws: 4608,
            ..EngineTuneParams::balanced()
        },
        metrics: metrics(0.97, 0.003, 0.016, 80.0, 210.0),
    }
}

fn candidate_low_recall(profile_id: &str, recall: f64) -> EngineTuneConfig {
    EngineTuneConfig {
        domain: EngineTuneDomain::Index,
        profile_id: profile_id.to_string(),
        params: EngineTuneParams {
            index_ef_search: 48,
            ..EngineTuneParams::balanced()
        },
        metrics: metrics(recall, 0.003, 0.016, 80.0, 210.0),
    }
}

fn candidate_small_gain(profile_id: &str) -> EngineTuneConfig {
    EngineTuneConfig {
        domain: EngineTuneDomain::Index,
        profile_id: profile_id.to_string(),
        params: EngineTuneParams {
            index_ef_search: 72,
            ..EngineTuneParams::balanced()
        },
        metrics: metrics(0.95, 0.004, 0.018, 118.0, 240.0),
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

fn fsv_root() -> PathBuf {
    env::var("LITD_ANNEAL_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new("target/fsv/issue1199_engine_anneal").into())
}

fn reset_fsv_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
        let Some(readback) = artifact.source_truth_readback.as_mut() else {
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
