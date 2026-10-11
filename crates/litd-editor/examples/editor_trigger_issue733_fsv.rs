use std::{fs, path::Path};

use litd_editor::trigger_editor::{
    TriggerEditorProject, fire_unit_dies, issue733_sample_project, read_trigger_editor_project,
    save_trigger_editor_project, trigger_graph_stable_ron, trigger_graph_summary,
    unit_dies_create_unit_graph,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_TRIGGER_EDITOR_ISSUE733_FSV_ROOT",
        "issue733_trigger_editor",
        "manual",
    )?;
    reset_root(&root)?;

    let project_path = root.join("trigger-editor-project.json");
    let graph_path = root.join("unit-dies-create-unit.ron");
    let report_path = root.join("issue733-trigger-editor-fsv-readback.json");
    let before = file_state(&project_path)?;
    let project = issue733_sample_project()?;
    let save = save_trigger_editor_project(&project_path, &project)?;
    let readback = read_trigger_editor_project(&project_path)?;
    let graph = project
        .graphs
        .first()
        .ok_or("sample project did not build a graph")?;
    let graph_ron = trigger_graph_stable_ron(graph)?;
    fs::write(&graph_path, graph_ron.as_bytes())?;
    let fire = fire_unit_dies(&readback.project, "unit_scout")?;
    let after = file_state(&project_path)?;
    let roundtrip_same_hash = save.sha256 == readback.sha256;
    let roundtrip_same_fingerprint = save.project_fingerprint == readback.project_fingerprint;
    let happy_expected = fire.created_unit_ids == ["unit_reinforcement_guard"]
        && fire.before_unit_count == 1
        && fire.after_unit_count == 2
        && fire.sleep_actions_ms == [250]
        && readback.graph_count == 1
        && readback.event_count == 1
        && readback.condition_count == 1
        && readback.action_count == 2
        && roundtrip_same_hash
        && roundtrip_same_fingerprint;
    if !happy_expected {
        return Err("issue733 trigger editor happy path did not observe expected state".into());
    }

    let false_condition = edge_false_condition(&project)?;
    let unknown_ref = edge_unknown_ref(&project_path, &project)?;
    let sleep_roundtrip = edge_sleep_roundtrip(&root, &project)?;
    let report = json!({
        "schema": "litd.editor.issue733.trigger_editor_fsv.v1",
        "issue": 733,
        "manual_gate": "supporting report; manual FSV must inspect this JSON, graph RON, source stats, and live Triggers HWND screenshot",
        "source_of_truth": {
            "trigger_editor_project": project_path,
            "graph_ron": graph_path,
            "report": report_path
        },
        "states": {
            "before": before,
            "after": after
        },
        "happy_path": {
            "summary": trigger_graph_summary(graph),
            "readback": readback,
            "fire_unit_dies": fire,
            "roundtrip_same_hash": roundtrip_same_hash,
            "roundtrip_same_fingerprint": roundtrip_same_fingerprint,
            "graph_ron_sha256": sha256_file(&graph_path)?,
            "refs_resolved": {
                "event_unit_id": "unit_scout",
                "condition_variable": "reinforcements_enabled",
                "action_unit_type": "unit_guard_type",
                "action_region": "reinforcement_point"
            }
        },
        "edges": {
            "condition_false_action_skipped": false_condition,
            "unknown_ref_rejected": unknown_ref,
            "sleep_action_roundtrips": sleep_roundtrip
        }
    });
    let report_readback = fsv_run.write_json_file(&report_path, &report)?;
    let disk_report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if disk_report != report {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    println!(
        "ISSUE733_TRIGGER_EDITOR_FSV report={} report_sha256={} project_sha256={} graph_ron_sha256={} project_fingerprint={}",
        report_readback.path.display(),
        report_readback.sha256,
        sha256_file(&project_path)?,
        sha256_file(&graph_path)?,
        save.project_fingerprint,
    );
    Ok(())
}

fn edge_false_condition(
    project: &TriggerEditorProject,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut edge = project.clone();
    edge.graphs = vec![unit_dies_create_unit_graph(true, true)];
    edge.variables
        .get_mut("reinforcements_enabled")
        .ok_or("missing reinforcements_enabled variable")?
        .initial_value = "false".to_string();
    edge.validate()?;
    let fire = fire_unit_dies(&edge, "unit_scout")?;
    if fire.fired || fire.after_unit_count != fire.before_unit_count {
        return Err("false condition did not skip trigger actions".into());
    }
    Ok(json!({
        "fired": fire.fired,
        "condition_results": fire.condition_results,
        "actions_executed": fire.actions_executed,
        "before_unit_count": fire.before_unit_count,
        "after_unit_count": fire.after_unit_count
    }))
}

fn edge_unknown_ref(
    project_path: &Path,
    project: &TriggerEditorProject,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = file_state(project_path)?;
    let mut invalid = project.clone();
    invalid.graphs[0].actions[1]
        .args
        .insert("unit_type_id".to_string(), "unit_missing_type".to_string());
    let error = save_trigger_editor_project(project_path, &invalid)
        .unwrap_err()
        .to_string();
    let after = file_state(project_path)?;
    if before["sha256"] != after["sha256"] || !error.contains("unknown unit type") {
        return Err(format!("unknown ref edge did not fail closed: {error}").into());
    }
    Ok(json!({
        "before": before,
        "after": after,
        "unchanged": before["sha256"] == after["sha256"],
        "error": error
    }))
}

fn edge_sleep_roundtrip(
    root: &Path,
    project: &TriggerEditorProject,
) -> Result<Value, Box<dyn std::error::Error>> {
    let path = root.join("sleep-roundtrip-project.json");
    let mut sleeping = project.clone();
    sleeping.graphs = vec![unit_dies_create_unit_graph(true, true)];
    let saved = save_trigger_editor_project(&path, &sleeping)?;
    let readback = read_trigger_editor_project(&path)?;
    let sleep = readback.project.graphs[0].actions[0]
        .args
        .get("duration_ms")
        .cloned()
        .ok_or("sleep duration missing after readback")?;
    if sleep != "250" || saved.sha256 != readback.sha256 {
        return Err("sleep action did not round-trip through save/load".into());
    }
    Ok(json!({
        "path": path,
        "sha256": saved.sha256,
        "readback_sha256": readback.sha256,
        "same_hash": saved.sha256 == readback.sha256,
        "sleep_duration_ms": sleep
    }))
}

fn file_state(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(json!({
        "exists": path.exists(),
        "bytes": fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0),
        "sha256": if path.exists() { sha256_file(path)? } else { String::new() }
    }))
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
