use litd_engine::math::{Transform, Vec3};

use super::*;
use crate::history::{EditorObject, stable_fingerprint};

#[test]
fn snapping_transform_gizmo_mutates_document_and_history_undo_restores() {
    let mut document = fixture_document();
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let snap = snap_settings(true, true, true);
    let mut history = UndoRedoStack::new(8).unwrap();

    let move_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(1.24, 0.0, -2.76),
            snap,
        },
        Some(&mut history),
    )
    .unwrap();
    assert_eq!(move_report.operation, "viewport.move");
    assert!(move_report.history_recorded);
    assert_vec3(
        document.objects["unit.knight"].transform.translation,
        Vec3::new(3.0, 0.0, 0.0),
    );

    let rotate_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Rotate {
            axis: ViewportAxis::Y,
            degrees: 44.0,
            pivot: Vec3::ZERO,
            snap,
        },
        Some(&mut history),
    )
    .unwrap();
    assert_vec3(
        document.objects["unit.knight"].transform.translation,
        Vec3::new(2.1213202, 0.0, -2.1213202),
    );
    assert_approx(rotate_report.readback[0].yaw_y_degrees, 45.0);

    let scale_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Scale {
            factors: Vec3::splat(1.23),
            pivot: Vec3::ZERO,
            snap,
        },
        Some(&mut history),
    )
    .unwrap();
    assert_vec3(
        document.objects["unit.knight"].transform.translation,
        Vec3::new(2.6516504, 0.0, -2.6516504),
    );
    assert_vec3(
        document.objects["unit.knight"].transform.scale,
        Vec3::splat(1.25),
    );

    let before_undo = stable_fingerprint(&document).unwrap();
    let undo_label = history.undo(&mut document).unwrap().unwrap();
    assert_eq!(undo_label, "viewport.scale");
    assert_eq!(
        stable_fingerprint(&document).unwrap(),
        scale_report.before_fingerprint
    );
    assert_eq!(
        stable_fingerprint(&document).unwrap(),
        rotate_report.after_fingerprint
    );
    assert_ne!(stable_fingerprint(&document).unwrap(), before_undo);
}

#[test]
fn rubber_band_select_align_distribute_and_duplicate_use_real_objects() {
    let mut document = fixture_document();
    let mut selection = SelectionSet::default();
    let rect =
        ViewportSelectionRect::from_corners(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(9.0, 0.0, 10.0))
            .unwrap();

    let selection_report = select_objects_in_world_rect(
        &document,
        &mut selection,
        rect,
        ViewportSelectionMode::Replace,
    )
    .unwrap();
    assert_eq!(selection_report.hit_object_ids.len(), 3);
    assert_eq!(selection.selected_ids.len(), 3);

    let align_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Align {
            axis: ViewportAxis::X,
            reference: ViewportAlignReference::Coordinate,
            coordinate: Some(12.0),
        },
        None,
    )
    .unwrap();
    assert_eq!(align_report.changed_object_ids.len(), 3);
    for id in &selection.selected_ids {
        assert_approx(document.objects[id].transform.translation.x, 12.0);
    }

    let distribute_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Distribute {
            axis: ViewportAxis::Z,
        },
        None,
    )
    .unwrap();
    assert_eq!(distribute_report.changed_object_ids.len(), 3);
    assert_approx(
        document.objects["doodad.tower"].transform.translation.z,
        1.0,
    );
    assert_approx(document.objects["unit.knight"].transform.translation.z, 5.0);
    assert_approx(document.objects["doodad.tree"].transform.translation.z, 9.0);

    let duplicate_report = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Duplicate {
            id_suffix: "_dup".to_string(),
            offset: Vec3::new(0.49, 0.0, 1.51),
            snap: snap_settings(true, false, false),
        },
        None,
    )
    .unwrap();
    assert_eq!(duplicate_report.created_object_ids.len(), 3);
    assert_eq!(document.objects.len(), 7);
    assert_vec3(
        document.objects["unit.knight_dup"].transform.translation,
        Vec3::new(12.5, 0.0, 6.5),
    );
    assert_eq!(
        document.objects["unit.knight_dup"].parent.as_deref(),
        Some("world")
    );
    assert_vec3(
        document.objects["unit.knight"].transform.translation,
        Vec3::new(12.0, 0.0, 5.0),
    );
}

#[test]
fn invalid_viewport_operations_fail_loud_without_mutating_document() {
    let mut document = fixture_document();
    let before = stable_fingerprint(&document).unwrap();
    let empty_selection = SelectionSet::default();
    let error = apply_viewport_operation(
        &mut document,
        &empty_selection,
        ViewportTransformOperation::Move {
            delta: Vec3::X,
            snap: snap_settings(false, false, false),
        },
        None,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.viewport_transform.selection")
    );
    assert_eq!(stable_fingerprint(&document).unwrap(), before);

    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let error = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Scale {
            factors: Vec3::new(-1.0, 1.0, 1.0),
            pivot: Vec3::ZERO,
            snap: snap_settings(false, false, false),
        },
        None,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.viewport_transform.scale.factors")
    );
    assert_eq!(stable_fingerprint(&document).unwrap(), before);

    selection.select_many(["doodad.tower"]);
    let error = apply_viewport_operation(
        &mut document,
        &selection,
        ViewportTransformOperation::Distribute {
            axis: ViewportAxis::X,
        },
        None,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("editor.viewport_transform.distribute.selection")
    );
    assert_eq!(stable_fingerprint(&document).unwrap(), before);
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/viewport-transform-tests", "viewport-rev-1").unwrap();
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
            "unit.knight",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(2.0, 0.0, 3.0)),
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "doodad.tower",
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(8.0, 0.0, 1.0)),
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "doodad.tree",
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(4.0, 0.0, 9.0)),
        ))
        .unwrap();
    document
}

fn snap_settings(grid_enabled: bool, rotation_enabled: bool, scale_enabled: bool) -> SnapSettings {
    SnapSettings {
        grid_enabled,
        grid_size: 0.5,
        rotation_enabled,
        rotation_degrees: 15.0,
        scale_enabled,
        scale_step: 0.25,
    }
}

fn assert_vec3(actual: Vec3, expected: Vec3) {
    assert_approx(actual.x, expected.x);
    assert_approx(actual.y, expected.y);
    assert_approx(actual.z, expected.z);
}

fn assert_approx(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected {expected}, got {actual}"
    );
}
