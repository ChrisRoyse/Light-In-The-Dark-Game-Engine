use crate::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

#[test]
fn order_execution_is_stable_independent_of_enqueue_order() {
    let config = SimConfig::new(64).unwrap();
    let mut first = FixedTickSim::<1>::new(config, 1).unwrap();
    let first_body = first
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    let mut second = FixedTickSim::<1>::new(config, 1).unwrap();
    let second_body = second
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();

    let earlier = SimOrder::new(
        1,
        SimOrderSource::human(7),
        1,
        first_body,
        SimOrderKind::SetVelocity(v(128, 0, 0)),
    );
    let later = SimOrder::new(
        1,
        SimOrderSource::human(7),
        2,
        first_body,
        SimOrderKind::SetVelocity(v(64, 0, 0)),
    );

    let mut first_queue = SimOrderQueue::<4>::default();
    first_queue.enqueue(later).unwrap();
    first_queue.enqueue(earlier).unwrap();

    let mut second_queue = SimOrderQueue::<4>::default();
    second_queue
        .enqueue(SimOrder {
            target: second_body,
            ..earlier
        })
        .unwrap();
    second_queue
        .enqueue(SimOrder {
            target: second_body,
            ..later
        })
        .unwrap();

    first_queue.step(&mut first).unwrap();
    second_queue.step(&mut second).unwrap();

    assert_eq!(first.body_state(first_body).unwrap().position, v(1, 0, 0));
    assert_eq!(second.body_state(second_body).unwrap().position, v(1, 0, 0));
    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn lowered_agent_order_composes_with_human_order() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 2).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let mut queue = SimOrderQueue::<4>::default();

    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::lowered_agent(3),
            1,
            body,
            SimOrderKind::SetAcceleration(v(0, 4096, 0)),
        ))
        .unwrap();
    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ))
        .unwrap();

    let report = queue.step(&mut sim).unwrap();
    let state = sim.body_state(body).unwrap();

    assert_eq!(report.orders_applied, 2);
    assert_eq!(report.pending_orders, 0);
    assert_eq!(state.position, v(1, 1, 0));
    assert_eq!(state.velocity, v(64, 64, 0));
}

#[test]
fn order_fingerprints_distinguish_sources_and_payloads() {
    let target = SimBodyId::from_raw(5);
    let human = SimOrder::new(
        4,
        SimOrderSource::human(1),
        2,
        target,
        SimOrderKind::AddVelocity(v(1, 0, 0)),
    );
    let agent = SimOrder::new(
        4,
        SimOrderSource::lowered_agent(1),
        2,
        target,
        SimOrderKind::AddVelocity(v(1, 0, 0)),
    );

    assert_ne!(human.fingerprint(), agent.fingerprint());
    assert_eq!(human.fingerprint(), human.fingerprint());
    assert_ne!(
        human.fingerprint(),
        SimOrder::new(
            4,
            SimOrderSource::lowered_agent(1),
            2,
            target,
            SimOrderKind::ApplyAttack {
                attacker: SimBodyId::from_raw(4),
                damage_fp: 10,
            },
        )
        .fingerprint()
    );
}

#[test]
fn attack_order_records_hashed_sim_event() {
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 4).unwrap();
    let attacker = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let target = sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let before_hash = sim.state_hash();
    let mut queue = SimOrderQueue::<1>::default();
    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::lowered_agent(9),
            1,
            target,
            SimOrderKind::ApplyAttack {
                attacker,
                damage_fp: 25,
            },
        ))
        .unwrap();

    let report = queue.step(&mut sim).unwrap();
    let events = sim.attack_events().collect::<Vec<_>>();

    assert_eq!(report.orders_applied, 1);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tick, 1);
    assert_eq!(events[0].attacker, attacker);
    assert_eq!(events[0].target, target);
    assert_eq!(events[0].damage_fp, 25);
    assert_ne!(before_hash, sim.state_hash());
}

#[test]
fn capacity_and_missing_body_fail_loud() {
    let mut queue = SimOrderQueue::<1>::default();
    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            SimBodyId::from_raw(0),
            SimOrderKind::AddVelocity(v(1, 0, 0)),
        ))
        .unwrap();
    match queue.enqueue(SimOrder::new(
        1,
        SimOrderSource::human(1),
        2,
        SimBodyId::from_raw(0),
        SimOrderKind::AddVelocity(v(1, 0, 0)),
    )) {
        Err(EngineError::SimOrderCapacityExceeded { capacity }) => assert_eq!(capacity, 1),
        other => panic!("expected SimOrderCapacityExceeded, got {other:?}"),
    }

    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 3).unwrap();
    let mut missing = SimOrderQueue::<1>::default();
    missing
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            SimBodyId::from_raw(99),
            SimOrderKind::AddVelocity(v(1, 0, 0)),
        ))
        .unwrap();
    match missing.step(&mut sim) {
        Err(EngineError::SimBodyMissing { id }) => assert_eq!(id, 99),
        other => panic!("expected SimBodyMissing, got {other:?}"),
    }
}
