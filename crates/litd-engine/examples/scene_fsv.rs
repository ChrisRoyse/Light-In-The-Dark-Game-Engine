use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    ecs::{Entity, World},
    math::{Transform, Vec3},
    scene::SceneGraph,
};

fn entity(world: &mut World) -> Entity {
    world.spawn(()).expect("empty ECS entity spawn must work")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_SCENE_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1083-scene-fsv.txt"));

    let mut world = World::default();
    let root = entity(&mut world);
    let child = entity(&mut world);
    let grandchild = entity(&mut world);

    let mut graph = SceneGraph::default();
    graph.insert(root, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)))?;
    graph.insert(child, Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)))?;
    graph.insert(
        grandchild,
        Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
    )?;
    graph.set_parent(child, Some(root))?;
    graph.set_parent(grandchild, Some(child))?;

    let initial = graph.propagate()?;
    let child_global = graph.global_translation(child)?;
    graph.set_local_transform(child, Transform::from_translation(Vec3::new(0.0, 7.0, 0.0)))?;
    let second = graph.propagate()?;
    let grandchild_global = graph.global_translation(grandchild)?;
    let cycle_code = match graph.set_parent(root, Some(grandchild)) {
        Err(err @ EngineError::SceneParentCycle { .. }) => err.code(),
        other => panic!("expected SceneParentCycle, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "node_count={}\n\
             root_count={}\n\
             initial_recomputed={}\n\
             second_recomputed={}\n\
             dirty_after_propagate={}\n\
             child_global={:.3},{:.3},{:.3}\n\
             grandchild_global_after_update={:.3},{:.3},{:.3}\n\
             cycle_error_code={}\n",
            graph.len(),
            graph.root_count(),
            initial.recomputed_nodes,
            second.recomputed_nodes,
            graph.dirty_count(),
            child_global.x,
            child_global.y,
            child_global.z,
            grandchild_global.x,
            grandchild_global.y,
            grandchild_global.z,
            cycle_code,
        ),
    )?;
    println!("wrote scene FSV stats to {}", path.display());
    Ok(())
}
