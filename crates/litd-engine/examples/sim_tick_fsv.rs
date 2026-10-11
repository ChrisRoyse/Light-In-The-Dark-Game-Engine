use std::{
    alloc::{GlobalAlloc, Layout, System},
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use litd_engine::{
    EngineError,
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimConfig},
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
    let path = env::var("LITD_SIM_TICK_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1133-sim-tick-fsv.txt"));

    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<4>::new(config, 0x1133)?;
    let mover = sim.spawn_body(v(0, 0, 0), v(64, 0, -128), v(0, 0, 0))?;
    let accelerating = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(4096, 0, 0))?;
    let initial_hash = sim.state_hash();

    ALLOCATIONS.store(0, Ordering::Relaxed);
    let run_report = sim.run_ticks(8)?;
    let step_allocations = ALLOCATIONS.load(Ordering::Relaxed);

    let mover_state = sim.body_state(mover).expect("body exists after fixed run");
    let accelerating_state = sim
        .body_state(accelerating)
        .expect("body exists after fixed run");

    let mut repeat = FixedTickSim::<4>::new(config, 0x1133)?;
    repeat.spawn_body(v(0, 0, 0), v(64, 0, -128), v(0, 0, 0))?;
    repeat.spawn_body(v(0, 0, 0), v(0, 0, 0), v(4096, 0, 0))?;
    let repeat_report = repeat.run_ticks(8)?;

    let mut zero_tick = FixedTickSim::<1>::new(config, 0x77)?;
    zero_tick.spawn_body(v(1, 2, 3), v(64, 64, 64), v(0, 0, 0))?;
    let zero_before = zero_tick.state_hash();
    let zero_report = zero_tick.run_ticks(0)?;
    let zero_after = zero_tick.state_hash();

    let never_true_report = repeat.run_until(3, |_| false)?;
    let invalid_tick_code = match SimConfig::new(0) {
        Err(err @ EngineError::SimInvalidConfig { .. }) => err.code(),
        other => panic!("expected SimInvalidConfig, got {other:?}"),
    };
    let capacity_code = capacity_probe(config);
    let overflow_code = overflow_probe(config);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "ticks_per_second={}\n\
             fixed_delta_raw={}\n\
             capacity={}\n\
             body_count={}\n\
             initial_hash={:016X}\n\
             final_tick={}\n\
             steps={}\n\
             bodies_updated={}\n\
             mover_position_raw={},{},{}\n\
             mover_position_value={:.3},{:.3},{:.3}\n\
             accelerating_position_raw={},{},{}\n\
             accelerating_position_value={:.3},{:.3},{:.3}\n\
             deterministic_repeat_hash_match={}\n\
             final_hash={:016X}\n\
             repeat_hash={:016X}\n\
             zero_tick_start={}\n\
             zero_tick_final={}\n\
             zero_tick_hash_preserved={}\n\
             never_true_steps={}\n\
             never_true_final_tick={}\n\
             step_allocations={}\n\
             invalid_tick_code={}\n\
             capacity_code={}\n\
             overflow_code={}\n\
             render_touched=false\n",
            config.ticks_per_second,
            sim.fixed_delta().raw(),
            sim.capacity(),
            sim.body_count(),
            initial_hash,
            run_report.final_tick,
            run_report.steps,
            run_report.bodies_updated,
            raw(mover_state.position.x),
            raw(mover_state.position.y),
            raw(mover_state.position.z),
            mover_state.position.x.to_f64(),
            mover_state.position.y.to_f64(),
            mover_state.position.z.to_f64(),
            raw(accelerating_state.position.x),
            raw(accelerating_state.position.y),
            raw(accelerating_state.position.z),
            accelerating_state.position.x.to_f64(),
            accelerating_state.position.y.to_f64(),
            accelerating_state.position.z.to_f64(),
            run_report.state_hash == repeat_report.state_hash,
            run_report.state_hash,
            repeat_report.state_hash,
            zero_report.start_tick,
            zero_report.final_tick,
            zero_before == zero_after,
            never_true_report.steps,
            never_true_report.final_tick,
            step_allocations,
            invalid_tick_code,
            capacity_code,
            overflow_code,
        ),
    )?;
    println!("wrote sim tick FSV stats to {}", path.display());
    Ok(())
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn raw(value: Fixed) -> i64 {
    value.raw()
}

fn capacity_probe(config: SimConfig) -> &'static str {
    let mut sim = FixedTickSim::<1>::new(config, 1).expect("valid fixed tick sim");
    sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .expect("first body fits");
    match sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)) {
        Err(err @ EngineError::SimCapacityExceeded { .. }) => err.code(),
        other => panic!("expected SimCapacityExceeded, got {other:?}"),
    }
}

fn overflow_probe(config: SimConfig) -> &'static str {
    let mut sim = FixedTickSim::<1>::new(config, 2).expect("valid fixed tick sim");
    sim.spawn_body(
        FixedVec3::new(Fixed::from_raw(i64::MAX), Fixed::ZERO, Fixed::ZERO),
        FixedVec3::from_i32s(64, 0, 0),
        FixedVec3::ZERO,
    )
    .expect("overflow probe body fits");
    match sim.step() {
        Err(err @ EngineError::FixedPointOverflow { .. }) => err.code(),
        other => panic!("expected FixedPointOverflow, got {other:?}"),
    }
}
