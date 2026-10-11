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
        HarnessRunMode, HarnessRunRenderMode, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL,
        call_mcp_harness_tool, read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1169_headless_fixed_until_and_zero_tick_runs_persist_expected_state() {
    let root = test_root("runs");
    clean_dir(&root);
    let project_path = root.join("headless-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let before = state_summary(&state_path, &aster_dir);
    println!(
        "issue1169 before {}",
        serde_json::to_string(&before).unwrap()
    );

    let fixed = run_headless(
        &root,
        &state_path,
        &aster_dir,
        "fixed-ledger",
        json!({
            "run_id": "issue1169-fixed",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 8
        }),
    );
    assert_eq!(fixed.state_readback.as_ref().unwrap().run_count, 1);
    let fixed_record = &fixed.state_readback.as_ref().unwrap().state.runs[0];
    assert_eq!(fixed_record.run_mode, HarnessRunMode::FixedTicks);
    assert_eq!(
        fixed_record.render_mode,
        HarnessRunRenderMode::HeadlessNoRender
    );
    assert_eq!(fixed_record.termination_reason, "ticks_completed");
    assert_eq!(fixed_record.final_tick, 8);
    assert_eq!(fixed_record.steps, 8);
    assert_eq!(fixed_record.bodies_updated, 8);
    assert_eq!(
        fixed_record.final_snapshot.state_hash,
        fixed_record.final_state_hash
    );
    assert!(fixed.ledger_readback.as_ref().unwrap().aster.graph_rows >= 2);

    let until = run_headless(
        &root,
        &state_path,
        &aster_dir,
        "until-ledger",
        json!({
            "run_id": "issue1169-until",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 64,
            "until_condition": {"kind": "tick_at_least", "tick": 5}
        }),
    );
    let until_record = &until.state_readback.as_ref().unwrap().state.runs[1];
    assert_eq!(until_record.run_mode, HarnessRunMode::UntilCondition);
    assert_eq!(until_record.termination_reason, "until_condition_satisfied");
    assert_eq!(until_record.final_tick, 5);
    assert_eq!(until_record.steps, 5);
    assert!(until_record.until_condition.is_some());

    let zero = run_headless(
        &root,
        &state_path,
        &aster_dir,
        "zero-ledger",
        json!({
            "run_id": "issue1169-zero",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 0
        }),
    );
    let zero_record = &zero.state_readback.as_ref().unwrap().state.runs[2];
    assert_eq!(zero_record.final_tick, 0);
    assert_eq!(zero_record.steps, 0);
    assert_eq!(zero_record.bodies_updated, 0);
    assert_eq!(zero_record.initial_state_hash, zero_record.final_state_hash);

    let read = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path }))
        .expect("state readback must work after writes");
    let state = &read.state_readback.as_ref().unwrap().state;
    assert_eq!(state.runs.len(), 3);
    assert_eq!(state.events.len(), 3);
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1169-zero-ledger")
        .unwrap()
        .expect("zero ledger must exist in Aster");
    assert_eq!(aster.sha256, zero.ledger_readback.as_ref().unwrap().sha256);

    let after = state_summary(&state_path, &aster_dir);
    println!("issue1169 after {}", serde_json::to_string(&after).unwrap());
}

#[test]
fn issue1169_headless_edges_fail_without_partial_state_writes() {
    let root = test_root("edges");
    clean_dir(&root);
    let project_path = root.join("headless-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let timeout_before = state_summary(&state_path, &aster_dir);
    let timeout = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "timeout-ledger",
            json!({
                "run_id": "issue1169-timeout",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 3,
                "until_condition": {"kind": "tick_at_least", "tick": 9}
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let timeout_after = state_summary(&state_path, &aster_dir);
    assert_eq!(timeout_before, timeout_after);
    println!(
        "issue1169 timeout before={} after={} error={}",
        timeout_before, timeout_after, timeout
    );

    let missing_before = state_summary(&state_path, &aster_dir);
    let missing_project = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "missing-project-ledger",
            json!({
                "run_id": "issue1169-missing-project",
                "project_path": root.join("missing.litdworld.json"),
                "camera_rig": "TopDown",
                "ticks": 1
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let missing_after = state_summary(&state_path, &aster_dir);
    assert_eq!(missing_before, missing_after);
    println!(
        "issue1169 missing_project before={} after={} error={}",
        missing_before, missing_after, missing_project
    );

    let invalid_before = state_summary(&state_path, &aster_dir);
    let invalid_condition = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "invalid-condition-ledger",
            json!({
                "run_id": "issue1169-invalid-condition",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 1,
                "until_condition": {"kind": "body_count_at_least", "body_count": 17}
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let invalid_after = state_summary(&state_path, &aster_dir);
    assert_eq!(invalid_before, invalid_after);
    println!(
        "issue1169 invalid_condition before={} after={} error={}",
        invalid_before, invalid_after, invalid_condition
    );
}

fn run_headless(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> crate::mcp_harness_tools::McpHarnessToolCallReadback {
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )
    .unwrap()
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
        json!("litd-editor-issue1169-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1169-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1169-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1169-project-save",
            "requested_by": "litd-editor-issue1169-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1169-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1169-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1169",
            "title": "Harness Issue 1169",
            "version": "1.0.0",
            "tags": ["agent_harness", "fsv"],
            "project_path": project_path,
            "snapshot_label": "headless-source",
            "document": fixture_document()
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1169-headless", "issue1169-rev-1").unwrap();
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
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1169_headless_runner", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
