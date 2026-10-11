use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::{EditorObject, SelectionSet, UndoRedoStack, stable_fingerprint},
    shell::SnapSettings,
    viewport_transform::{
        ViewportAlignReference, ViewportAxis, ViewportSelectionMode, ViewportSelectionRect,
        ViewportTransformOperation, apply_viewport_operation_in_world_record,
        select_objects_in_world_rect,
    },
    world_creation::{
        EditorWorldCreationSpec, WorldAsterReadback, create_editor_world_record,
        parse_world_record, read_world_record, validate_world_record, write_world_record,
    },
};
use litd_engine::math::{Transform, Vec3};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_VIEWPORT_TRANSFORM_FSV",
        "issue1215_viewport_transform",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1215 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");

    let happy = happy_path(&root, &aster_dir)?;
    let align_distribute = multi_select_align_distribute(&root, &aster_dir)?;
    let snap_edge = snap_grid_edge(&root, &aster_dir)?;
    let pivot_edge = pivot_rotation_edge(&root, &aster_dir)?;
    let undo_edge = undo_edge(&root, &aster_dir)?;

    let mut snapshot = json!({
        "schema": "litd.issue1215.viewport_transform_fsv.v1",
        "issue": 1215,
        "source_of_truth": "Aster world record readback plus EditorDocument.objects transform snapshots and UndoRedoStack readback",
        "artifact_path": path,
        "before": {"artifact": before_artifact},
        "happy_path": happy,
        "multi_select_align_distribute": align_distribute,
        "edges": [snap_edge, pivot_edge, undo_edge],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote viewport transform FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "happy final {}",
        snapshot["happy_path"]["after"]["readback_object"]
    );
    println!("edge snap before {}", snapshot["edges"][0]["before"]);
    println!("edge snap after {}", snapshot["edges"][0]["after"]);
    println!("edge pivot before {}", snapshot["edges"][1]["before"]);
    println!("edge pivot after {}", snapshot["edges"][1]["after"]);
    println!("edge undo before {}", snapshot["edges"][2]["before"]);
    println!("edge undo after {}", snapshot["edges"][2]["after"]);
    Ok(())
}

fn happy_path(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("issue1215-happy")?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let snap = snap_settings(true, true, true);
    let mut history = UndoRedoStack::new(8)?;
    let initial = object_snapshot(&record, "unit.knight")?;

    let move_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(1.24, 0.0, -2.76),
            snap,
        },
        Some(&mut history),
    )?;
    let rotate_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Rotate {
            axis: ViewportAxis::Y,
            degrees: 44.0,
            pivot: Vec3::ZERO,
            snap,
        },
        Some(&mut history),
    )?;
    let scale_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Scale {
            factors: Vec3::splat(1.23),
            pivot: Vec3::ZERO,
            snap,
        },
        Some(&mut history),
    )?;
    let aster = write_and_read_world(root, aster_dir, &record)?;
    let readback = parse_world_record(&aster.bytes)?;
    Ok(json!({
        "before": {
            "selection": selection.selected_ids.iter().cloned().collect::<Vec<_>>(),
            "object": initial,
            "snap": {"grid": 0.5, "rotation_degrees": 15.0, "scale_step": 0.25}
        },
        "after": {
            "move_report": move_report,
            "rotate_report": rotate_report,
            "scale_report": scale_report,
            "aster": readback_summary(&aster),
            "readback_object": object_snapshot(&readback, "unit.knight")?,
            "expected_translation": [2.6517, 0.0, -2.6517],
            "expected_yaw_y_degrees": 45.0,
            "expected_scale": [1.25, 1.25, 1.25]
        }
    }))
}

fn multi_select_align_distribute(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("issue1215-align-distribute")?;
    let mut selection = SelectionSet::default();
    let rect =
        ViewportSelectionRect::from_corners(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(9.0, 0.0, 10.0))?;
    let selection_report = select_objects_in_world_rect(
        &record.document,
        &mut selection,
        rect,
        ViewportSelectionMode::Replace,
    )?;
    let before = selected_snapshots(&record, &selection)?;
    let align_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Align {
            axis: ViewportAxis::X,
            reference: ViewportAlignReference::Coordinate,
            coordinate: Some(12.0),
        },
        None,
    )?;
    let distribute_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Distribute {
            axis: ViewportAxis::Z,
        },
        None,
    )?;
    let aster = write_and_read_world(root, aster_dir, &record)?;
    let readback = parse_world_record(&aster.bytes)?;
    Ok(json!({
        "before": {"selection_report": selection_report, "objects": before},
        "after": {
            "align_report": align_report,
            "distribute_report": distribute_report,
            "aster": readback_summary(&aster),
            "readback_objects": selected_snapshots(&readback, &selection)?,
            "expected_x": 12.0,
            "expected_z_by_id": {
                "doodad.tower": 1.0,
                "unit.knight": 5.0,
                "doodad.tree": 9.0
            }
        }
    }))
}

fn snap_grid_edge(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("issue1215-edge-snap")?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let before = object_snapshot(&record, "unit.knight")?;
    let report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(2.24, 0.0, -3.26),
            snap: snap_settings(true, false, false),
        },
        None,
    )?;
    let aster = write_and_read_world(root, aster_dir, &record)?;
    let readback = parse_world_record(&aster.bytes)?;
    Ok(json!({
        "name": "snap-to-grid exact",
        "before": {"object": before, "delta": [2.24, 0.0, -3.26], "grid": 0.5},
        "after": {
            "report": report,
            "aster": readback_summary(&aster),
            "readback_object": object_snapshot(&readback, "unit.knight")?,
            "expected_translation": [4.0, 0.0, -0.5]
        }
    }))
}

