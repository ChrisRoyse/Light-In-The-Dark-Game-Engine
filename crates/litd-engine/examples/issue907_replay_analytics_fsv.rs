//! #907 FSV: replay & telemetry analytics loop — grounded balance (Assay),
//! desync forensics (Lodestar-style), recurrence forecast (Oracle), all
//! offline/read-only, persisted in Calyx-backed Aster vaults.
//!
//! Run: cargo run -p litd-engine --features fsv --example issue907_replay_analytics_fsv
//!
//! Source of truth: the Aster vault rows under the run directory. The final
//! step REOPENS the vault from disk and reads every row back; command success
//! alone is not proof.

use calyx_core::FixedClock;
use litd_engine::{
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_replay_analytics::{
        ReplayMatchRecord, read_replay_match_batch, store_replay_match_batch,
    },
    calyx_replay_analytics_assay::{
        ReplayBalanceAssayReadback, measure_replay_balance_assay, read_replay_balance_assay,
    },
    calyx_replay_analytics_desync::{DesyncVerdict, read_desync_forensics, run_desync_forensics},
    calyx_replay_analytics_oracle::{forecast_crash_recurrence, read_crash_recurrence},
    fsv_artifacts::begin_issue_run,
    live_world_forecast::{EngineOracleService, EngineOracleServiceConfig},
    sim_order::{SimOrder, SimOrderKind},
    sim_replay::SimReplayRecord,
};
use serde_json::{Value, json};

#[path = "issue907_replay_analytics_fsv/support.rs"]
mod support;

use support::{
    DECOY_STATS, DRIVER_STATS, DeterministicStream, MATCH_COUNT, MB, MO, RUN_TICKS, build_match,
    crash_event, crash_series,
};

