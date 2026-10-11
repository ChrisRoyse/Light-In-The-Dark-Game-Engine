use std::collections::BTreeMap;

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    ecs::{Entity, World},
    fsv_artifacts::begin_issue_run,
    math::{Aabb, FixedVec3, Vec2, Vec3},
    presentation::{
        PresentationBodyStyle, PresentationEntityKind, PresentationFrame,
        PresentationLifecycleDiff, diff_presentation_frames, extract_presentation_frame,
    },
    render::batching::{BatchingSettings, Frustum, ScreenRect, build_batch_plan},
    scene::SceneGraph,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::SimSnapshot,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue875_scene_lifecycle")?;
    let report_path = run.report_path();
    let report = build_report(&report_path.display().to_string())?;
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue875_scene_lifecycle_fsv report={} bytes={} sha256={} create={} update={} destroy={} graph_nodes={} edges={} manifest={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["update_diff"]["created_count"],
        readback.json["update_diff"]["updated_count"],
        readback.json["update_diff"]["destroyed_count"],
        readback.json["scene_graph_after_update"]["node_count"],
        readback.json["edges"].as_array().map_or(0, Vec::len),
        readback.latest_manifest_path.display(),
    );
    Ok(())
}

fn build_report(report_path: &str) -> Result<Value, EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(64)?, 875)?;
    let unit = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), FixedVec3::ZERO)?;
    let doodad = sim.spawn_body(v(2, 0, 0), v(0, 0, 0), FixedVec3::ZERO)?;
    let destructible = sim.spawn_body(v(4, 0, 0), v(0, 0, 0), FixedVec3::ZERO)?;
    let frame0 = frame(
        &SimSnapshot::capture(&sim),
        &[
            (unit, PresentationEntityKind::Unit),
            (doodad, PresentationEntityKind::Doodad),
            (destructible, PresentationEntityKind::Destructible),
        ],
    )?;
    let initial_diff = diff_presentation_frames(None, &frame0)?;

    let mut world = World::default();
    let mut graph = SceneGraph::default();
    let mut entity_by_body = BTreeMap::new();
    apply_diff(&mut world, &mut graph, &mut entity_by_body, &initial_diff)?;
    let initial_stats = graph.propagate()?;
    let initial_graph_state = graph_state(&graph, &entity_by_body, initial_stats);

    sim.set_body_velocity(unit, v(64, 0, 0))?;
    sim.step()?;
    let removed_doodad = sim.despawn_body(doodad)?;
    let effect = sim.spawn_body(v(6, 0, 0), v(0, 0, 0), FixedVec3::ZERO)?;
    let frame1 = frame(
        &SimSnapshot::capture(&sim),
        &[
            (unit, PresentationEntityKind::Unit),
            (destructible, PresentationEntityKind::Destructible),
            (effect, PresentationEntityKind::Effect),
        ],
    )?;
    let update_diff = frame1.lifecycle_diff_from(Some(&frame0))?;
    apply_diff(&mut world, &mut graph, &mut entity_by_body, &update_diff)?;
    let update_stats = graph.propagate()?;
    let update_graph_state = graph_state(&graph, &entity_by_body, update_stats);

    let render_plan = build_batch_plan(
        BatchingSettings::default(),
        &Frustum::orthographic(Aabb::from_center_half_extents(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(8.0, 4.0, 4.0),
        )?)?,
        &[],
        &frame1.render_instances(),
    )?;
    let edges = vec![
        edge_missing_style(&frame0),
        edge_duplicate_style(unit),
        edge_duplicate_lifecycle(&frame1),
        edge_scene_remove_missing(),
        edge_sim_double_despawn(&mut sim, doodad),
    ];

    Ok(json!({
        "schema_version": 1,
        "issue": 875,
        "source_of_truth": {
            "report_path": report_path,
            "source_modules": [
                "crates/litd-engine/src/presentation.rs",
                "crates/litd-engine/src/scene.rs",
                "crates/litd-engine/src/sim.rs"
            ],
            "manual_fsv_readback": "open this persisted JSON through Synapse, read its SHA256 from disk, and compare actual create/update/destroy graph state to expected"
        },
        "sufficiency_argument": "The synthetic two-frame scenario starts with unit, doodad, and destructible bodies and then moves the unit, despawns the doodad, keeps the destructible stable, and creates an effect. That covers every required render node kind and all lifecycle arms with known IDs. Five edge probes cover missing style, duplicate style, duplicate lifecycle identity, missing scene-node removal, and double sim despawn.",
        "initial_frame": frame_state(&frame0),
        "initial_diff": diff_state(&initial_diff),
        "scene_graph_after_initial": initial_graph_state,
        "removed_doodad": {
            "body_id": removed_doodad.id.index(),
            "position": vec3_fixed(removed_doodad.position),
        },
        "update_frame": frame_state(&frame1),
        "update_diff": diff_state(&update_diff),
        "scene_graph_after_update": update_graph_state,
        "render_plan": {
            "visible_instance_ids": render_plan.visible_instance_ids,
            "draw_batches": render_plan.draw_batches.len(),
            "batch_keys": render_plan.draw_batches.iter().map(|batch| {
                format!("{}:{}:{}", batch.key.mesh_id, batch.key.material_id, batch.instance_ids.len())
            }).collect::<Vec<_>>(),
        },
        "edges": edges,
        "expected_actuals": {
            "initial_created": 3,
            "update_created": [effect.index()],
            "update_updated": [unit.index()],
            "update_destroyed": [doodad.index()],
            "unchanged_body": destructible.index(),
            "scene_graph_nodes_after_update": 3,
            "tracked_body_ids_after_update": [unit.index(), destructible.index(), effect.index()],
            "render_visible_ids": [1, 4, 3],
            "edge_count": 5,
        }
    }))
}

