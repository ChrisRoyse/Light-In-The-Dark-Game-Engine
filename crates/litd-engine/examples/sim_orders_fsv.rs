use std::{
    alloc::{GlobalAlloc, Layout, System},
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use litd_engine::{
    EngineError,
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
};

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_SIM_ORDERS_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1134-sim-orders-fsv.txt"));

    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<2>::new(config, 0x1134)?;
    let body = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let mut queue = SimOrderQueue::<8>::default();

    let first_fingerprint = queue.enqueue(SimOrder::new(
        1,
        SimOrderSource::lowered_agent(2),
        1,
        body,
        SimOrderKind::SetAcceleration(v(0, 4096, 0)),
    ))?;
    queue.enqueue(SimOrder::new(
        1,
        SimOrderSource::human(1),
        1,
        body,
        SimOrderKind::SetVelocity(v(64, 0, 0)),
    ))?;
    queue.enqueue(SimOrder::new(
        2,
        SimOrderSource::lowered_agent(2),
        2,
        body,
        SimOrderKind::AddVelocity(v(0, 0, -128)),
    ))?;

    ALLOCATIONS.store(0, Ordering::Relaxed);
    let run_report = queue.run_ticks(&mut sim, 2)?;
    let order_step_allocations = ALLOCATIONS.load(Ordering::Relaxed);

    let final_state = sim.body_state(body).expect("body exists after order run");
    let reverse_enqueue_hash_match = reverse_enqueue_hash_match(config)?;
    let capacity_code = capacity_probe();
    let missing_body_code = missing_body_probe(config);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "ticks_per_second={}\n\
             order_capacity={}\n\
             pending_orders={}\n\
             applied_total={}\n\
             first_order_fingerprint={:016X}\n\
             final_tick={}\n\
             steps={}\n\
             bodies_updated={}\n\
             final_position_raw={},{},{}\n\
             final_position_value={:.3},{:.3},{:.3}\n\
             final_velocity_raw={},{},{}\n\
             final_velocity_value={:.3},{:.3},{:.3}\n\
             final_hash={:016X}\n\
             reverse_enqueue_hash_match={}\n\
             order_step_allocations={}\n\
             capacity_code={}\n\
             missing_body_code={}\n\
             render_touched=false\n",
            config.ticks_per_second,
            queue.capacity(),
            queue.pending_orders(),
            queue.applied_total(),
            first_fingerprint,
            run_report.final_tick,
            run_report.steps,
            run_report.bodies_updated,
            raw(final_state.position.x),
            raw(final_state.position.y),
            raw(final_state.position.z),
            final_state.position.x.to_f64(),
            final_state.position.y.to_f64(),
            final_state.position.z.to_f64(),
            raw(final_state.velocity.x),
            raw(final_state.velocity.y),
            raw(final_state.velocity.z),
            final_state.velocity.x.to_f64(),
            final_state.velocity.y.to_f64(),
            final_state.velocity.z.to_f64(),
            run_report.state_hash,
            reverse_enqueue_hash_match,
            order_step_allocations,
            capacity_code,
            missing_body_code,
        ),
    )?;
    println!("wrote sim orders FSV stats to {}", path.display());
    Ok(())
}

fn reverse_enqueue_hash_match(config: SimConfig) -> Result<bool, EngineError> {
    let mut first = FixedTickSim::<1>::new(config, 7)?;
    let first_body = first.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let mut second = FixedTickSim::<1>::new(config, 7)?;
    let second_body = second.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;

    let earlier = SimOrder::new(
        1,
        SimOrderSource::human(9),
        1,
        first_body,
        SimOrderKind::SetVelocity(v(128, 0, 0)),
    );
    let later = SimOrder::new(
        1,
        SimOrderSource::human(9),
        2,
        first_body,
        SimOrderKind::SetVelocity(v(64, 0, 0)),
    );

    let mut first_queue = SimOrderQueue::<4>::default();
    first_queue.enqueue(later)?;
    first_queue.enqueue(earlier)?;
    first_queue.step(&mut first)?;

    let mut second_queue = SimOrderQueue::<4>::default();
    second_queue.enqueue(SimOrder {
        target: second_body,
        ..earlier
    })?;
    second_queue.enqueue(SimOrder {
        target: second_body,
        ..later
    })?;
    second_queue.step(&mut second)?;

    Ok(first.state_hash() == second.state_hash())
}

fn capacity_probe() -> &'static str {
    let mut queue = SimOrderQueue::<1>::default();
    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            SimBodyId::from_raw(0),
            SimOrderKind::AddVelocity(v(1, 0, 0)),
        ))
        .expect("first order fits");
    match queue.enqueue(SimOrder::new(
        1,
        SimOrderSource::human(1),
        2,
        SimBodyId::from_raw(0),
        SimOrderKind::AddVelocity(v(1, 0, 0)),
    )) {
        Err(err @ EngineError::SimOrderCapacityExceeded { .. }) => err.code(),
        other => panic!("expected SimOrderCapacityExceeded, got {other:?}"),
    }
}

fn missing_body_probe(config: SimConfig) -> &'static str {
    let mut sim = FixedTickSim::<1>::new(config, 8).expect("valid fixed tick sim");
    let mut queue = SimOrderQueue::<1>::default();
    queue
        .enqueue(SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            SimBodyId::from_raw(99),
            SimOrderKind::AddVelocity(v(1, 0, 0)),
        ))
        .expect("order fits");
    match queue.step(&mut sim) {
        Err(err @ EngineError::SimBodyMissing { .. }) => err.code(),
        other => panic!("expected SimBodyMissing, got {other:?}"),
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn raw(value: Fixed) -> i64 {
    value.raw()
}