const BATCH_ID: &str = "batch-issue907";
const FSV_TS: u64 = 1_785_599_198;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue907_replay_analytics")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let oracle_dir = run.run_dir().join("oracle-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    // ---- 1. Ingest: real executed match replays -> Calyx rows -------------
    let mut stream = DeterministicStream::new(0x907_907_907);
    let matches: Vec<ReplayMatchRecord> = (0..MATCH_COUNT)
        .map(|index| build_match(index, &mut stream))
        .collect();
    let batch = store_replay_match_batch::<MB, MO>(&database, BATCH_ID, &matches)?;
    if batch.payload.win_count < 12 || batch.payload.loss_count < 12 {
        return Err(format!(
            "GATE_FAILED: step=ingest detail=degenerate class balance win={} loss={}",
            batch.payload.win_count, batch.payload.loss_count
        )
        .into());
    }
    let first_match = &batch.matches[0];
    println!(
        "INGEST batch={} matches={} wins={} losses={} first_row_sha256={} first_tick_events={}",
        BATCH_ID,
        batch.payload.match_count,
        batch.payload.win_count,
        batch.payload.loss_count,
        first_match.database.sha256,
        first_match.payload.tick_events.len()
    );

    // ---- 2. Assay: which stats predict win/loss, in bits ------------------
    let assay = measure_replay_balance_assay(&database, BATCH_ID)?;
    print_ranked(&assay);
    verify_measured_balance(&assay)?;

    // ---- 3. Desync forensics over divergent replays -----------------------
    let replay_a = matches[0].replay.clone();
    let replay_b = corrupted_peer_replay(&replay_a)?;
    let diverged =
        run_desync_forensics::<MB, MO>(&database, "case-corrupted-input", &replay_a, &replay_b)?;
    if diverged.payload.verdict != DesyncVerdict::Diverged
        || diverged.payload.first_divergent_tick != Some(1)
        || !diverged.payload.order_log_divergence
    {
        return Err(format!(
            "GATE_FAILED: step=desync detail=expected diverged at tick 1, got {:?} tick {:?}",
            diverged.payload.verdict, diverged.payload.first_divergent_tick
        )
        .into());
    }
    let clean = run_desync_forensics::<MB, MO>(&database, "case-clean", &replay_a, &replay_a)?;
    if clean.payload.verdict != DesyncVerdict::Clean || clean.payload.compared_ticks != RUN_TICKS {
        return Err(format!(
            "GATE_FAILED: step=desync-clean detail=expected clean over {RUN_TICKS} ticks, got {:?} over {}",
            clean.payload.verdict, clean.payload.compared_ticks
        )
        .into());
    }
    println!(
        "DESYNC diverged_case first_divergent_tick={:?} order_ticks={:?} clean_case_ticks={}",
        diverged.payload.first_divergent_tick,
        diverged.payload.divergent_order_ticks,
        clean.payload.compared_ticks
    );

    // ---- 4. Oracle: crash recurrence forecast over telemetry --------------
    let mut oracle = EngineOracleService::open_with_clock(
        EngineOracleServiceConfig::for_vault(&oracle_dir),
        FixedClock::new(FSV_TS),
    )?;
    let gpu_hang = crash_series("sig-gpu-hang", 1_000, 3_600, 12);
    let last_secs = 1_000 + 3_600 * 11;
    let predicted = forecast_crash_recurrence(
        &mut oracle,
        &database,
        "fc-gpu-hang",
        "sig-gpu-hang",
        &gpu_hang,
        last_secs + 120,
    )?;
    let next = predicted
        .payload
        .predicted_next_secs
        .ok_or("GATE_FAILED: step=oracle detail=expected a predicted next crash time")?;
    if predicted.payload.outcome != "predicted" || next <= last_secs || next > last_secs + 7_200 {
        return Err(format!(
            "GATE_FAILED: step=oracle detail=implausible prediction outcome={} next={next} last={last_secs}",
            predicted.payload.outcome
        )
        .into());
    }
    let rejected = forecast_crash_recurrence(
        &mut oracle,
        &database,
        "fc-rare",
        "sig-rare",
        &crash_series("sig-rare", 5_000, 7_200, 3),
        90_000,
    )?;
    if rejected.payload.outcome != "rejected"
        || rejected.payload.rejection_reason.as_deref() != Some("InsufficientHistory")
    {
        return Err(format!(
            "GATE_FAILED: step=oracle-reject detail=expected InsufficientHistory, got {} {:?}",
            rejected.payload.outcome, rejected.payload.rejection_reason
        )
        .into());
    }
    println!(
        "ORACLE predicted_next_secs={next} support={} rejected_support={} rejected_reason={:?}",
        predicted.payload.support, rejected.payload.support, rejected.payload.rejection_reason
    );

    // ---- 5. Edge-case audit (fail-loud, state unchanged) -------------------
    let edges = edge_case_audit(&database, &replay_a)?;

    // ---- 6. FULL STATE VERIFICATION: reopen the vault, read every row -----
    drop(database);
    let reopened = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let batch_back = read_replay_match_batch(&reopened, BATCH_ID)?;
    let assay_back = read_replay_balance_assay(&reopened, BATCH_ID)?;
    let diverged_back = read_desync_forensics(&reopened, "case-corrupted-input")?;
    let clean_back = read_desync_forensics(&reopened, "case-clean")?;
    let predicted_back = read_crash_recurrence(&reopened, "fc-gpu-hang")?;
    let rejected_back = read_crash_recurrence(&reopened, "fc-rare")?;
    for (label, before, after) in [
        ("batch", &batch.database.sha256, &batch_back.database.sha256),
        ("assay", &assay.database.sha256, &assay_back.database.sha256),
        (
            "desync_diverged",
            &diverged.database.sha256,
            &diverged_back.database.sha256,
        ),
        (
            "desync_clean",
            &clean.database.sha256,
            &clean_back.database.sha256,
        ),
        (
            "oracle_predicted",
            &predicted.database.sha256,
            &predicted_back.database.sha256,
        ),
        (
            "oracle_rejected",
            &rejected.database.sha256,
            &rejected_back.database.sha256,
        ),
    ] {
        if before != after {
            return Err(format!(
                "GATE_FAILED: step=reopen-readback detail={label} sha256 drifted {before} -> {after}"
            )
            .into());
        }
    }

    let report = json!({
        "schema": "litd.engine.issue907.replay_analytics_fsv.v1",
        "issue": 907,
        "source_of_truth": {
            "engine_vault_dir": vault_dir.display().to_string(),
            "oracle_vault_dir": oracle_dir.display().to_string(),
            "report_path": run.report_path().display().to_string(),
            "note": "All six analytics rows were re-read from a REOPENED vault; sha256 values matched the store-time readbacks."
        },
        "ingest": {
            "batch_id": BATCH_ID,
            "match_count": batch_back.payload.match_count,
            "win_count": batch_back.payload.win_count,
            "loss_count": batch_back.payload.loss_count,
            "stat_keys": batch_back.payload.stat_keys,
            "batch_row_sha256": batch_back.database.sha256,
            "first_match_row_sha256": batch_back.matches[0].database.sha256,
            "first_match_tick_events": batch_back.matches[0].payload.tick_events.len(),
            "first_match_replay_ron_sha256": batch_back.matches[0].payload.replay_ron_sha256
        },
        "balance_assay": {
            "anchor": assay_back.payload.anchor,
            "sample_count": assay_back.payload.sample_count,
            "row_sha256": assay_back.database.sha256,
            "ranked": assay_back.payload.ranked.iter().map(|report| json!({
                "stat_key": report.stat_key,
                "bits": report.bits,
                "ci": [report.ci_low, report.ci_high],
                "probe_accuracy": report.probe_accuracy
            })).collect::<Vec<_>>()
        },
        "desync_forensics": {
            "diverged": {
                "row_sha256": diverged_back.database.sha256,
                "verdict": "diverged",
                "first_divergent_tick": diverged_back.payload.first_divergent_tick,
                "divergent_order_ticks": diverged_back.payload.divergent_order_ticks,
                "hash_a": diverged_back.payload.divergent_tick_hash_a,
                "hash_b": diverged_back.payload.divergent_tick_hash_b
            },
            "clean": {
                "row_sha256": clean_back.database.sha256,
                "verdict": "clean",
                "compared_ticks": clean_back.payload.compared_ticks
            }
        },
        "crash_recurrence": {
            "predicted": {
                "row_sha256": predicted_back.database.sha256,
                "crash_signature": predicted_back.payload.crash_signature,
                "predicted_next_secs": predicted_back.payload.predicted_next_secs,
                "support": predicted_back.payload.support,
                "oracle_forecast_key_hex": predicted_back.payload.oracle_forecast_key_hex
            },
            "rejected": {
                "row_sha256": rejected_back.database.sha256,
                "rejection_reason": rejected_back.payload.rejection_reason,
                "support": rejected_back.payload.support
            }
        },
        "edge_cases": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "FSV_SOURCE_OF_TRUTH_READBACK report={} bytes={} batch_sha256={} assay_sha256={} desync_sha256={} oracle_sha256={}",
        run.report_path().display(),
        readback.bytes.len(),
        batch_back.database.sha256,
        assay_back.database.sha256,
        diverged_back.database.sha256,
        predicted_back.database.sha256
    );
    Ok(())
}