fn frame(
    snapshot: &SimSnapshot,
    bindings: &[(SimBodyId, PresentationEntityKind)],
) -> Result<PresentationFrame, EngineError> {
    let mut styles = Vec::new();
    for (body_id, kind) in bindings {
        styles.push(style(*body_id, *kind)?);
    }
    extract_presentation_frame(snapshot, &styles)
}

fn style(
    body_id: SimBodyId,
    entity_kind: PresentationEntityKind,
) -> Result<PresentationBodyStyle, EngineError> {
    let index = body_id.index();
    Ok(PresentationBodyStyle {
        body_id,
        entity_kind,
        mesh_id: format!("{}_mesh", entity_kind.label()),
        material_id: format!("{}_material", entity_kind.label()),
        half_extents: Vec3::splat(0.45),
        screen_rect: rect(
            0.10 + index as f32 * 0.12,
            0.20,
            0.18 + index as f32 * 0.12,
            0.32,
        )?,
        nearest_depth: 0.25 + index as f32 * 0.05,
        index_count: 96 + index * 12,
    })
}

fn apply_diff(
    world: &mut World,
    graph: &mut SceneGraph,
    entity_by_body: &mut BTreeMap<u32, Entity>,
    diff: &PresentationLifecycleDiff,
) -> Result<(), EngineError> {
    for entry in &diff.destroyed {
        let body = entry.body_id.index();
        let entity = entity_by_body
            .remove(&body)
            .ok_or_else(|| EngineError::SceneNodeMissing {
                entity: format!("presentation body {body}"),
            })?;
        graph.remove(entity)?;
    }
    for entry in &diff.created {
        let entity = world.spawn(())?;
        graph.insert(entity, entry.transform)?;
        entity_by_body.insert(entry.body_id.index(), entity);
    }
    for entry in &diff.updated {
        let entity = entity_by_body
            .get(&entry.body_id.index())
            .copied()
            .ok_or_else(|| EngineError::SceneNodeMissing {
                entity: format!("presentation body {}", entry.body_id.index()),
            })?;
        graph.set_local_transform(entity, entry.transform)?;
    }
    Ok(())
}

fn edge_missing_style(frame: &PresentationFrame) -> Value {
    let body_id = frame.instances[0].body_id;
    edge_result(
        "missing_style",
        json!({"body_id": body_id.index(), "style_count": 0}),
        extract_presentation_frame(
            &SimSnapshot {
                schema_version: 3,
                config: SimConfig::new(64).unwrap(),
                tick: frame.source_tick,
                next_body_id: body_id.index() + 1,
                rng_state: 1,
                bodies: vec![litd_engine::sim::SimBodyState {
                    id: body_id,
                    position: v(0, 0, 0),
                    velocity: FixedVec3::ZERO,
                    acceleration: FixedVec3::ZERO,
                }],
                attack_events: vec![],
                attack_events_digest: 0,
                attack_events_total: 0,
                state_hash: frame.source_state_hash,
            },
            &[],
        ),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    )
}

fn edge_duplicate_style(body_id: SimBodyId) -> Value {
    let first = style(body_id, PresentationEntityKind::Unit).unwrap();
    let duplicate = PresentationBodyStyle {
        material_id: "duplicate".to_string(),
        ..first.clone()
    };
    let snapshot = SimSnapshot {
        schema_version: 3,
        config: SimConfig::new(64).unwrap(),
        tick: 0,
        next_body_id: body_id.index() + 1,
        rng_state: 1,
        bodies: vec![litd_engine::sim::SimBodyState {
            id: body_id,
            position: v(0, 0, 0),
            velocity: FixedVec3::ZERO,
            acceleration: FixedVec3::ZERO,
        }],
        attack_events: vec![],
        attack_events_digest: 0,
        attack_events_total: 0,
        state_hash: 0,
    };
    edge_result(
        "duplicate_style",
        json!({"body_id": body_id.index()}),
        extract_presentation_frame(&snapshot, &[first, duplicate]),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    )
}

fn edge_duplicate_lifecycle(frame: &PresentationFrame) -> Value {
    let mut duplicate = frame.clone();
    duplicate.instances.push(frame.instances[0].clone());
    edge_result(
        "duplicate_lifecycle_body",
        json!({"body_id": frame.instances[0].body_id.index()}),
        diff_presentation_frames(Some(frame), &duplicate),
        "LITD-RENDER-BATCHING-INVALID-CONFIG",
    )
}

