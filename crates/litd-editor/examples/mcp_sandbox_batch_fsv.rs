use std::{error::Error, fs, path::Path};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, RUN_SANDBOXES_TOOL, call_mcp_harness_tool,
        read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let root = Path::new("target")
        .join("fsv")
        .join("issue1170_sandbox_batches")
        .join("manual");
    clean_dir(&root)?;
    let project_path = root.join("sandbox-world.litdworld.json");
    create_project(&root, &project_path)?;
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let source_run = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "source-run-ledger",
            json!({
                "run_id": "issue1170-manual-source",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 1
            }),
        ),
    )?;
    let before = state_summary(&state_path, &aster_dir);
    println!(
        "manual issue1170 before {}",
        serde_json::to_string(&before)?
    );

    let happy = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "happy-ledger",
        json!({
            "sandbox_batch_id": "issue1170-manual-happy",
            "run_id": "issue1170-manual-source",
            "sandboxes": [
                {"sandbox_id": "east", "ticks": 4, "velocity_x_milli": 1000, "velocity_z_milli": 0},
                {"sandbox_id": "north", "ticks": 4, "velocity_x_milli": 0, "velocity_z_milli": 1000},
                {"sandbox_id": "west", "ticks": 4, "velocity_x_milli": -1000, "velocity_z_milli": 0}
            ]
        }),
    )?;
    let single = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "single-ledger",
        json!({
            "sandbox_batch_id": "issue1170-manual-single",
            "run_id": "issue1170-manual-source",
            "sandboxes": [
                {"sandbox_id": "single", "ticks": 1, "velocity_x_milli": 250, "velocity_z_milli": 0}
            ]
        }),
    )?;
    let max = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "max-ledger",
        json!({
            "sandbox_batch_id": "issue1170-manual-max",
            "run_id": "issue1170-manual-source",
            "sandboxes": max_sandboxes()
        }),
    )?;
    let partial = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "partial-ledger",
        json!({
            "sandbox_batch_id": "issue1170-manual-partial",
            "run_id": "issue1170-manual-source",
            "sandboxes": [
                {"sandbox_id": "good", "ticks": 2, "velocity_x_milli": 750, "velocity_z_milli": 0},
                {"sandbox_id": "bad-target", "target_body_id": 999, "ticks": 2, "velocity_x_milli": 750, "velocity_z_milli": 0}
            ]
        }),
    )?;
    let invalid_empty = fail_closed_edge(
        &root,
        &state_path,
        &aster_dir,
        "empty-ledger",
        json!({
            "sandbox_batch_id": "issue1170-manual-empty",
            "run_id": "issue1170-manual-source",
            "sandboxes": []
        }),
    )?;

    let read_harness =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path }))?;
    let state = read_mcp_harness_state(&state_path)?;
    let partial_aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1170-partial-ledger")?
        .ok_or("partial sandbox Aster ledger row missing")?;
    let after = state_summary(&state_path, &aster_dir);

    let report = json!({
        "issue": 1170,
        "source_of_truth": {
            "state_path": state_path,
            "aster_dir": aster_dir,
            "happy_ledger_path": root.join("happy-ledger.json"),
            "single_ledger_path": root.join("single-ledger.json"),
            "max_ledger_path": root.join("max-ledger.json"),
            "partial_ledger_path": root.join("partial-ledger.json")
        },
        "before": before,
        "after": after,
        "source_run": source_run.result["run"],
        "happy": happy.result["sandbox_batch"],
        "single": single.result["sandbox_batch"],
        "max": max.result["sandbox_batch"],
        "partial": partial.result["sandbox_batch"],
        "invalid_empty": invalid_empty,
        "state_readback": {
            "schema": state.state.schema,
            "sha256": state.sha256,
            "run_count": state.run_count,
            "sandbox_batch_count": state.state.sandbox_batches.len(),
            "event_count": state.event_count
        },
        "read_harness": read_harness.result,
        "partial_aster": partial_aster
    });
    let report_path = root.join("manual-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "manual issue1170 after {}",
        serde_json::to_string(&state_summary(&state_path, &aster_dir))?
    );
    println!(
        "manual issue1170 report={} bytes={} state_sha={} report_sha={}",
        report_path.display(),
        fs::metadata(&report_path)?.len(),
        state.sha256,
        sha256_file(&report_path)?
    );
    Ok(())
}

fn run_sandboxes(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> Result<litd_editor::mcp_harness_tools::McpHarnessToolCallReadback, Box<dyn Error>> {
    Ok(call_mcp_harness_tool(
        RUN_SANDBOXES_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )?)
}

fn fail_closed_edge(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> Result<Value, Box<dyn Error>> {
    let before = state_summary(state_path, aster_dir);
    let error = call_mcp_harness_tool(
        RUN_SANDBOXES_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )
    .expect_err("edge must fail closed")
    .to_string();
    let after = state_summary(state_path, aster_dir);
    if before != after {
        return Err(format!("edge {ledger_id} mutated state before failure").into());
    }
    Ok(json!({ "ledger_id": ledger_id, "before": before, "after": after, "error": error }))
}

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value
        .as_object_mut()
        .expect("context target must be object");
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1170-manual"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1170-manual-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1170-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn max_sandboxes() -> Vec<Value> {
    (0..8)
        .map(|index| {
            json!({
                "sandbox_id": format!("max-{index}"),
                "ticks": 2,
                "velocity_x_milli": 100 + index as i64,
                "velocity_z_milli": 25 * index as i64
            })
        })
        .collect()
}

fn create_project(root: &Path, project_path: &Path) -> Result<(), Box<dyn Error>> {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1170-manual-project-save",
            "requested_by": "litd-editor-issue1170-manual",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1170-manual-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1170-manual-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1170_manual",
            "title": "Harness Issue 1170 Manual",
            "version": "1.0.0",
            "tags": ["agent_harness", "fsv"],
            "project_path": project_path,
            "snapshot_label": "sandbox-source",
            "document": fixture_document()?
        }),
    )?;
    Ok(())
}

fn fixture_document() -> Result<EditorDocument, Box<dyn Error>> {
    let mut document =
        EditorDocument::new("calyx://world/issue1170-sandbox-manual", "issue1170-rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(1.0, 0.0, -2.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or("real vault is missing Quaternius Rock1")?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}

fn state_summary(state_path: &Path, aster_dir: &Path) -> Value {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "sandbox_batch_count": state.as_ref().map(|read| read.state.sandbox_batches.len()).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}

fn clean_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}
