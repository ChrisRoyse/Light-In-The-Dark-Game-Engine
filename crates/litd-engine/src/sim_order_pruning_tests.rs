//! #1463 supersedes #1340: a past-due sim order (one whose `execute_tick`
//! already elapsed) is a scheduler bug, not something to silently prune. Under
//! lockstep netplay, dropping a late order on one peer while another applied it
//! on time diverges their state hashes, so `SimOrderQueue::step` now fails loud
//! with a structured `EngineError::SimOrderPastDue` and mutates nothing.

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimBodyState, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
};
use serde_json::{Value, json};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn body_state_json(state: SimBodyState) -> Value {
    json!({
        "id": state.id.index(),
        "position_raw": [state.position.x.raw(), state.position.y.raw(), state.position.z.raw()],
        "velocity_raw": [state.velocity.x.raw(), state.velocity.y.raw(), state.velocity.z.raw()],
        "acceleration_raw": [
            state.acceleration.x.raw(),
            state.acceleration.y.raw(),
            state.acceleration.z.raw()
        ],
    })
}

fn queue_snapshot<const MAX_ORDERS: usize, const MAX_BODIES: usize>(
    label: &str,
    queue: &SimOrderQueue<MAX_ORDERS>,
    sim: &FixedTickSim<MAX_BODIES>,
    body: SimBodyId,
) -> Value {
    json!({
        "label": label,
        "tick": sim.tick(),
        "pending_orders": queue.pending_orders(),
        "pending_order_ticks": queue.pending_order_ticks(),
        "pending_order_fingerprints": queue.pending_order_fingerprints(),
        "applied_total": queue.applied_total(),
        "state_hash": sim.state_hash(),
        "body": body_state_json(sim.body_state(body).unwrap()),
    })
}

fn order(
    execute_tick: u64,
    source: SimOrderSource,
    sequence: u64,
    target: SimBodyId,
    kind: SimOrderKind,
) -> SimOrder {
    SimOrder::new(execute_tick, source, sequence, target, kind)
}

fn past_due_details(result: Result<crate::sim_order::SimOrderStepReport, EngineError>) -> Value {
    match result {
        Ok(report) => panic!("expected SimOrderPastDue, got Ok({report:?})"),
        Err(EngineError::SimOrderPastDue {
            execute_tick,
            earliest_execute_tick,
            orders_past_due,
        }) => json!({
            "code": "SimOrderPastDue",
            "execute_tick": execute_tick,
            "earliest_execute_tick": earliest_execute_tick,
            "orders_past_due": orders_past_due,
        }),
        Err(other) => panic!("expected SimOrderPastDue, got {other:?}"),
    }
}

#[test]
fn issue1463_past_due_orders_fail_loud_and_mutate_nothing() {
    let config = SimConfig::new(64).unwrap();
    let mut sim = FixedTickSim::<1>::new(config, 1463).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    sim.run_ticks(8).unwrap();

    // A stale order for tick 3 alongside legitimately-future orders (ticks 9, 11).
    let mut queue = SimOrderQueue::<4>::default();
    queue
        .enqueue(order(
            3,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(999, 0, 0)),
        ))
        .unwrap();
    queue
        .enqueue(order(
            9,
            SimOrderSource::human(1),
            2,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ))
        .unwrap();
    queue
        .enqueue(order(
            11,
            SimOrderSource::lowered_agent(7),
            1,
            body,
            SimOrderKind::AddVelocity(v(0, 64, 0)),
        ))
        .unwrap();

    let before = queue_snapshot("past_due_before", &queue, &sim, body);
    let tick_before = sim.tick();
    let hash_before = sim.state_hash();
    let pending_before = queue.pending_orders();

    // step() must fail loud: next execute_tick is 9, order for tick 3 is past-due.
    let details = past_due_details(queue.step(&mut sim));

    let after = queue_snapshot("past_due_after", &queue, &sim, body);

    // Fail-loud contract: nothing mutated. Same tick, same hash, same pending set
    // (the stale order was NOT silently dropped — the caller still holds it).
    assert_eq!(details["execute_tick"], 9);
    assert_eq!(details["earliest_execute_tick"], 3);
    assert_eq!(details["orders_past_due"], 1);
    assert_eq!(sim.tick(), tick_before);
    assert_eq!(sim.state_hash(), hash_before);
    assert_eq!(queue.pending_orders(), pending_before);
    assert_eq!(queue.applied_total(), 0);
    assert_eq!(queue.pending_order_ticks(), vec![3, 9, 11]);

    // Edge cases.
    let empty = empty_queue_edge(config);
    let future = all_future_edge(config);
    let duplicate = duplicate_execute_tick_edge(config);
    let boundary = exact_boundary_edge(config);

    let report = json!({
        "schema_version": 2,
        "issue": 1463,
        "supersedes": 1340,
        "status": "passed",
        "source_of_truth": "SimOrderQueue + FixedTickSim memory readback plus this physical JSON report",
        "past_due": {
            "expected": {
                "structured_error": "SimOrderPastDue",
                "sim_mutated": false,
                "stale_order_dropped": false
            },
            "before": before,
            "after": after,
            "error": details
        },
        "edge_cases": {
            "empty_queue": empty,
            "all_future_orders": future,
            "duplicate_execute_tick": duplicate,
            "exact_boundary_not_past_due": boundary
        }
    });
    let run = begin_issue_run("issue1463_sim_order_past_due").unwrap();
    let report_path = run.report_path().to_path_buf();
    let report_readback = run.write_report(&report).unwrap();
    let report_json = report_readback.json;
    assert_eq!(report_json["status"], "passed");
    assert_eq!(report_json["past_due"]["error"]["orders_past_due"], 1);
    assert_eq!(report_json["past_due"]["after"]["tick"], tick_before);
    println!(
        "ISSUE1463_FSV_REPORT_READBACK path={} bytes={} sha256={} hash_before={} hash_after={}",
        report_path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        hash_before,
        sim.state_hash()
    );
}

