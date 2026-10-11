use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    ecs::{Entity, Schedule, World},
    fsv_artifacts::begin_issue_run,
    math::Vec3,
};

#[derive(Debug)]
struct Position(Vec3);

#[derive(Debug)]
struct Velocity(Vec3);

#[derive(Debug)]
struct Health(i32);

struct Damage {
    target: Entity,
    amount: i32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = match env::var("LITD_ECS_FSV_STATS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => begin_issue_run("issue1082_ecs")?
            .run_dir()
            .join("ecs-fsv-stats.txt"),
    };

    let mut world = World::default();
    let unit = world.spawn((
        Position(Vec3::new(1.0, 2.0, 3.0)),
        Velocity(Vec3::new(0.5, 0.0, 1.5)),
        Health(10),
    ))?;
    let _marker = world.spawn((Position(Vec3::new(-2.0, 0.0, 1.0)),))?;
    let stale = world.spawn((Position(Vec3::ZERO),))?;
    world.despawn(stale)?;

    let baseline_tick = world.current_tick();
    world.send_event(Damage {
        target: unit,
        amount: 3,
    });

    let mut schedule = Schedule::default();
    schedule.add_system("parallel-move", |world| {
        world.for_each2_mut_parallel::<Position, Velocity, _>(|_, position, velocity| {
            position.0 += velocity.0;
        })?;
        Ok(())
    });
    schedule.add_system("damage-events", |world| {
        for event in world.drain_events::<Damage>() {
            world.get_mut::<Health>(event.target)?.0 -= event.amount;
        }
        Ok(())
    });
    let report = schedule.run(&mut world)?;

    let moved = world.get::<Position>(unit)?.0;
    let health = world.get::<Health>(unit)?.0;
    let stale_code = match world.get::<Position>(stale) {
        Err(err @ EngineError::EcsEntityNotAlive { .. }) => err.code(),
        other => panic!("expected EcsEntityNotAlive, got {other:?}"),
    };
    let position_changed = world.component_changed_since::<Position>(unit, baseline_tick)?;
    let velocity_changed = world.component_changed_since::<Velocity>(unit, baseline_tick)?;
    let pending_damage_events = world.pending_event_count::<Damage>();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stats = format!(
        "entity_count={}\n\
             archetype_count={}\n\
             schedule_systems={}\n\
             schedule_system_runs={}\n\
             moved_position={:.3},{:.3},{:.3}\n\
             health_after_damage={}\n\
             position_changed_since_baseline={}\n\
             velocity_changed_since_baseline={}\n\
             damage_events_after_drain={}\n\
             stale_entity_error_code={}\n",
        world.len(),
        world.archetype_count(),
        report.system_count,
        report.system_runs,
        moved.x,
        moved.y,
        moved.z,
        health,
        position_changed,
        velocity_changed,
        pending_damage_events,
        stale_code,
    );
    fs::write(&path, &stats)?;
    let persisted = fs::read_to_string(&path)?;
    if persisted != stats {
        return Err("ECS FSV stats changed after physical write".into());
    }
    for expected in [
        "entity_count=2",
        "schedule_systems=2",
        "schedule_system_runs=2",
        "moved_position=1.500,2.000,4.500",
        "health_after_damage=7",
        "position_changed_since_baseline=true",
        "velocity_changed_since_baseline=false",
        "damage_events_after_drain=0",
        "stale_entity_error_code=LITD-ECS-ENTITY-NOT-ALIVE",
    ] {
        if !persisted.lines().any(|line| line.trim() == expected) {
            return Err(format!("ECS FSV persisted readback missing {expected:?}").into());
        }
    }
    println!("wrote ECS FSV stats to {}", path.display());
    Ok(())
}
