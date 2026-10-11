use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_ISSUE1295_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("target/fsv/issue1295_sim_heap/manual/sim-heap-storage.txt")
        });
    let stack_bytes = std::mem::size_of::<FixedTickSim<32_768>>();
    if stack_bytes >= 256 {
        return Err(format!("FixedTickSim stack footprint too large: {stack_bytes}").into());
    }
    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<32_768>::new(config, 0x1295)?;
    let before_capacity = sim.capacity();
    let before_body_count = sim.body_count();
    for index in 0..4096 {
        sim.spawn_body(v(index, 0, -(index % 64)), v(1, 0, -1), v(0, 0, 0))?;
    }
    let after_spawn_count = sim.body_count();
    let report = sim.run_ticks(2)?;
    let after_run_hash = sim.state_hash();

    for _ in 0..(sim.capacity() - sim.body_count()) {
        sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    }
    let capacity_code = match sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)) {
        Err(err @ EngineError::SimCapacityExceeded { .. }) => err.code(),
        other => panic!("expected SimCapacityExceeded, got {other:?}"),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!(
        "issue=1295\n\
         source_of_truth=physical stats file written after heap-backed FixedTickSim run\n\
         stack_bytes={}\n\
         before_capacity={}\n\
         before_body_count={}\n\
         after_spawn_count={}\n\
         final_tick={}\n\
         steps={}\n\
         bodies_updated={}\n\
         after_run_hash={:016X}\n\
         state_hash_matches_report={}\n\
         capacity_after_fill={}\n\
         body_count_after_fill={}\n\
         capacity_code={}\n",
        stack_bytes,
        before_capacity,
        before_body_count,
        after_spawn_count,
        report.final_tick,
        report.steps,
        report.bodies_updated,
        after_run_hash,
        after_run_hash == report.state_hash,
        sim.capacity(),
        sim.body_count(),
        capacity_code
    );
    fs::write(&path, body)?;
    let readback = fs::read_to_string(&path)?;
    if !readback.contains("capacity_code=LITD-SIM-CAPACITY-EXCEEDED")
        || !readback.contains("state_hash_matches_report=true")
    {
        return Err(format!("sim heap FSV readback invalid at {}", path.display()).into());
    }
    println!("wrote issue1295 sim heap FSV stats to {}", path.display());
    Ok(())
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