/// Empty queue: no orders, no past-due, step advances cleanly.
fn empty_queue_edge(config: SimConfig) -> Value {
    let mut sim = FixedTickSim::<1>::new(config, 1464).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let mut queue = SimOrderQueue::<4>::default();
    let before = queue_snapshot("empty_before", &queue, &sim, body);
    let report = queue.step(&mut sim).expect("empty queue steps cleanly");
    let after = queue_snapshot("empty_after", &queue, &sim, body);
    assert_eq!(report.orders_applied, 0);
    assert_eq!(sim.tick(), 1);
    json!({ "before": before, "after": after, "orders_applied": report.orders_applied })
}

/// All-future orders: nothing is due yet, no past-due error, orders retained.
fn all_future_edge(config: SimConfig) -> Value {
    let mut sim = FixedTickSim::<1>::new(config, 1465).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let mut queue = SimOrderQueue::<4>::default();
    queue
        .enqueue(order(
            8,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ))
        .unwrap();
    let before = queue_snapshot("future_before", &queue, &sim, body);
    let report = queue
        .step(&mut sim)
        .expect("future orders do not fail loud");
    let after = queue_snapshot("future_after", &queue, &sim, body);
    assert_eq!(report.orders_applied, 0);
    assert_eq!(queue.pending_order_ticks(), vec![8]);
    json!({ "before": before, "after": after, "orders_applied": report.orders_applied })
}

/// Duplicate execute_tick at the current boundary: both apply, no past-due.
fn duplicate_execute_tick_edge(config: SimConfig) -> Value {
    let mut sim = FixedTickSim::<1>::new(config, 1466).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let mut queue = SimOrderQueue::<4>::default();
    queue
        .enqueue(order(
            1,
            SimOrderSource::human(1),
            2,
            body,
            SimOrderKind::AddVelocity(v(0, 64, 0)),
        ))
        .unwrap();
    queue
        .enqueue(order(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ))
        .unwrap();
    let before = queue_snapshot("duplicate_before", &queue, &sim, body);
    let report = queue.step(&mut sim).expect("current-tick orders apply");
    let after = queue_snapshot("duplicate_after", &queue, &sim, body);
    assert_eq!(report.orders_applied, 2);
    assert_eq!(queue.pending_orders(), 0);
    assert_eq!(sim.body_state(body).unwrap().velocity, v(64, 64, 0));
    json!({ "before": before, "after": after, "orders_applied": report.orders_applied })
}

/// Boundary: an order for exactly the next execute_tick is NOT past-due.
fn exact_boundary_edge(config: SimConfig) -> Value {
    let mut sim = FixedTickSim::<1>::new(config, 1467).unwrap();
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    sim.run_ticks(5).unwrap();
    let mut queue = SimOrderQueue::<2>::default();
    // sim.tick() == 5, next execute_tick == 6; an order for 6 is exactly current.
    queue
        .enqueue(order(
            6,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(v(64, 0, 0)),
        ))
        .unwrap();
    let before = queue_snapshot("boundary_before", &queue, &sim, body);
    let report = queue
        .step(&mut sim)
        .expect("exact-boundary order is not past-due");
    let after = queue_snapshot("boundary_after", &queue, &sim, body);
    assert_eq!(report.orders_applied, 1);
    assert_eq!(sim.tick(), 6);
    json!({ "before": before, "after": after, "orders_applied": report.orders_applied })
}
