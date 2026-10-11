use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        HarnessSandboxStatus, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, RUN_SANDBOXES_TOOL,
        call_mcp_harness_tool, read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1170_sandbox_batch_persists_isolated_parallel_results() {
    let root = test_root("happy");
    clean_dir(&root);
    let project_path = root.join("sandbox-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    let run = create_source_run(
        &root,
        &state_path,
        &aster_dir,
        &project_path,
        "happy-run",
        1,
    );
    let source_hash = run.state_readback.as_ref().unwrap().state.runs[0].final_state_hash;

    let before = state_summary(&state_path, &aster_dir);
    println!(
        "issue1170 happy before {}",
        serde_json::to_string(&before).unwrap()
    );
    let batch = call_mcp_harness_tool(
        RUN_SANDBOXES_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "happy-sandbox-ledger",
            json!({
                "sandbox_batch_id": "issue1170-happy",
                "run_id": "happy-run",
                "sandboxes": [
                    {"sandbox_id": "east", "ticks": 4, "velocity_x_milli": 1000, "velocity_z_milli": 0},
                    {"sandbox_id": "north", "ticks": 4, "velocity_x_milli": 0, "velocity_z_milli": 1000},
                    {"sandbox_id": "west", "ticks": 4, "velocity_x_milli": -1000, "velocity_z_milli": 0}
                ]
            }),
        ),
    )
    .unwrap();
    let record = &batch.state_readback.as_ref().unwrap().state.sandbox_batches[0];
    assert_eq!(record.completed_count, 3);
    assert_eq!(record.failed_count, 0);
    assert!(record.completed_final_hashes_unique);
    assert!(record.sandboxes.iter().all(|sandbox| {
        sandbox.status == HarnessSandboxStatus::Completed
            && sandbox.initial_state_hash == source_hash
            && sandbox.order_count == 1
            && sandbox.final_tick == Some(5)
    }));
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1170-happy-sandbox-ledger")
        .unwrap()
        .expect("sandbox ledger must exist in Aster");
    assert_eq!(aster.sha256, batch.ledger_readback.as_ref().unwrap().sha256);
    let after = state_summary(&state_path, &aster_dir);
    println!(
        "issue1170 happy after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1170_sandbox_edges_cover_single_max_failure_and_invalid_empty() {
    let root = test_root("edges");
    clean_dir(&root);
    let project_path = root.join("sandbox-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    create_source_run(&root, &state_path, &aster_dir, &project_path, "edge-run", 1);

    let one = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "one-ledger",
        json!({
            "sandbox_batch_id": "issue1170-one",
            "run_id": "edge-run",
            "sandboxes": [
                {"sandbox_id": "single", "ticks": 1, "velocity_x_milli": 250, "velocity_z_milli": 0}
            ]
        }),
    );
    let one_record = &one.state_readback.as_ref().unwrap().state.sandbox_batches[0];
    assert_eq!(one_record.requested_count, 1);
    assert_eq!(one_record.completed_count, 1);

    let max = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "max-ledger",
        json!({
            "sandbox_batch_id": "issue1170-max",
            "run_id": "edge-run",
            "sandboxes": max_sandboxes()
        }),
    );
    let max_record = &max.state_readback.as_ref().unwrap().state.sandbox_batches[1];
    assert_eq!(max_record.requested_count, 8);
    assert_eq!(max_record.completed_count, 8);
    assert!(max_record.completed_final_hashes_unique);

    let partial_before = state_summary(&state_path, &aster_dir);
    let partial = run_sandboxes(
        &root,
        &state_path,
        &aster_dir,
        "partial-ledger",
        json!({
            "sandbox_batch_id": "issue1170-partial",
            "run_id": "edge-run",
            "sandboxes": [
                {"sandbox_id": "good", "ticks": 2, "velocity_x_milli": 750, "velocity_z_milli": 0},
                {"sandbox_id": "bad-target", "target_body_id": 999, "ticks": 2, "velocity_x_milli": 750, "velocity_z_milli": 0}
            ]
        }),
    );
    let partial_record = partial
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .sandbox_batches
        .iter()
        .find(|batch| batch.sandbox_batch_id == "issue1170-partial")
        .unwrap();
    assert_eq!(partial_record.completed_count, 1);
    assert_eq!(partial_record.failed_count, 1);
    assert!(partial_record.sandboxes.iter().any(|sandbox| {
        sandbox.status == HarnessSandboxStatus::Failed
            && sandbox.sandbox_id == "bad-target"
            && sandbox
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("sim body 999")
    }));
    let partial_after = state_summary(&state_path, &aster_dir);
    println!(
        "issue1170 partial before={} after={}",
        partial_before, partial_after
    );

    let invalid_before = state_summary(&state_path, &aster_dir);
    let invalid = call_mcp_harness_tool(
        RUN_SANDBOXES_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "empty-ledger",
            json!({
                "sandbox_batch_id": "issue1170-empty",
                "run_id": "edge-run",
                "sandboxes": []
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let invalid_after = state_summary(&state_path, &aster_dir);
    assert_eq!(invalid_before, invalid_after);
    println!(
        "issue1170 invalid_empty before={} after={} error={}",
        invalid_before, invalid_after, invalid
    );

    let read = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path }))
        .expect("state readback must work");
    assert_eq!(
        read.state_readback
            .as_ref()
            .unwrap()
            .state
            .sandbox_batches
            .len(),
        3
    );
}

fn run_sandboxes(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> crate::mcp_harness_tools::McpHarnessToolCallReadback {
    call_mcp_harness_tool(
        RUN_SANDBOXES_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )
    .unwrap()
}

fn create_source_run(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    project_path: &Path,
    run_id: &str,
    ticks: u64,
) -> crate::mcp_harness_tools::McpHarnessToolCallReadback {
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            root,
            state_path,
            aster_dir,
            &format!("{run_id}-ledger"),
            json!({
                "run_id": run_id,
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": ticks
            }),
        ),
    )
    .unwrap()
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

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value.as_object_mut().unwrap();
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1170-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1170-harness-state"));
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

fn create_project(root: &Path, project_path: &Path) {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1170-project-save",
            "requested_by": "litd-editor-issue1170-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1170-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1170-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1170",
            "title": "Harness Issue 1170",
            "version": "1.0.0",
            "tags": ["agent_harness", "fsv"],
            "project_path": project_path,
            "snapshot_label": "sandbox-source",
            "document": fixture_document()
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1170-sandbox", "issue1170-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(1.0, 0.0, -2.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .expect("real vault must contain Quaternius Rock1");
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
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

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1170_sandbox_batches", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
