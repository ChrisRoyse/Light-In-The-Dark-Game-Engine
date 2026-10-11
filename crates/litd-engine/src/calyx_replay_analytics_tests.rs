//! Unit tests for the #907 replay & telemetry analytics loop.

use std::collections::BTreeMap;

use calyx_core::FixedClock;

use crate::{
    calyx_observability::{EngineTelemetryEvent, EngineTelemetryEventKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_replay_analytics::{
        EngineReplayAnalyticsError, ReplayMatchOutcome, ReplayMatchRecord, read_replay_match_batch,
        store_replay_match_batch,
    },
    calyx_replay_analytics_assay::measure_replay_balance_assay,
    calyx_replay_analytics_desync::{DesyncVerdict, run_desync_forensics},
    calyx_replay_analytics_oracle::forecast_crash_recurrence,
    live_world_forecast::{EngineOracleService, EngineOracleServiceConfig},
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
    test_temp::TestTempDir,
};

const MB: usize = 2;
const MO: usize = 8;
const RUN_TICKS: u64 = 4;

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn temp_db(label: &str) -> (TestTempDir, EngineCalyxDatabase) {
    let dir = TestTempDir::new(&format!("issue907-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(dir.path())).unwrap();
    (dir, database)
}

/// Build one REAL executed match: two bodies trade scripted attacks; the
/// expected final hash comes from an actual run, so the record replays clean.
fn build_match(
    match_id: &str,
    seed: u64,
    subject_damage_fp: i64,
    opponent_damage_fp: i64,
) -> ReplayMatchRecord {
    let config = SimConfig::new(32).unwrap();
    let mut sim = FixedTickSim::<MB>::new(config, seed).unwrap();
    let subject = sim.spawn_body(v(0, 0, 0), v(1, 0, 0), v(0, 0, 0)).unwrap();
    let opponent = sim.spawn_body(v(4, 0, 0), v(-1, 0, 0), v(0, 0, 0)).unwrap();
    let initial = SimSnapshot::capture(&sim);
    let orders = match_orders(subject, opponent, subject_damage_fp, opponent_damage_fp);
    let mut queue = SimOrderQueue::<MO>::default();
    for order in &orders {
        queue.enqueue(*order).unwrap();
    }
    let expected = queue.run_ticks(&mut sim, RUN_TICKS).unwrap().state_hash;
    let mut stats_milli = BTreeMap::new();
    stats_milli.insert("damage_milli".to_string(), subject_damage_fp);
    stats_milli.insert("decoy_milli".to_string(), 500);
    ReplayMatchRecord {
        match_id: match_id.to_string(),
        replay: SimReplayRecord {
            schema_version: SIM_REPLAY_SCHEMA_VERSION,
            initial_snapshot: initial,
            orders,
            run_ticks: RUN_TICKS,
            expected_final_hash: expected,
        },
        subject_bodies: vec![subject.index()],
        opponent_bodies: vec![opponent.index()],
        stats_milli,
    }
}

fn match_orders(
    subject: SimBodyId,
    opponent: SimBodyId,
    subject_damage_fp: i64,
    opponent_damage_fp: i64,
) -> Vec<SimOrder> {
    vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            opponent,
            SimOrderKind::ApplyAttack {
                attacker: subject,
                damage_fp: subject_damage_fp,
            },
        ),
        SimOrder::new(
            2,
            SimOrderSource::human(2),
            2,
            subject,
            SimOrderKind::ApplyAttack {
                attacker: opponent,
                damage_fp: opponent_damage_fp,
            },
        ),
    ]
}

#[test]
fn batch_roundtrip_derives_outcomes_from_executed_replays() {
    let (_dir, database) = temp_db("roundtrip");
    let matches = vec![
        build_match("m-win", 11, 900, 300),
        build_match("m-loss", 12, 200, 700),
    ];
    let stored = store_replay_match_batch::<MB, MO>(&database, "batch-rt", &matches).unwrap();
    assert_eq!(stored.payload.win_count, 1);
    assert_eq!(stored.payload.loss_count, 1);

    let readback = read_replay_match_batch(&database, "batch-rt").unwrap();
    assert_eq!(readback.payload, stored.payload);
    let win = &readback.matches[0];
    assert_eq!(win.payload.outcome, ReplayMatchOutcome::SubjectWin);
    assert_eq!(win.payload.subject_damage_fp, 900);
    assert_eq!(win.payload.opponent_damage_fp, 300);
    assert_eq!(win.payload.tick_events.len(), 2);
    assert_eq!(win.payload.tick_events[0].tick, 1);
    assert_eq!(win.payload.tick_events[1].tick, 2);
    assert_eq!(
        readback.matches[1].payload.outcome,
        ReplayMatchOutcome::SubjectLoss
    );
}

#[test]
fn empty_batch_fails_loud() {
    let (_dir, database) = temp_db("empty");
    let error = store_replay_match_batch::<MB, MO>(&database, "batch-empty", &[]).unwrap_err();
    assert!(matches!(
        error,
        EngineReplayAnalyticsError::InvalidInput { ref field, .. } if field == "matches"
    ));
}

#[test]
fn tie_outcome_fails_loud() {
    let (_dir, database) = temp_db("tie");
    let matches = vec![build_match("m-tie", 13, 400, 400)];
    let error = store_replay_match_batch::<MB, MO>(&database, "batch-tie", &matches).unwrap_err();
    match error {
        EngineReplayAnalyticsError::Outcome { match_id, reason } => {
            assert_eq!(match_id, "m-tie");
            assert!(reason.contains("ambiguous outcome"), "reason: {reason}");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn mismatched_stat_roster_fails_loud() {
    let (_dir, database) = temp_db("roster");
    let mut matches = vec![
        build_match("m-a", 14, 900, 300),
        build_match("m-b", 15, 200, 700),
    ];
    matches[1].stats_milli.remove("decoy_milli");
    let error =
        store_replay_match_batch::<MB, MO>(&database, "batch-roster", &matches).unwrap_err();
    assert!(matches!(
        error,
        EngineReplayAnalyticsError::InvalidInput { ref field, .. } if field == "stats_milli"
    ));
}

#[test]
fn assay_single_class_fails_loud() {
    let (_dir, database) = temp_db("single-class");
    let matches = vec![
        build_match("m-1", 16, 900, 300),
        build_match("m-2", 17, 800, 300),
    ];
    store_replay_match_batch::<MB, MO>(&database, "batch-one-class", &matches).unwrap();
    let error = measure_replay_balance_assay(&database, "batch-one-class").unwrap_err();
    match error {
        EngineReplayAnalyticsError::Assay(reason) => {
            assert!(reason.contains("both outcome classes"), "reason: {reason}");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn assay_below_min_samples_fails_loud() {
    let (_dir, database) = temp_db("min-samples");
    let matches = vec![
        build_match("m-1", 18, 900, 300),
        build_match("m-2", 19, 200, 700),
    ];
    store_replay_match_batch::<MB, MO>(&database, "batch-small", &matches).unwrap();
    let error = measure_replay_balance_assay(&database, "batch-small").unwrap_err();
    match error {
        EngineReplayAnalyticsError::Assay(reason) => {
            assert!(reason.contains("probe failed"), "reason: {reason}");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn desync_forensics_pinpoints_first_divergent_tick() {
    let (_dir, database) = temp_db("desync");
    let base = build_match("m-desync", 20, 900, 300);
    let replay_a = base.replay.clone();
    // Peer B saw a corrupted order: same tick, one damage point different.
    // Re-execute B's timeline for its own honest expected hash.
    let mut replay_b = replay_a.clone();
    replay_b.orders[1] = SimOrder::new(
        2,
        SimOrderSource::human(2),
        2,
        SimBodyId::from_raw(0),
        SimOrderKind::ApplyAttack {
            attacker: SimBodyId::from_raw(1),
            damage_fp: 301,
        },
    );
    let mut sim_b = replay_b.initial_snapshot.restore::<MB>().unwrap();
    let mut queue_b = SimOrderQueue::<MO>::default();
    for order in &replay_b.orders {
        queue_b.enqueue(*order).unwrap();
    }
    replay_b.expected_final_hash = queue_b.run_ticks(&mut sim_b, RUN_TICKS).unwrap().state_hash;

    let readback =
        run_desync_forensics::<MB, MO>(&database, "case-diverged", &replay_a, &replay_b).unwrap();
    assert_eq!(readback.payload.verdict, DesyncVerdict::Diverged);
    assert_eq!(readback.payload.first_divergent_tick, Some(2));
    assert!(readback.payload.order_log_divergence);
    assert_eq!(readback.payload.divergent_order_ticks, vec![2]);
}

#[test]
fn desync_forensics_clean_for_identical_replays() {
    let (_dir, database) = temp_db("desync-clean");
    let base = build_match("m-clean", 21, 900, 300);
    let readback =
        run_desync_forensics::<MB, MO>(&database, "case-clean", &base.replay, &base.replay)
            .unwrap();
    assert_eq!(readback.payload.verdict, DesyncVerdict::Clean);
    assert_eq!(readback.payload.first_divergent_tick, None);
    assert_eq!(readback.payload.compared_ticks, RUN_TICKS);
    assert!(!readback.payload.order_log_divergence);
}

#[test]
fn desync_forensics_rejects_incomparable_records() {
    let (_dir, database) = temp_db("desync-bad");
    let base = build_match("m-bad", 22, 900, 300);
    let mut other = base.replay.clone();
    other.run_ticks = RUN_TICKS + 1;
    let error =
        run_desync_forensics::<MB, MO>(&database, "case-bad", &base.replay, &other).unwrap_err();
    assert!(matches!(
        error,
        EngineReplayAnalyticsError::Forensics { ref reason, .. }
            if reason.contains("not comparable")
    ));
}

#[test]
fn crash_recurrence_rejects_missing_signature_and_no_events() {
    let (_dir, database) = temp_db("oracle-edge");
    let oracle_dir = TestTempDir::new("issue907-oracle-edge-vault");
    let mut oracle = EngineOracleService::open_with_clock(
        EngineOracleServiceConfig::for_vault(oracle_dir.path()),
        FixedClock::new(1_785_599_198),
    )
    .unwrap();

    let unsigned_crash = EngineTelemetryEvent {
        event_id: "evt-1".to_string(),
        source: "game".to_string(),
        kind: EngineTelemetryEventKind::Crash,
        observed_at_ms: 1_000,
        latency_ms: None,
        error_code: None,
        chain_ok: None,
        crash_signature: None,
        agent_ood_score_milli: None,
        frame_time_ns: None,
        endpoint_ok: None,
        provenance_refs: Vec::new(),
    };
    let error = forecast_crash_recurrence(
        &mut oracle,
        &database,
        "fc-unsigned",
        "sig-gpu-hang",
        std::slice::from_ref(&unsigned_crash),
        10_000,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        EngineReplayAnalyticsError::Oracle { ref reason, .. }
            if reason.contains("no crash_signature")
    ));

    let error = forecast_crash_recurrence(
        &mut oracle,
        &database,
        "fc-none",
        "sig-gpu-hang",
        &[],
        10_000,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        EngineReplayAnalyticsError::Oracle { ref reason, .. }
            if reason.contains("no crash telemetry events match")
    ));
}
