use crate::{
    EngineError,
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimConfig},
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

#[test]
fn fixed_tick_loop_moves_bodies_with_fixed_point_delta() {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    let body = sim
        .spawn_body(v(0, 0, 0), v(64, 0, -128), v(0, 0, 0))
        .unwrap();

    let report = sim.run_ticks(8).unwrap();
    let state = sim.body_state(body).unwrap();

    assert_eq!(report.start_tick, 0);
    assert_eq!(report.final_tick, 8);
    assert_eq!(report.steps, 8);
    assert_eq!(report.bodies_updated, 8);
    assert_eq!(state.position, v(8, 0, -16));
    assert_eq!(state.velocity, v(64, 0, -128));
}

#[test]
fn acceleration_and_run_until_are_deterministic() {
    let mut left = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 11).unwrap();
    let left_body = left
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(4096, 0, 0))
        .unwrap();
    let mut right = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 11).unwrap();
    let right_body = right
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(4096, 0, 0))
        .unwrap();

    let left_report = left
        .run_until(8, |sim| {
            sim.body_state(left_body).unwrap().position.x.raw() >= Fixed::from_i32(10).raw()
        })
        .unwrap();
    let right_report = right.run_ticks(4).unwrap();

    assert_eq!(left_report.final_tick, 4);
    assert_eq!(right_report.final_tick, 4);
    assert_eq!(
        left.body_state(left_body).unwrap().position.x,
        Fixed::from_i32(10)
    );
    assert_eq!(
        right.body_state(right_body).unwrap().position.x,
        Fixed::from_i32(10)
    );
    assert_eq!(left.state_hash(), right.state_hash());
}

#[test]
fn zero_tick_run_preserves_state() {
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 5).unwrap();
    sim.spawn_body(v(1, 2, 3), v(64, 64, 64), v(0, 0, 0))
        .unwrap();
    let before = sim.state_hash();

    let report = sim.run_ticks(0).unwrap();

    assert_eq!(report.start_tick, 0);
    assert_eq!(report.final_tick, 0);
    assert_eq!(report.steps, 0);
    assert_eq!(report.bodies_updated, 0);
    assert_eq!(report.state_hash, before);
    assert_eq!(sim.state_hash(), before);
}

#[test]
fn issue1463_body_slot_index_stays_correct_across_spawn_despawn_respawn() {
    // The O(1) body_slots index must return the right body after slot churn, and
    // freeing a slot must let a later spawn reuse it — with the sim's state_hash
    // matching a sim built the same way (index never affects determinism).
    let config = SimConfig::new(64).unwrap();
    let mut sim = FixedTickSim::<3>::new(config, 1463).unwrap();
    let a = sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let b = sim.spawn_body(v(2, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let c = sim.spawn_body(v(3, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();

    // Lookups resolve to the correct bodies.
    assert_eq!(sim.body_state(a).unwrap().position.x, Fixed::from_i32(1));
    assert_eq!(sim.body_state(b).unwrap().position.x, Fixed::from_i32(2));
    assert_eq!(sim.body_state(c).unwrap().position.x, Fixed::from_i32(3));

    // Despawn the middle body: its lookup fails loud, others still resolve.
    let removed = sim.despawn_body(b).unwrap();
    assert_eq!(removed.position.x, Fixed::from_i32(2));
    assert!(sim.body_state(b).is_none());
    assert!(matches!(
        sim.set_body_velocity(b, v(9, 0, 0)),
        Err(EngineError::SimBodyMissing { .. })
    ));
    assert_eq!(sim.body_state(a).unwrap().position.x, Fixed::from_i32(1));
    assert_eq!(sim.body_state(c).unwrap().position.x, Fixed::from_i32(3));

    // Respawn: a new id reuses the freed slot and is looked up correctly.
    let d = sim.spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    assert_ne!(d.index(), b.index());
    assert_eq!(sim.body_state(d).unwrap().position.x, Fixed::from_i32(4));
    sim.set_body_velocity(d, v(7, 0, 0)).unwrap();
    assert_eq!(sim.body_state(d).unwrap().velocity.x, Fixed::from_i32(7));

    // Determinism: an independent sim built with the same spawn/despawn/respawn
    // sequence produces the identical state_hash (index is lookup-only).
    let mut mirror = FixedTickSim::<3>::new(config, 1463).unwrap();
    mirror
        .spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    let mb = mirror
        .spawn_body(v(2, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    mirror
        .spawn_body(v(3, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    mirror.despawn_body(mb).unwrap();
    let md = mirror
        .spawn_body(v(4, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    mirror.set_body_velocity(md, v(7, 0, 0)).unwrap();
    assert_eq!(sim.state_hash(), mirror.state_hash());
}

#[test]
fn invalid_tick_rate_and_capacity_fail_loud() {
    match SimConfig::new(0) {
        Err(EngineError::SimInvalidConfig { field, reason }) => {
            assert_eq!(field, "ticks_per_second");
            assert!(reason.contains("non-zero"));
        }
        other => panic!("expected SimInvalidConfig, got {other:?}"),
    }

    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 9).unwrap();
    sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    match sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)) {
        Err(EngineError::SimCapacityExceeded { capacity }) => assert_eq!(capacity, 1),
        other => panic!("expected SimCapacityExceeded, got {other:?}"),
    }
}

#[test]
fn issue1295_large_capacity_lives_on_heap_and_runs() {
    let stack_bytes = std::mem::size_of::<FixedTickSim<32_768>>();
    assert!(stack_bytes < 256);
    let mut sim = FixedTickSim::<32_768>::new(SimConfig::new(64).unwrap(), 0x1295).unwrap();
    for index in 0..4096 {
        sim.spawn_body(v(index, 0, -(index % 64)), v(1, 0, -1), v(0, 0, 0))
            .unwrap();
    }
    let report = sim.run_ticks(2).unwrap();
    assert_eq!(sim.capacity(), 32_768);
    assert_eq!(sim.body_count(), 4096);
    assert_eq!(report.final_tick, 2);
    assert_eq!(report.bodies_updated, 8192);

    for _ in 0..(32_768 - 4096) {
        sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    }
    match sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)) {
        Err(EngineError::SimCapacityExceeded { capacity }) => assert_eq!(capacity, 32_768),
        other => panic!("expected SimCapacityExceeded, got {other:?}"),
    }
    println!(
        "issue1295 stack_bytes={} capacity={} body_count={} final_tick={} bodies_updated={} hash={:016X}",
        stack_bytes,
        sim.capacity(),
        sim.body_count(),
        report.final_tick,
        report.bodies_updated,
        report.state_hash
    );
}
