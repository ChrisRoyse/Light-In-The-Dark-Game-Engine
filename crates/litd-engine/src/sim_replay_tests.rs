use crate::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

#[test]
fn snapshot_ron_roundtrip_restores_identical_hash() {
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 55).unwrap();
    sim.spawn_body(v(1, 2, 3), v(64, 0, 0), v(0, 0, 0)).unwrap();
    sim.run_ticks(2).unwrap();
    let snapshot = SimSnapshot::capture(&sim);
    let text = snapshot.to_ron_string().unwrap();
    let loaded = SimSnapshot::from_ron_str(&text).unwrap();
    let restored = loaded.restore::<2>().unwrap();

    assert_eq!(loaded.state_hash, snapshot.state_hash);
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert!(text.contains("state_hash"));
}

#[test]
fn replay_record_reproduces_final_hash() {
    let config = SimConfig::new(64).unwrap();
    let mut sim = FixedTickSim::<1>::new(config, 99).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let initial = SimSnapshot::capture(&sim);
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
            SimOrderKind::AddVelocity(v(0, 64, 0)),
        ),
    ];
    let mut queue = SimOrderQueue::<4>::default();
    for order in &orders {
        queue.enqueue(*order).unwrap();
    }
    let expected = queue.run_ticks(&mut sim, 2).unwrap().state_hash;
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: initial,
        orders,
        run_ticks: 2,
        expected_final_hash: expected,
    };

    let loaded = SimReplayRecord::from_ron_str(&record.to_ron_string().unwrap()).unwrap();
    let outcome = loaded.replay::<1, 4>().unwrap();

    assert_eq!(outcome.final_hash, expected);
    assert_eq!(outcome.final_tick, 2);
    assert_eq!(outcome.orders_applied, 2);
}

#[test]
fn replay_hash_mismatch_fails_loud() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: SimSnapshot::capture(&sim),
        orders: vec![SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        )],
        run_ticks: 1,
        expected_final_hash: 0,
    };

    match record.replay::<1, 2>() {
        Err(EngineError::SerializedFingerprintMismatch { expected, actual }) => {
            assert_eq!(expected, "0000000000000000");
            assert_ne!(actual, expected);
        }
        other => panic!("expected SerializedFingerprintMismatch, got {other:?}"),
    }
}

#[test]
fn invalid_schema_and_replay_capacity_fail_loud() {
    let snapshot = SimSnapshot {
        schema_version: 999,
        config: SimConfig::new(64).unwrap(),
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
        Err(EngineError::SerializedTypeMismatch { expected, actual }) => {
            assert!(expected.contains("SimSnapshot/v3"));
            assert!(actual.contains("SimSnapshot/v999"));
        }
        Ok(_) => panic!("expected SerializedTypeMismatch, got Ok"),
        Err(other) => panic!("expected SerializedTypeMismatch, got {}", other.code()),
    }

    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: SimSnapshot::capture(&sim),
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
                SimBodyId::from_raw(body.index()),
                SimOrderKind::AddVelocity(v(1, 0, 0)),
            ),
        ],
        run_ticks: 1,
        expected_final_hash: sim.state_hash(),
    };
    match record.replay::<1, 1>() {
        Err(EngineError::SimOrderCapacityExceeded { capacity }) => assert_eq!(capacity, 1),
        other => panic!("expected SimOrderCapacityExceeded, got {other:?}"),
    }
}
