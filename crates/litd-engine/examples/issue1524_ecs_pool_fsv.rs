//! FSV for issue #1524 item 1: prove the ECS parallel query stopped launching a fresh
//! OS thread per archetype **per call** and now reuses a persistent Rayon pool.
//!
//! Source of truth = the number of DISTINCT OS threads that execute the per-archetype
//! work across a fixed workload (`ARCHETYPES` archetypes x `ITERS` calls). The old
//! `std::thread::scope` + `Scope::spawn` pattern creates a brand-new thread for every
//! spawn, so the distinct-thread count grows with the workload. Rayon's pool has a
//! fixed set of long-lived workers, so the distinct-thread count is bounded by the pool
//! size no matter how many calls are made.
//!
//! BEFORE is the exact old primitive (`std::thread::scope`) run over synthetic
//! archetype data; AFTER is (a) the same work over `rayon::scope` and (b) the REAL
//! `World::for_each2_mut_parallel`, which now uses `rayon::scope`. All three do
//! identical per-archetype mutations so the comparison isolates the thread primitive.

use std::{
    collections::HashSet,
    env, fs,
    path::PathBuf,
    sync::Mutex,
    thread::{self, ThreadId},
    time::Instant,
};

use litd_engine::ecs::World;
use litd_engine::math::Vec3;

const ARCHETYPES: usize = 6;
const ENTITIES_PER_ARCHETYPE: usize = 256;
const ITERS: usize = 400;

#[derive(Debug)]
struct Position(Vec3);
#[derive(Debug)]
struct Velocity(Vec3);
// Distinct marker components force the shared (Position, Velocity) pair to live in
// several archetypes, so the parallel query spawns several tasks per call.
#[derive(Debug)]
struct M0;
#[derive(Debug)]
struct M1;
#[derive(Debug)]
struct M2;
#[derive(Debug)]
struct M3;
#[derive(Debug)]
struct M4;
#[derive(Debug)]
struct M5;

/// Old primitive: a fresh OS thread per archetype, per call.
fn run_before(data: &mut [Vec<u64>], seen: &Mutex<HashSet<ThreadId>>) {
    thread::scope(|scope| {
        for column in data.iter_mut() {
            let seen = &*seen;
            scope.spawn(move || {
                seen.lock().unwrap().insert(thread::current().id());
                for value in column.iter_mut() {
                    *value = value.wrapping_add(1);
                }
            });
        }
    });
}

/// New primitive: Rayon's persistent pool (this is exactly what `World` now does).
fn run_after(data: &mut [Vec<u64>], seen: &Mutex<HashSet<ThreadId>>) {
    rayon::scope(|scope| {
        for column in data.iter_mut() {
            let seen = &*seen;
            scope.spawn(move |_| {
                seen.lock().unwrap().insert(thread::current().id());
                for value in column.iter_mut() {
                    *value = value.wrapping_add(1);
                }
            });
        }
    });
}

fn build_world() -> Result<World, Box<dyn std::error::Error>> {
    let mut world = World::default();
    let p = || Position(Vec3::new(1.0, 2.0, 3.0));
    let v = || Velocity(Vec3::new(0.5, 0.0, 1.5));
    for _ in 0..ENTITIES_PER_ARCHETYPE {
        world.spawn((p(), v(), M0))?;
        world.spawn((p(), v(), M1))?;
        world.spawn((p(), v(), M2))?;
        world.spawn((p(), v(), M3))?;
        world.spawn((p(), v(), M4))?;
        world.spawn((p(), v(), M5))?;
    }
    Ok(world)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    // Self-cleaning scratch dir (#1535): removed on Drop, so keep `temp` alive.
    let temp = litd_engine::test_temp::TestTempDir::new("issue1524-ecs-pool");
    let path = match env::var("LITD_ISSUE1524_ECS_STATS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => temp.path().join("issue1524-ecs-pool-fsv.txt"),
    };

    let mut synth: Vec<Vec<u64>> = (0..ARCHETYPES)
        .map(|_| vec![0u64; ENTITIES_PER_ARCHETYPE])
        .collect();

    // BEFORE: old std::thread::scope primitive.
    let before_seen = Mutex::new(HashSet::new());
    let before_start = Instant::now();
    for _ in 0..ITERS {
        run_before(&mut synth, &before_seen);
    }
    let before_elapsed = before_start.elapsed();
    let before_threads = before_seen.lock().unwrap().len();

    // AFTER (synthetic): identical work over rayon::scope.
    let after_seen = Mutex::new(HashSet::new());
    let after_start = Instant::now();
    for _ in 0..ITERS {
        run_after(&mut synth, &after_seen);
    }
    let after_elapsed = after_start.elapsed();
    let after_threads = after_seen.lock().unwrap().len();

    // AFTER (real): the actual World::for_each2_mut_parallel now uses rayon::scope.
    let mut world = build_world()?;
    let real_seen = Mutex::new(HashSet::new());
    let mut touched = 0usize;
    for _ in 0..ITERS {
        touched =
            world.for_each2_mut_parallel::<Position, Velocity, _>(|_, position, velocity| {
                real_seen.lock().unwrap().insert(thread::current().id());
                position.0 += velocity.0;
            })?;
    }
    let real_threads = real_seen.lock().unwrap().len();
    let expected_touched = ARCHETYPES * ENTITIES_PER_ARCHETYPE;
    let pool_threads = rayon::current_num_threads();

    // Determinism / correctness: read every matching entity back and confirm each
    // Position advanced exactly ITERS * velocity, i.e. the parallel query visited each
    // matching entity exactly once per call with no lost or double updates.
    let expected_x = 1.0 + 0.5 * ITERS as f32;
    let mut visited = 0usize;
    let mut all_advanced = true;
    world.for_each2::<Position, Velocity>(|_, position, _| {
        visited += 1;
        if (position.0.x - expected_x).abs() > 1e-3 {
            all_advanced = false;
        }
    })?;
    let determinism_ok = all_advanced && visited == expected_touched;

    if touched != expected_touched {
        return Err(
            format!("real parallel query touched {touched}, expected {expected_touched}").into(),
        );
    }
    if after_threads > pool_threads {
        return Err(format!(
            "rayon distinct threads {after_threads} exceeded pool size {pool_threads}"
        )
        .into());
    }
    if real_threads > pool_threads {
        return Err(format!(
            "real query distinct threads {real_threads} exceeded pool size {pool_threads}"
        )
        .into());
    }
    if before_threads <= after_threads {
        return Err(format!(
            "expected old primitive to spawn more distinct threads: before={before_threads} after={after_threads}"
        )
        .into());
    }

    let stats = format!(
        "archetypes={ARCHETYPES}\n\
         entities_per_archetype={ENTITIES_PER_ARCHETYPE}\n\
         iters={ITERS}\n\
         rayon_pool_threads={pool_threads}\n\
         before_distinct_threads={before_threads}\n\
         after_synth_distinct_threads={after_threads}\n\
         after_real_distinct_threads={real_threads}\n\
         before_elapsed_ms={:.3}\n\
         after_elapsed_ms={:.3}\n\
         real_touched_per_call={touched}\n\
         determinism_ok={determinism_ok}\n",
        before_elapsed.as_secs_f64() * 1e3,
        after_elapsed.as_secs_f64() * 1e3,
    );

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &stats)?;
    let persisted = fs::read_to_string(&path)?;
    if persisted != stats {
        return Err("issue1524 ECS pool FSV stats changed after physical write".into());
    }
    print!("{stats}");
    println!("wrote issue1524 ECS pool FSV stats to {}", path.display());
    Ok(())
}
