use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env::var("LITD_SIM_REPLAY_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1135-sim-replay-fsv.txt"));
    let snapshot_path = env::var("LITD_SIM_REPLAY_FSV_SNAPSHOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1135-sim-snapshot.ron"));
    let replay_path = env::var("LITD_SIM_REPLAY_FSV_RECORD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1135-sim-replay.ron"));

    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<2>::new(config, 0x1135)?;
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let initial_snapshot = SimSnapshot::capture(&sim);

    let orders = vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ),
        SimOrder::new(
            2,
            SimOrderSource::lowered_agent(2),
            1,
            body,
            SimOrderKind::AddVelocity(v(0, 128, 0)),
        ),
        SimOrder::new(
            3,
            SimOrderSource::lowered_agent(2),
            2,
            body,
            SimOrderKind::SetAcceleration(v(0, 0, -4096)),
        ),
    ];

    let mut queue = SimOrderQueue::<8>::default();
    for order in &orders {
        queue.enqueue(*order)?;
    }
    let expected_report = queue.run_ticks(&mut sim, 4)?;
    let expected_final_snapshot = SimSnapshot::capture(&sim);
    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: initial_snapshot.clone(),
        orders,
        run_ticks: 4,
        expected_final_hash: expected_report.state_hash,
    };

    write_text(&snapshot_path, &expected_final_snapshot.to_ron_string()?)?;
    write_text(&replay_path, &replay_record.to_ron_string()?)?;

    let loaded_snapshot = SimSnapshot::from_ron_str(&fs::read_to_string(&snapshot_path)?)?;
    let restored_final = loaded_snapshot.restore::<2>()?;
    let loaded_replay = SimReplayRecord::from_ron_str(&fs::read_to_string(&replay_path)?)?;
    let replay_outcome = loaded_replay.replay::<2, 8>()?;

    let mismatch_code = mismatch_probe(&loaded_replay);
    let invalid_schema_code = invalid_schema_probe(config);
    let replay_capacity_code = capacity_probe(body, initial_snapshot.clone());
    let malformed_code = match SimReplayRecord::from_ron_str("not a replay record") {
        Err(err @ EngineError::SerializationFailed { .. }) => err.code(),
        other => panic!("expected SerializationFailed, got {other:?}"),
    };

    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &stats_path,
        format!(
            "schema_version={}\n\
             snapshot_file={}\n\
             replay_file={}\n\
             snapshot_file_bytes={}\n\
             replay_file_bytes={}\n\
             initial_hash={:016X}\n\
             expected_final_hash={:016X}\n\
             restored_final_hash={:016X}\n\
             replay_final_hash={:016X}\n\
             replay_hash_match={}\n\
             final_tick={}\n\
             replay_steps={}\n\
             replay_orders_applied={}\n\
             final_position_value={:.3},{:.3},{:.3}\n\
             final_velocity_value={:.3},{:.3},{:.3}\n\
             mismatch_code={}\n\
             invalid_schema_code={}\n\
             replay_capacity_code={}\n\
             malformed_ron_code={}\n\
             render_touched=false\n",
            SIM_REPLAY_SCHEMA_VERSION,
            snapshot_path.display(),
            replay_path.display(),
            fs::metadata(&snapshot_path)?.len(),
            fs::metadata(&replay_path)?.len(),
            initial_snapshot.state_hash,
            expected_report.state_hash,
            restored_final.state_hash(),
            replay_outcome.final_hash,
            restored_final.state_hash() == replay_outcome.final_hash
                && replay_outcome.final_hash == expected_report.state_hash,
            replay_outcome.final_tick,
            replay_outcome.steps,
            replay_outcome.orders_applied,
            loaded_snapshot.bodies[0].position.x.to_f64(),
            loaded_snapshot.bodies[0].position.y.to_f64(),
            loaded_snapshot.bodies[0].position.z.to_f64(),
            loaded_snapshot.bodies[0].velocity.x.to_f64(),
            loaded_snapshot.bodies[0].velocity.y.to_f64(),
            loaded_snapshot.bodies[0].velocity.z.to_f64(),
            mismatch_code,
            invalid_schema_code,
            replay_capacity_code,
            malformed_code,
        ),
    )?;
    println!("wrote sim replay FSV stats to {}", stats_path.display());
    Ok(())
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn mismatch_probe(record: &SimReplayRecord) -> &'static str {
    let mut bad = record.clone();
    bad.expected_final_hash = 0;
    match bad.replay::<2, 8>() {
        Err(err @ EngineError::SerializedFingerprintMismatch { .. }) => err.code(),
        other => panic!("expected SerializedFingerprintMismatch, got {other:?}"),
    }
}

fn invalid_schema_probe(config: SimConfig) -> &'static str {
    let snapshot = SimSnapshot {
        schema_version: 999,
        config,
        tick: 0,
        next_body_id: 0,
        rng_state: 1,
        bodies: Vec::new(),
        attack_events: Vec::new(),
        attack_events_digest: 0,
        attack_events_total: 0,
        state_hash: 0,
    };
    match snapshot.restore::<1>() {
        Err(err @ EngineError::SerializedTypeMismatch { .. }) => err.code(),
        Ok(_) => panic!("expected SerializedTypeMismatch, got Ok"),
        Err(other) => panic!("expected SerializedTypeMismatch, got {}", other.code()),
    }
}

fn capacity_probe(body: litd_engine::sim::SimBodyId, snapshot: SimSnapshot) -> &'static str {
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: snapshot,
        orders: vec![
            SimOrder::new(
                1,
                SimOrderSource::human(1),
                1,
                body,
                SimOrderKind::AddVelocity(v(1, 0, 0)),
            ),
            SimOrder::new(
                1,
                SimOrderSource::human(1),
                2,
                body,
                SimOrderKind::AddVelocity(v(1, 0, 0)),
            ),
        ],
        run_ticks: 1,
        expected_final_hash: 0,
    };
    match record.replay::<2, 1>() {
        Err(err @ EngineError::SimOrderCapacityExceeded { .. }) => err.code(),
        other => panic!("expected SimOrderCapacityExceeded, got {other:?}"),
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
