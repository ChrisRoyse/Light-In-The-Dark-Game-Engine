use litd_engine::math::{Transform, Vec3};

use super::*;
use crate::history::{EditorObject, stable_fingerprint};

#[test]
fn reparent_preserves_world_transform_and_history_undo_restores() {
    let mut document = fixture_document();
    let mut history = UndoRedoStack::new(4).unwrap();
    let before = stable_fingerprint(&document).unwrap();

    let report = apply_scene_composition_operation(
        &mut document,
        SceneCompositionOperation::SetParent {
            child_id: "unit.knight".to_string(),
            parent_id: Some("doodad.castle".to_string()),
            preserve_world_transform: true,
        },
        Some(&mut history),
    )
    .unwrap();

    assert_eq!(report.operation, "scene.reparent");
    assert!(report.history_recorded);
    assert_eq!(
        document.objects["unit.knight"].parent.as_deref(),
        Some("doodad.castle")
    );
    assert_vec3(
        document.objects["unit.knight"].transform.translation,
        Vec3::new(2.0, 0.0, 2.0),
    );
    let knight = report.graph_readback.object("unit.knight").unwrap();
    assert_eq!(knight.parent_object_id.as_deref(), Some("doodad.castle"));
    assert_vec3(
        Vec3::from_array(knight.global_translation),
        Vec3::new(12.0, 0.0, 2.0),
    );
    let torch = report.graph_readback.object("doodad.torch").unwrap();
    assert_vec3(
        Vec3::from_array(torch.global_translation),
        Vec3::new(12.0, 0.0, 5.0),
    );

    let undo_label = history.undo(&mut document).unwrap().unwrap();
    assert_eq!(undo_label, "scene.reparent");
    assert_eq!(stable_fingerprint(&document).unwrap(), before);
    assert_eq!(
        document.objects["unit.knight"].parent.as_deref(),
        Some("world")
    );
}

#[test]
fn scene_graph_readback_uses_engine_parenting_order() {
    let document = fixture_document();
    let graph = build_scene_graph_readback(&document).unwrap();

    assert_eq!(graph.node_count, 4);
    assert_eq!(graph.root_count, 1);
    assert_eq!(graph.propagation.visited_nodes, 4);
    assert_eq!(graph.propagation.recomputed_nodes, 4);
    let world = graph.object("world").unwrap();
    assert_eq!(world.child_object_ids, vec!["doodad.castle", "unit.knight"]);
    let knight = graph.object("unit.knight").unwrap();
    assert_eq!(knight.child_object_ids, vec!["doodad.torch"]);
}

#[test]
fn invalid_reparent_edges_fail_without_mutating_document() {
    let mut missing_parent = fixture_document();
    let before_missing = stable_fingerprint(&missing_parent).unwrap();
    let error = apply_scene_composition_operation(
        &mut missing_parent,
        SceneCompositionOperation::SetParent {
            child_id: "unit.knight".to_string(),
            parent_id: Some("missing.parent".to_string()),
            preserve_world_transform: true,
        },
        None,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.scene_composition.parent_id")
    );
    assert_eq!(stable_fingerprint(&missing_parent).unwrap(), before_missing);

    let mut cycle = fixture_document();
    cycle.objects.get_mut("unit.knight").unwrap().parent = Some("doodad.castle".to_string());
    let before_cycle = stable_fingerprint(&cycle).unwrap();
    let error = apply_scene_composition_operation(
        &mut cycle,
        SceneCompositionOperation::SetParent {
            child_id: "doodad.castle".to_string(),
            parent_id: Some("unit.knight".to_string()),
            preserve_world_transform: false,
        },
        None,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.scene_composition.parent.cycle")
    );
    assert_eq!(stable_fingerprint(&cycle).unwrap(), before_cycle);

    let mut noop = fixture_document();
    let before_noop = stable_fingerprint(&noop).unwrap();
    let error = apply_scene_composition_operation(
        &mut noop,
        SceneCompositionOperation::SetParent {
            child_id: "unit.knight".to_string(),
            parent_id: Some("world".to_string()),
            preserve_world_transform: true,
        },
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("already under"));
    assert_eq!(stable_fingerprint(&noop).unwrap(), before_noop);
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new(
        "calyx://world/scene-composition-tests",
        "scene-composition-rev-1",
    )
    .unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "doodad.castle",
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "unit.knight",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(12.0, 0.0, 2.0)),
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "doodad.torch",
            "Doodad",
            Some("unit.knight".to_string()),
            Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
        ))
        .unwrap();
    document
}

fn assert_vec3(actual: Vec3, expected: Vec3) {
    assert!((actual.x - expected.x).abs() < 0.0001);
    assert!((actual.y - expected.y).abs() < 0.0001);
    assert!((actual.z - expected.z).abs() < 0.0001);
}