fn pivot_rotation_edge(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("issue1215-edge-pivot")?;
    record
        .document
        .objects
        .get_mut("unit.knight")
        .unwrap()
        .transform
        .translation = Vec3::new(6.0, 0.0, 0.0);
    refresh_record(&mut record)?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let before = object_snapshot(&record, "unit.knight")?;
    let report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Rotate {
            axis: ViewportAxis::Y,
            degrees: 89.0,
            pivot: Vec3::new(2.0, 0.0, 0.0),
            snap: snap_settings(false, true, false),
        },
        None,
    )?;
    let aster = write_and_read_world(root, aster_dir, &record)?;
    let readback = parse_world_record(&aster.bytes)?;
    Ok(json!({
        "name": "rotate about pivot",
        "before": {"object": before, "pivot": [2.0, 0.0, 0.0], "degrees": 89.0},
        "after": {
            "report": report,
            "aster": readback_summary(&aster),
            "readback_object": object_snapshot(&readback, "unit.knight")?,
            "expected_translation": [2.0, 0.0, -4.0],
            "expected_yaw_y_degrees": 90.0
        }
    }))
}

fn undo_edge(
    root: &PathBuf,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("issue1215-edge-undo")?;
    let before_fingerprint = stable_fingerprint(&record.document)?;
    let before_object = object_snapshot(&record, "unit.knight")?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let mut history = UndoRedoStack::new(4)?;
    let move_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(1.0, 0.0, 1.0),
            snap: snap_settings(false, false, false),
        },
        Some(&mut history),
    )?;
    let moved_aster = write_and_read_world(root, aster_dir, &record)?;
    let moved_readback = parse_world_record(&moved_aster.bytes)?;
    let undo_label = history
        .undo(&mut record.document)?
        .ok_or("expected viewport history entry to undo")?;
    refresh_record(&mut record)?;
    let undo_aster = write_and_read_world(root, aster_dir, &record)?;
    let undo_readback = parse_world_record(&undo_aster.bytes)?;
    Ok(json!({
        "name": "undo restores prior transforms",
        "before": {
            "fingerprint": before_fingerprint,
            "object": before_object,
            "history_depth": 0
        },
        "after_move": {
            "report": move_report,
            "aster": readback_summary(&moved_aster),
            "readback_object": object_snapshot(&moved_readback, "unit.knight")?
        },
        "after": {
            "undo_label": undo_label,
            "aster": readback_summary(&undo_aster),
            "fingerprint": stable_fingerprint(&record.document)?,
            "readback_object": object_snapshot(&undo_readback, "unit.knight")?,
            "matches_before_fingerprint": stable_fingerprint(&record.document)? == before_fingerprint
        }
    }))
}

fn fixture_record(
    id: &str,
) -> Result<litd_editor::world_creation::EditorWorldRecord, Box<dyn std::error::Error>> {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/{id}");
    spec.revision_id = format!("{id}-rev-1");
    spec.properties.display_name = format!("Issue 1215 Viewport Transform {id}");
    let mut record = create_editor_world_record(spec)?;
    record.source_issue = 1215;
    record.document.upsert_object(EditorObject::new(
        "unit.knight",
        "Unit",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(2.0, 0.0, 3.0)),
    ))?;
    record.document.upsert_object(EditorObject::new(
        "doodad.tower",
        "Doodad",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(8.0, 0.0, 1.0)),
    ))?;
    record.document.upsert_object(EditorObject::new(
        "doodad.tree",
        "Doodad",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(4.0, 0.0, 9.0)),
    ))?;
    refresh_record(&mut record)?;
    Ok(record)
}

fn refresh_record(
    record: &mut litd_editor::world_creation::EditorWorldRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.summary = format!(
        "world={} viewport_transform_fixture objects={} fingerprint={}",
        record.spec.properties.display_name,
        record.document.objects.len(),
        record.document_fingerprint
    );
    validate_world_record(record)?;
    Ok(())
}

fn write_and_read_world(
    _root: &PathBuf,
    aster_dir: &Path,
    record: &litd_editor::world_creation::EditorWorldRecord,
) -> Result<WorldAsterReadback, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn selected_snapshots(
    record: &litd_editor::world_creation::EditorWorldRecord,
    selection: &SelectionSet,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    selection
        .selected_ids
        .iter()
        .map(|id| object_snapshot(record, id))
        .collect()
}

fn object_snapshot(
    record: &litd_editor::world_creation::EditorWorldRecord,
    id: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let object = record
        .document
        .objects
        .get(id)
        .ok_or_else(|| format!("missing object {id}"))?;
    Ok(json!({
        "object_id": id,
        "translation": round_vec3(object.transform.translation),
        "yaw_y_degrees": round4((2.0 * object.transform.rotation.y.atan2(object.transform.rotation.w)).to_degrees()),
        "scale": round_vec3(object.transform.scale)
    }))
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

fn round_vec3(value: Vec3) -> [f32; 3] {
    [round4(value.x), round4(value.y), round4(value.z)]
}

fn round4(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "bytes": readback.bytes.len(),
        "cell_key": readback.cell_key,
        "index_key": readback.index_key
    })
}

fn write_document_with_final_state(
    path: &PathBuf,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err(format!(
        "FSV artifact byte-count stabilization failed for {}",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}