fn print_ranked(assay: &ReplayBalanceAssayReadback) {
    for report in &assay.payload.ranked {
        println!(
            "ASSAY stat={} bits={:.4} ci=[{:.4},{:.4}] accuracy={:.3}",
            report.stat_key, report.bits, report.ci_low, report.ci_high, report.probe_accuracy
        );
    }
}

/// The measured-balance gate: the two stats that actually drive the scripted
/// attack plan must carry strictly more predictive bits than every decoy.
fn verify_measured_balance(
    assay: &ReplayBalanceAssayReadback,
) -> Result<(), Box<dyn std::error::Error>> {
    let bits_of = |key: &str| -> Result<f32, Box<dyn std::error::Error>> {
        assay
            .payload
            .ranked
            .iter()
            .find(|report| report.stat_key == key)
            .map(|report| report.bits)
            .ok_or_else(|| format!("GATE_FAILED: step=assay detail=stat {key} missing").into())
    };
    let min_driver = DRIVER_STATS
        .iter()
        .map(|key| bits_of(key))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(f32::INFINITY, f32::min);
    let max_decoy = DECOY_STATS
        .iter()
        .map(|key| bits_of(key))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    if min_driver <= max_decoy || min_driver < 0.05 {
        return Err(format!(
            "GATE_FAILED: step=assay detail=measured balance did not separate drivers from decoys min_driver={min_driver} max_decoy={max_decoy}"
        )
        .into());
    }
    println!("ASSAY_GATE min_driver_bits={min_driver:.4} max_decoy_bits={max_decoy:.4}");
    Ok(())
}

/// Build peer B's genuinely divergent timeline: the first order's damage is
/// off by one (a corrupted input, the classic desync root cause), and B's
/// expected hash is honestly recomputed by executing B's own timeline.
fn corrupted_peer_replay(
    replay_a: &SimReplayRecord,
) -> Result<SimReplayRecord, Box<dyn std::error::Error>> {
    let mut replay_b = replay_a.clone();
    let first = replay_b
        .orders
        .first()
        .copied()
        .ok_or("GATE_FAILED: step=desync-setup detail=replay has no orders")?;
    let SimOrderKind::ApplyAttack {
        attacker,
        damage_fp,
    } = first.kind
    else {
        return Err("GATE_FAILED: step=desync-setup detail=first order is not an attack".into());
    };
    replay_b.orders[0] = SimOrder::new(
        first.execute_tick,
        first.source,
        first.sequence,
        first.target,
        SimOrderKind::ApplyAttack {
            attacker,
            damage_fp: damage_fp + 1,
        },
    );
    let outcome = replay_b.replay::<MB, MO>();
    // replay() fails loud on the (now wrong) expected hash — recompute it by
    // executing B's timeline directly.
    let mut sim = replay_b.initial_snapshot.restore::<MB>()?;
    let mut queue = litd_engine::sim_order::SimOrderQueue::<MO>::default();
    for order in &replay_b.orders {
        queue.enqueue(*order)?;
    }
    replay_b.expected_final_hash = queue.run_ticks(&mut sim, replay_b.run_ticks)?.state_hash;
    if outcome.is_ok() {
        return Err(
            "GATE_FAILED: step=desync-setup detail=tampered replay unexpectedly matched A's hash"
                .into(),
        );
    }
    Ok(replay_b)
}

