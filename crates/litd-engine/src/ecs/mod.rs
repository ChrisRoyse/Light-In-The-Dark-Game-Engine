//! Archetype ECS foundation: entities, typed components, queries, events, and schedules.

mod entity;
mod schedule;
mod storage;
mod world;

pub use entity::Entity;
pub use schedule::{Schedule, ScheduleReport};
pub use storage::{Component, ComponentBundle, Event};
pub use world::{ArchetypeInfo, World, WorldSlotStats};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineError, math::Vec3};

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

    #[test]
    fn spawn_get_and_change_ticks_are_typed() {
        let mut world = World::default();
        let entity = world
            .spawn((
                Position(Vec3::new(1.0, 2.0, 3.0)),
                Velocity(Vec3::new(1.0, 0.0, 0.0)),
            ))
            .unwrap();
        let baseline = world.current_tick();

        world.get_mut::<Position>(entity).unwrap().0.x += 2.0;

        assert_eq!(world.get::<Position>(entity).unwrap().0.x, 3.0);
        assert!(
            world
                .component_changed_since::<Position>(entity, baseline)
                .unwrap()
        );
        assert!(
            !world
                .component_changed_since::<Velocity>(entity, baseline)
                .unwrap()
        );
    }

    #[test]
    fn parallel_query_moves_matching_archetypes() {
        let mut world = World::default();
        let moving = world
            .spawn((
                Position(Vec3::new(1.0, 2.0, 3.0)),
                Velocity(Vec3::new(0.5, 0.0, 1.5)),
            ))
            .unwrap();
        world.spawn((Position(Vec3::ZERO),)).unwrap();

        let touched = world
            .for_each2_mut_parallel::<Position, Velocity, _>(|_, position, velocity| {
                position.0 += velocity.0;
            })
            .unwrap();

        assert_eq!(touched, 1);
        assert_eq!(
            world.get::<Position>(moving).unwrap().0,
            Vec3::new(1.5, 2.0, 4.5)
        );
    }

    #[test]
    fn schedule_runs_systems_and_drains_events() {
        let mut world = World::default();
        let unit = world
            .spawn((Position(Vec3::ZERO), Health(10), Velocity(Vec3::X)))
            .unwrap();
        world.send_event(Damage {
            target: unit,
            amount: 3,
        });

        let mut schedule = Schedule::default();
        schedule.add_system("damage", |world| {
            for event in world.drain_events::<Damage>() {
                world.get_mut::<Health>(event.target)?.0 -= event.amount;
            }
            Ok(())
        });
        let report = schedule.run(&mut world).unwrap();

        assert_eq!(report.system_runs, 1);
        assert_eq!(world.get::<Health>(unit).unwrap().0, 7);
        assert_eq!(world.pending_event_count::<Damage>(), 0);
    }

    #[test]
    fn stale_entity_fails_loud() {
        let mut world = World::default();
        let entity = world.spawn((Position(Vec3::ZERO),)).unwrap();
        world.despawn(entity).unwrap();

        match world.get::<Position>(entity) {
            Err(EngineError::EcsEntityNotAlive { index, generation }) => {
                assert_eq!(index, entity.index());
                assert_eq!(generation, entity.generation());
            }
            other => panic!("expected EcsEntityNotAlive, got {other:?}"),
        }
    }
}
