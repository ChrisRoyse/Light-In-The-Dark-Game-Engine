use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::{
    EngineError,
    ecs::{Entity, World, WorldSlotStats},
    fsv_artifacts::begin_issue_run,
    math::Vec3,
};

#[derive(Debug)]
struct Position(Vec3);

#[derive(Debug)]
struct Health(i32);

#[test]
fn issue1331_ecs_slot_free_list_reuses_despawned_slots() {
    let run = begin_issue_run("issue1331_ecs_slot_free_list").unwrap();
    let report_path = run.report_path().to_path_buf();

    let mut world = World::default();
    let initial = stats_json(world.slot_stats());
    let mut first_generation = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        first_generation.push(
            world
                .spawn((Position(Vec3::new(i as f32, 0.0, 0.0)), Health(i)))
                .unwrap(),
        );
    }
    let after_first_spawn = world.slot_stats();
    assert_eq!(after_first_spawn.slot_count, 10_000);
    assert_eq!(after_first_spawn.free_slot_count, 0);
    assert_eq!(after_first_spawn.alive_entities, 10_000);
    assert_eq!(world.get::<Health>(first_generation[1234]).unwrap().0, 1234);

    for entity in first_generation.iter().copied() {
        world.despawn(entity).unwrap();
    }
    let after_despawn_all = world.slot_stats();
    assert_eq!(after_despawn_all.slot_count, 10_000);
    assert_eq!(after_despawn_all.free_slot_count, 10_000);
    assert_eq!(after_despawn_all.alive_entities, 0);

    let mut second_generation = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        second_generation.push(
            world
                .spawn((
                    Position(Vec3::new((20_000 + i) as f32, 1.0, 0.0)),
                    Health(20_000 + i),
                ))
                .unwrap(),
        );
    }
    let after_second_spawn = world.slot_stats();
    assert_eq!(after_second_spawn.slot_count, 10_000);
    assert_eq!(after_second_spawn.free_slot_count, 0);
    assert_eq!(after_second_spawn.alive_entities, 10_000);
    assert_eq!(world.get::<Health>(second_generation[0]).unwrap().0, 20_000);
    assert_eq!(
        world.get::<Position>(second_generation[0]).unwrap().0.y,
        1.0
    );

    let first_indices = entity_indices(&first_generation);
    let second_indices = entity_indices(&second_generation);
    assert_eq!(first_indices, second_indices);
    assert!(
        second_generation
            .iter()
            .all(|entity| entity.generation() == 1)
    );
    let stale_error = world.get::<Position>(first_generation[0]).unwrap_err();
    assert!(matches!(stale_error, EngineError::EcsEntityNotAlive { .. }));

    let reuse_edge = reuse_generation_edge();
    let empty_free_list_edge = empty_free_list_edge();
    let double_despawn_edge = double_despawn_edge();
    let generation_overflow_edge = generation_overflow_edge();
    let max_free_index_edge = max_free_index_edge();

    let report = json!({
        "schema_version": 1,
        "issue": 1331,
        "source_of_truth": {
            "report_path": report_path,
            "world_state": "World::slot_stats read after each action",
            "entity_count": 10_000,
        },
        "happy_path": {
            "initial": initial,
            "after_first_spawn": stats_json(after_first_spawn),
            "after_despawn_all": stats_json(after_despawn_all),
            "after_second_spawn": stats_json(after_second_spawn),
            "first_unique_indices": first_indices.len(),
            "second_unique_indices": second_indices.len(),
            "index_sets_equal": first_indices == second_indices,
            "reused_generation": second_generation[0].generation(),
            "stale_error_code": stale_error.code(),
            "sample_new_entity": entity_json(second_generation[0]),
            "sample_new_health": world.get::<Health>(second_generation[0]).unwrap().0,
            "sample_new_position_y": world.get::<Position>(second_generation[0]).unwrap().0.y,
        },
        "edges": {
            "reuse_after_despawn_generation_mismatch": reuse_edge,
            "empty_free_list_grows_once": empty_free_list_edge,
            "double_despawn_does_not_duplicate_free_slot": double_despawn_edge,
            "generation_overflow_fails_before_mutation": generation_overflow_edge,
            "max_free_index_corruption_fails_before_mutation": max_free_index_edge,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_json = report_readback.json;
    assert_eq!(report_json["issue"], 1331);
    assert_eq!(
        report_json["happy_path"]["after_second_spawn"]["slot_count"],
        10_000
    );
    assert_eq!(
        report_json["happy_path"]["after_despawn_all"]["free_slot_count"],
        10_000
    );
    assert_eq!(report_json["happy_path"]["index_sets_equal"], true);
    assert_eq!(
        report_json["edges"]["double_despawn_does_not_duplicate_free_slot"]["after"]["free_slot_count"],
        1
    );
    assert_eq!(
        report_json["edges"]["generation_overflow_fails_before_mutation"]["error_code"],
        "LITD-ECS-ENTITY-GENERATION-OVERFLOW"
    );
    assert_eq!(
        report_json["edges"]["max_free_index_corruption_fails_before_mutation"]["error_code"],
        "LITD-ECS-FREE-LIST-CORRUPT"
    );
    println!(
        "ISSUE1331_FSV_REPORT_READBACK path={} bytes={} sha256={} after_first_slots={} after_despawn_free={} after_second_slots={} stale_error={}",
        report_path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        after_first_spawn.slot_count,
        after_despawn_all.free_slot_count,
        after_second_spawn.slot_count,
        stale_error.code(),
    );
}

fn reuse_generation_edge() -> Value {
    let mut world = World::default();
    let original = world.spawn((Position(Vec3::ZERO), Health(7))).unwrap();
    let before = stats_json(world.slot_stats());
    world.despawn(original).unwrap();
    let after_despawn = stats_json(world.slot_stats());
    let reused = world.spawn((Position(Vec3::X), Health(9))).unwrap();
    let after_reuse = stats_json(world.slot_stats());
    let stale_error = world.get::<Position>(original).unwrap_err();
    assert_eq!(original.index(), reused.index());
    assert_eq!(reused.generation(), original.generation() + 1);
    assert!(matches!(stale_error, EngineError::EcsEntityNotAlive { .. }));
    assert_eq!(world.get::<Health>(reused).unwrap().0, 9);
    json!({
        "before": before,
        "after_despawn": after_despawn,
        "after": after_reuse,
        "original": entity_json(original),
        "reused": entity_json(reused),
        "same_index": original.index() == reused.index(),
        "generation_incremented": reused.generation() == original.generation() + 1,
        "stale_error_code": stale_error.code(),
        "reused_health": world.get::<Health>(reused).unwrap().0,
    })
}

fn empty_free_list_edge() -> Value {
    let mut world = World::default();
    let before = stats_json(world.slot_stats());
    let entity = world.spawn((Position(Vec3::ZERO),)).unwrap();
    let after = stats_json(world.slot_stats());
    assert_eq!(world.slot_stats().slot_count, 1);
    assert_eq!(world.slot_stats().free_slot_count, 0);
    json!({
        "before": before,
        "after": after,
        "spawned": entity_json(entity),
    })
}

fn double_despawn_edge() -> Value {
    let mut world = World::default();
    let entity = world.spawn((Position(Vec3::ZERO),)).unwrap();
    world.despawn(entity).unwrap();
    let before = stats_json(world.slot_stats());
    let error = world.despawn(entity).unwrap_err();
    let after = stats_json(world.slot_stats());
    assert!(matches!(error, EngineError::EcsEntityNotAlive { .. }));
    assert_eq!(world.slot_stats().free_slot_count, 1);
    json!({
        "before": before,
        "after": after,
        "error_code": error.code(),
        "error_message": error.to_string(),
    })
}

fn generation_overflow_edge() -> Value {
    let mut world = World::default();
    let entity = world.spawn((Position(Vec3::ZERO),)).unwrap();
    world.issue1331_set_slot_generation_for_test(entity, u32::MAX);
    let max_entity = world.issue1331_entity_for_slot_for_test(entity.index());
    let before = stats_json(world.slot_stats());
    let error = world.despawn(max_entity).unwrap_err();
    let after = stats_json(world.slot_stats());
    assert!(matches!(
        error,
        EngineError::EcsEntityGenerationOverflow {
            index: 0,
            generation: u32::MAX
        }
    ));
    assert_eq!(before, after);
    json!({
        "before": before,
        "after": after,
        "entity": entity_json(max_entity),
        "error_code": error.code(),
        "error_message": error.to_string(),
    })
}

fn max_free_index_edge() -> Value {
    let mut world = World::default();
    world.issue1331_push_free_slot_for_test(u32::MAX);
    let before = stats_json(world.slot_stats());
    let error = world.spawn((Position(Vec3::ZERO),)).unwrap_err();
    let after = stats_json(world.slot_stats());
    assert!(matches!(
        error,
        EngineError::EcsFreeListCorrupt {
            index: u32::MAX,
            ..
        }
    ));
    assert_eq!(before, after);
    json!({
        "before": before,
        "after": after,
        "free_index": u32::MAX,
        "error_code": error.code(),
        "error_message": error.to_string(),
    })
}

fn entity_indices(entities: &[Entity]) -> BTreeSet<u32> {
    entities.iter().map(|entity| entity.index()).collect()
}

fn entity_json(entity: Entity) -> Value {
    json!({
        "index": entity.index(),
        "generation": entity.generation(),
    })
}

fn stats_json(stats: WorldSlotStats) -> Value {
    json!({
        "slot_count": stats.slot_count,
        "free_slot_count": stats.free_slot_count,
        "alive_entities": stats.alive_entities,
        "archetype_count": stats.archetype_count,
        "current_tick": stats.current_tick,
    })
}
