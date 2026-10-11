use std::path::Path;

use litd_editor::{
    history::{EditorObject, SelectionSet, UndoRedoStack, stable_fingerprint},
    mcp_render_tools::{
        COMPOSE_SCENE_TOOL, READ_RENDER_TOOL, RenderSceneComposition, RenderSceneLayer,
        RenderScenePlacement, call_mcp_render_tool, read_mcp_render_state,
    },
    scene_composition::{
        SceneCompositionOperation, apply_scene_composition_operation,
        apply_scene_composition_operation_in_world_record, build_scene_graph_readback,
    },
    shell::SnapSettings,
    viewport_transform::{
        ViewportAxis, ViewportTransformOperation, apply_viewport_operation_in_world_record,
    },
    world_creation::{
        EditorWorldCreationSpec, EditorWorldRecord, WorldAsterReadback, create_editor_world_record,
        parse_world_record, read_world_record, validate_world_record, write_world_record,
    },
};
use litd_engine::{
    fsv_artifacts::begin_issue_run,
    math::{Transform, Vec3},
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue870_scene_composition")?;
    let report_path = run.report_path();
    let root = report_path.parent().ok_or("report path has no parent")?;
    let aster_dir = root.join("world-aster");
    let mcp_state = root.join("mcp-render-state.json");
    let mcp_ledger = root.join("mcp-render-ledger.json");

    let happy = happy_path(root, &aster_dir)?;
    let mcp = mcp_parented_scene(&mcp_state, &mcp_ledger)?;
    let edges = vec![
        edge_missing_parent()?,
        edge_parent_cycle()?,
        edge_zero_snapped_move()?,
        edge_mcp_parent_cycle(&mcp_state, &root.join("mcp-edge-parent-cycle.json"))?,
    ];

    let report = json!({
        "schema": "litd.issue870.scene_composition_fsv.v1",
        "issue": 870,
        "source_of_truth": {
            "world_record": "Calyx Aster world record row read back and parsed after mutation",
            "engine_graph": "Transient litd_engine::scene::SceneGraph built from parsed EditorDocument and propagated",
            "mcp_render_state": "Calyx Aster MCP render state row read back through litd.engine.render.read_render",
            "manual_fsv": "Open this report through Synapse, inspect fields under happy_path/mcp_shared_capability/edges, and compare expected_actuals."
        },
        "visual_references_reviewed": [
            "docs/readme/ED01-editor-shell.png",
            "docs/readme/ED02-terrain-editor.png",
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/04-editor.png",
            "docs/readme/ED06-material-assets.png"
        ],
        "happy_path": happy,
        "mcp_shared_capability": mcp,
        "edges": edges,
        "expected_actuals": {
            "snapped_translation": [13.0, 0.0, 1.5],
            "snapped_yaw_degrees": 45.0,
            "snapped_scale": [1.25, 1.25, 1.25],
            "parent_after_reparent": "doodad.castle",
            "local_translation_after_reparent": [3.0, 0.0, 1.5],
            "preserved_global_translation": [13.0, 0.0, 1.5],
            "mcp_parented_placements": 1,
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue870_scene_composition_fsv report={} bytes={} sha256={} parent={} global={:?} mcp_parented={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["after"]["readback_object"]["parent"],
        readback.json["happy_path"]["after"]["graph_object"]["global_translation"],
        readback.json["mcp_shared_capability"]["read_result"]["scene_parented_placement_count"],
        readback.json["edges"]
            .as_array()
            .map(|edges| edges.len())
            .unwrap_or(0),
    );
    Ok(())
}

fn happy_path(root: &Path, aster_dir: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("happy")?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let snap = snap_settings();
    let mut history = UndoRedoStack::new(8)?;
    let before = object_snapshot(&record, "unit.knight")?;

    let move_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(1.24, 0.0, -0.74),
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
            pivot: Vec3::new(13.0, 0.0, 1.5),
            snap,
        },
        Some(&mut history),
    )?;
    let scale_report = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Scale {
            factors: Vec3::splat(1.23),
            pivot: Vec3::new(13.0, 0.0, 1.5),
            snap,
        },
        Some(&mut history),
    )?;
    let reparent_report = apply_scene_composition_operation_in_world_record(
        &mut record,
        SceneCompositionOperation::SetParent {
            child_id: "unit.knight".to_string(),
            parent_id: Some("doodad.castle".to_string()),
            preserve_world_transform: true,
        },
        Some(&mut history),
    )?;
    let aster = write_and_read_world(aster_dir, &record)?;
    let readback = parse_world_record(&aster.bytes)?;
    let graph = build_scene_graph_readback(&readback.document)?;
    let graph_object = graph.object("unit.knight").ok_or("missing graph object")?;
    let history_undo_label = history.undo(&mut record.document)?;
    Ok(json!({
        "before": {
            "object": before,
            "selection": selection.summary(),
            "snap": {"grid": 0.5, "rotation_degrees": 15.0, "scale_step": 0.25}
        },
        "after": {
            "move_report": move_report,
            "rotate_report": rotate_report,
            "scale_report": scale_report,
            "reparent_report": reparent_report,
            "aster": readback_summary(&aster),
            "artifact_root": root.display().to_string(),
            "readback_object": object_snapshot(&readback, "unit.knight")?,
            "graph": graph,
            "graph_object": graph_object,
            "history_undo_label": history_undo_label
        }
    }))
}