/// Edge cases (#800 §3): each must fail loud and leave no partial row behind.
fn edge_case_audit(
    database: &EngineCalyxDatabase,
    replay_a: &SimReplayRecord,
) -> Result<Value, Box<dyn std::error::Error>> {
    // Edge 1: empty batch.
    let empty = store_replay_match_batch::<MB, MO>(database, "batch-empty", &[])
        .expect_err("empty batch must fail")
        .to_string();
    let empty_row_absent = read_replay_match_batch(database, "batch-empty").is_err();

    // Edge 2: ambiguous (tied) outcome.
    let mut stream = DeterministicStream::new(0xE0E0);
    let mut tie = build_match(999, &mut stream);
    tie.match_id = "match-tie".to_string();
    tie.replay = tied_replay(replay_a)?;
    let tie_error = store_replay_match_batch::<MB, MO>(database, "batch-tie", &[tie])
        .expect_err("tied outcome must fail")
        .to_string();
    let tie_row_absent = read_replay_match_batch(database, "batch-tie").is_err();

    // Edge 3: incomparable forensics inputs.
    let mut longer = replay_a.clone();
    longer.run_ticks += 1;
    let incomparable =
        run_desync_forensics::<MB, MO>(database, "case-incomparable", replay_a, &longer)
            .expect_err("incomparable records must fail")
            .to_string();
    let incomparable_row_absent = read_desync_forensics(database, "case-incomparable").is_err();

    // Edge 4: crash telemetry without a signature.
    let oracle_dir = litd_engine::test_temp::TestTempDir::new("issue907-edge-oracle");
    let mut oracle = EngineOracleService::open_with_clock(
        EngineOracleServiceConfig::for_vault(oracle_dir.path()),
        FixedClock::new(FSV_TS),
    )?;
    let unsigned = forecast_crash_recurrence(
        &mut oracle,
        database,
        "fc-unsigned",
        "sig-any",
        &[crash_event("crash-unsigned", None, 9_000)],
        10_000,
    )
    .expect_err("unsigned crash must fail")
    .to_string();
    let unsigned_row_absent = read_crash_recurrence(database, "fc-unsigned").is_err();

    for (name, message, absent) in [
        ("empty_batch", &empty, empty_row_absent),
        ("tied_outcome", &tie_error, tie_row_absent),
        (
            "incomparable_forensics",
            &incomparable,
            incomparable_row_absent,
        ),
        ("unsigned_crash", &unsigned, unsigned_row_absent),
    ] {
        if !absent {
            return Err(format!(
                "GATE_FAILED: step=edge-audit detail={name} left a partial row behind"
            )
            .into());
        }
        println!("EDGE {name} failed_loud_with: {message}");
    }
    Ok(json!({
        "empty_batch": { "error": empty, "row_absent_after_failure": empty_row_absent },
        "tied_outcome": { "error": tie_error, "row_absent_after_failure": tie_row_absent },
        "incomparable_forensics": { "error": incomparable, "row_absent_after_failure": incomparable_row_absent },
        "unsigned_crash": { "error": unsigned, "row_absent_after_failure": unsigned_row_absent }
    }))
}

/// A genuinely executed tied match: both sides land identical damage.
fn tied_replay(template: &SimReplayRecord) -> Result<SimReplayRecord, Box<dyn std::error::Error>> {
    let mut tied = template.clone();
    let subject = litd_engine::sim::SimBodyId::from_raw(0);
    let opponent = litd_engine::sim::SimBodyId::from_raw(1);
    tied.orders = support::match_orders(subject, opponent, 700, 3);
    let mut sim = tied.initial_snapshot.restore::<MB>()?;
    let mut queue = litd_engine::sim_order::SimOrderQueue::<MO>::default();
    for order in &tied.orders {
        queue.enqueue(*order)?;
    }
    tied.expected_final_hash = queue.run_ticks(&mut sim, tied.run_ticks)?.state_hash;
    Ok(tied)
}