fn edge_scene_remove_missing() -> Value {
    let mut graph = SceneGraph::default();
    let mut world = World::default();
    let missing = world.spawn(()).unwrap();
    edge_result(
        "scene_remove_missing",
        json!({"entity": missing.to_string()}),
        graph.remove(missing),
        "LITD-SCENE-NODE-MISSING",
    )
}

fn edge_sim_double_despawn(sim: &mut FixedTickSim<4>, body_id: SimBodyId) -> Value {
    edge_result(
        "sim_double_despawn",
        json!({"body_id": body_id.index()}),
        sim.despawn_body(body_id),
        "LITD-SIM-BODY-MISSING",
    )
}

fn frame_state(frame: &PresentationFrame) -> Value {
    json!({
        "source_tick": frame.source_tick,
        "source_state_hash": format!("{:016X}", frame.source_state_hash),
        "body_ids": frame.instances.iter().map(|instance| instance.body_id.index()).collect::<Vec<_>>(),
        "entity_kinds": frame.instances.iter().map(|instance| instance.entity_kind.label()).collect::<Vec<_>>(),
        "render_instance_ids": frame.instances.iter().map(|instance| instance.render_instance.id).collect::<Vec<_>>(),
        "mesh_ids": frame.instances.iter().map(|instance| instance.render_instance.mesh_id.clone()).collect::<Vec<_>>(),
        "translations": frame.instances.iter().map(|instance| vec3(instance.transform.translation)).collect::<Vec<_>>(),
    })
}

fn diff_state(diff: &PresentationLifecycleDiff) -> Value {
    json!({
        "summary": diff.summary(),
        "created_count": diff.created.len(),
        "updated_count": diff.updated.len(),
        "destroyed_count": diff.destroyed.len(),
        "created": entries(&diff.created),
        "updated": entries(&diff.updated),
        "destroyed": entries(&diff.destroyed),
    })
}

fn entries(entries: &[litd_engine::presentation::PresentationLifecycleEntry]) -> Value {
    json!(
        entries
            .iter()
            .map(|entry| {
                json!({
                    "body_id": entry.body_id.index(),
                    "kind": entry.entity_kind.label(),
                    "entity_id": entry.render_instance_id,
                    "mesh": entry.mesh_id,
                    "material": entry.material_id,
                    "translation": vec3(entry.transform.translation),
                })
            })
            .collect::<Vec<_>>()
    )
}

fn graph_state(
    graph: &SceneGraph,
    entity_by_body: &BTreeMap<u32, Entity>,
    stats: litd_engine::scene::ScenePropagationStats,
) -> Value {
    json!({
        "node_count": graph.len(),
        "root_count": graph.root_count(),
        "dirty_count": graph.dirty_count(),
        "propagation": {
            "visited_nodes": stats.visited_nodes,
            "recomputed_nodes": stats.recomputed_nodes,
            "root_count": stats.root_count,
        },
        "tracked_body_ids": entity_by_body.keys().copied().collect::<Vec<_>>(),
        "tracked_entities": entity_by_body.iter().map(|(body, entity)| {
            format!("{body}:{}", entity)
        }).collect::<Vec<_>>(),
    })
}

fn edge_result<T>(
    name: &'static str,
    before: Value,
    result: Result<T, EngineError>,
    expected_code: &'static str,
) -> Value {
    match result {
        Ok(_) => json!({
            "name": name,
            "before": before,
            "after": {"status": "accepted"},
            "expected_code": expected_code,
            "matched_expected_code": false,
        }),
        Err(error) => {
            let report: ErrorReport = error.report();
            json!({
                "name": name,
                "before": before,
                "after": {
                    "status": "rejected",
                    "error": {
                        "code": report.code,
                        "message": report.message,
                        "remediation": report.remediation,
                        "source_error": report.source_error,
                    }
                },
                "expected_code": expected_code,
                "matched_expected_code": report.code == expected_code,
            })
        }
    }
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let ok = report["issue"] == 875
        && report["initial_diff"]["created_count"] == 3
        && report["update_diff"]["created_count"] == 1
        && report["update_diff"]["updated_count"] == 1
        && report["update_diff"]["destroyed_count"] == 1
        && report["scene_graph_after_update"]["node_count"] == 3
        && report["render_plan"]["visible_instance_ids"] == json!([1, 4, 3])
        && report["update_frame"]["entity_kinds"] == json!(["unit", "effect", "destructible"])
        && report["edges"].as_array().is_some_and(|edges| {
            edges.len() == 5
                && edges
                    .iter()
                    .all(|edge| edge["matched_expected_code"] == true)
        });
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue875 scene lifecycle report with typed create/update/destroy and five matched edges".to_string(),
            actual: report.to_string(),
        })
    }
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn vec3(value: Vec3) -> String {
    format!("{:.3},{:.3},{:.3}", value.x, value.y, value.z)
}

fn vec3_fixed(value: FixedVec3) -> String {
    vec3(Vec3::new(
        value.x.to_f64() as f32,
        value.y.to_f64() as f32,
        value.z.to_f64() as f32,
    ))
}