fn mcp_parented_scene(
    state_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let write = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue870-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue870-scene-ledger",
            "request_id": "issue870-compose-parented-scene",
            "requested_by": "litd-editor-issue870-fsv",
            "composition": sample_mcp_scene()
        }),
    )?;
    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path }))?;
    let state = read_mcp_render_state(state_path)?;
    Ok(json!({
        "write_result": write.result,
        "read_result": read.result,
        "state_sha256": state.sha256,
        "parented_placements": state.scene_parented_placement_count,
        "placements": state.state.scene.as_ref().unwrap().placements
    }))
}

fn edge_missing_parent() -> Result<Value, Box<dyn std::error::Error>> {
    let mut document = fixture_record("edge-missing")?.document;
    let before = stable_fingerprint(&document)?;
    let error = apply_scene_composition_operation(
        &mut document,
        SceneCompositionOperation::SetParent {
            child_id: "unit.knight".to_string(),
            parent_id: Some("missing.parent".to_string()),
            preserve_world_transform: true,
        },
        None,
    )
    .unwrap_err();
    Ok(edge_state(
        "missing_parent",
        before,
        stable_fingerprint(&document)?,
        error,
    ))
}

fn edge_parent_cycle() -> Result<Value, Box<dyn std::error::Error>> {
    let mut document = fixture_record("edge-cycle")?.document;
    document.objects.get_mut("unit.knight").unwrap().parent = Some("doodad.castle".to_string());
    let before = stable_fingerprint(&document)?;
    let error = apply_scene_composition_operation(
        &mut document,
        SceneCompositionOperation::SetParent {
            child_id: "doodad.castle".to_string(),
            parent_id: Some("unit.knight".to_string()),
            preserve_world_transform: false,
        },
        None,
    )
    .unwrap_err();
    Ok(edge_state(
        "parent_cycle",
        before,
        stable_fingerprint(&document)?,
        error,
    ))
}

fn edge_zero_snapped_move() -> Result<Value, Box<dyn std::error::Error>> {
    let mut record = fixture_record("edge-zero-snap")?;
    let before = stable_fingerprint(&record.document)?;
    let mut selection = SelectionSet::default();
    selection.select_many(["unit.knight"]);
    let error = apply_viewport_operation_in_world_record(
        &mut record,
        &selection,
        ViewportTransformOperation::Move {
            delta: Vec3::new(0.24, 0.0, 0.0),
            snap: snap_settings(),
        },
        None,
    )
    .unwrap_err();
    Ok(edge_state(
        "zero_snapped_move",
        before,
        stable_fingerprint(&record.document)?,
        error,
    ))
}

fn edge_mcp_parent_cycle(
    state_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_render_state(state_path)?.sha256;
    let mut scene = sample_mcp_scene();
    scene.placements[0].parent_placement_id = Some("placement.issue870.knight".to_string());
    let error = call_mcp_render_tool(
        COMPOSE_SCENE_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue870-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue870-edge-parent-cycle",
            "request_id": "issue870-edge-parent-cycle",
            "requested_by": "litd-editor-issue870-fsv",
            "composition": scene
        }),
    )
    .unwrap_err();
    let after = read_mcp_render_state(state_path)?.sha256;
    Ok(edge_state("mcp_parent_cycle", before, after, error))
}

fn fixture_record(id: &str) -> Result<EditorWorldRecord, Box<dyn std::error::Error>> {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue870-{id}");
    spec.revision_id = format!("issue870-{id}-rev-1");
    spec.properties.display_name = format!("Issue 870 Scene Composition {id}");
    let mut record = create_editor_world_record(spec)?;
    record.source_issue = 870;
    record.document.upsert_object(EditorObject::new(
        "doodad.castle",
        "Doodad",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
    ))?;
    record.document.upsert_object(EditorObject::new(
        "unit.knight",
        "Unit",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(12.0, 0.0, 2.0)),
    ))?;
    record.document.upsert_object(EditorObject::new(
        "doodad.torch",
        "Doodad",
        Some("unit.knight".to_string()),
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
    ))?;
    refresh_record(&mut record)?;
    Ok(record)
}

fn sample_mcp_scene() -> RenderSceneComposition {
    RenderSceneComposition {
        scene_id: "scene.issue870.parented_layout".to_string(),
        source_scene_id: "editor.document.issue870".to_string(),
        camera_mode: "bird_eye_editor".to_string(),
        layers: vec![RenderSceneLayer {
            layer_id: "layer.layout".to_string(),
            kind: "level_layout".to_string(),
            visible: true,
        }],
        placements: vec![
            RenderScenePlacement {
                placement_id: "placement.issue870.castle".to_string(),
                asset_id: "doodad.castle".to_string(),
                layer_id: "layer.layout".to_string(),
                parent_placement_id: None,
                translation: Vec3::new(10.0, 0.0, 0.0),
                rotation_y_rad: 0.0,
                scale: Vec3::ONE,
            },
            RenderScenePlacement {
                placement_id: "placement.issue870.knight".to_string(),
                asset_id: "unit.knight".to_string(),
                layer_id: "layer.layout".to_string(),
                parent_placement_id: Some("placement.issue870.castle".to_string()),
                translation: Vec3::new(3.0, 0.0, 1.5),
                rotation_y_rad: 45.0_f32.to_radians(),
                scale: Vec3::splat(1.25),
            },
        ],
        material_assignments: Vec::new(),
    }
}

fn refresh_record(record: &mut EditorWorldRecord) -> Result<(), Box<dyn std::error::Error>> {
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.summary = format!(
        "world={} issue870 objects={} fingerprint={}",
        record.spec.properties.display_name,
        record.document.objects.len(),
        record.document_fingerprint
    );
    validate_world_record(record)?;
    Ok(())
}

fn write_and_read_world(
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<WorldAsterReadback, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn object_snapshot(
    record: &EditorWorldRecord,
    id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let object = record
        .document
        .objects
        .get(id)
        .ok_or_else(|| format!("missing object {id}"))?;
    Ok(json!({
        "object_id": id,
        "parent": object.parent,
        "translation": round_vec3(object.transform.translation),
        "yaw_y_degrees": round4((2.0 * object.transform.rotation.y.atan2(object.transform.rotation.w)).to_degrees()),
        "scale": round_vec3(object.transform.scale)
    }))
}

fn snap_settings() -> SnapSettings {
    SnapSettings {
        grid_enabled: true,
        grid_size: 0.5,
        rotation_enabled: true,
        rotation_degrees: 15.0,
        scale_enabled: true,
        scale_step: 0.25,
    }
}

fn edge_state(name: &str, before: String, after: String, error: impl std::fmt::Display) -> Value {
    json!({
        "name": name,
        "before": before,
        "after": after,
        "unchanged": before == after,
        "error": error.to_string()
    })
}

fn readback_summary(readback: &WorldAsterReadback) -> Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "bytes": readback.bytes.len(),
        "sha256": readback.sha256,
        "cell_key": readback.cell_key,
        "index_key": readback.index_key
    })
}

fn round_vec3(value: Vec3) -> [f32; 3] {
    [round4(value.x), round4(value.y), round4(value.z)]
}

fn round4(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}

fn assert_report(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report["issue"], 870);
    assert_eq!(
        report["happy_path"]["after"]["move_report"]["readback"][0]["translation"],
        report["expected_actuals"]["snapped_translation"]
    );
    let actual_yaw = report["happy_path"]["after"]["rotate_report"]["readback"][0]["yaw_y_degrees"]
        .as_f64()
        .ok_or("rotate readback yaw_y_degrees was not numeric")?;
    let expected_yaw = report["expected_actuals"]["snapped_yaw_degrees"]
        .as_f64()
        .ok_or("expected snapped_yaw_degrees was not numeric")?;
    if (actual_yaw - expected_yaw).abs() > 1.0e-4 {
        return Err(
            format!("snapped yaw mismatch: expected {expected_yaw}, got {actual_yaw}").into(),
        );
    }
    assert_eq!(
        report["happy_path"]["after"]["scale_report"]["readback"][0]["scale"],
        report["expected_actuals"]["snapped_scale"]
    );
    assert_eq!(
        report["happy_path"]["after"]["readback_object"]["parent"],
        report["expected_actuals"]["parent_after_reparent"]
    );
    assert_eq!(
        report["happy_path"]["after"]["readback_object"]["translation"],
        report["expected_actuals"]["local_translation_after_reparent"]
    );
    assert_eq!(
        report["happy_path"]["after"]["readback_object"]["yaw_y_degrees"],
        report["expected_actuals"]["snapped_yaw_degrees"]
    );
    assert_eq!(
        report["happy_path"]["after"]["readback_object"]["scale"],
        report["expected_actuals"]["snapped_scale"]
    );
    assert_eq!(
        report["happy_path"]["after"]["graph_object"]["global_translation"],
        report["expected_actuals"]["preserved_global_translation"]
    );
    assert_eq!(
        report["mcp_shared_capability"]["read_result"]["scene_parented_placement_count"],
        1
    );
    assert_eq!(report["edges"].as_array().unwrap().len(), 4);
    assert!(
        report["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|edge| edge["unchanged"] == true)
    );
    Ok(())
}
