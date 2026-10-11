use std::{fs, path::Path, path::PathBuf};

use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        ASSERT_STATE_TOOL, MCP_HARNESS_STATE_SCHEMA, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL,
        call_mcp_harness_tool, read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};

#[test]
fn issue1171_declarative_assertions_read_real_snapshot_actuals() {
    let root = test_root("actuals");
    clean_dir(&root);
    let project_path = root.join("assertion-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    println!("issue1171 initial {}", state_summary(&state_path));
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "run-ledger",
            json!({
                "run_id": "issue1171-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 8
            }),
        ),
    )
    .unwrap();

    let good_before = state_summary(&state_path);
    let good = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "good-ledger",
            json!({
                "verdict_id": "issue1171-good",
                "run_id": "issue1171-run",
                "assertions": [
                    {"assertion_id": "body-zero-exists", "kind": "body_exists", "body_id": 0},
                    {
                        "assertion_id": "body-zero-x",
                        "kind": "body_scalar_in_range",
                        "body_id": 0,
                        "metric": "position",
                        "axis": "x",
                        "min_milli": 1000,
                        "max_milli": 1000
                    }
                ]
            }),
        ),
    )
    .unwrap();
    assert_eq!(good.result["assertion_verdict"]["verdict"], "passed");
    assert_eq!(
        good.result["assertion_verdict"]["checks"][1]["actual"],
        "final_snapshot.bodies[id=0].position.x=1000milli raw=4294967296"
    );
    println!(
        "issue1171 good before={} after={}",
        good_before,
        state_summary(&state_path)
    );

    let bad = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "bad-ledger",
            json!({
                "verdict_id": "issue1171-bad-range",
                "run_id": "issue1171-run",
                "assertions": [{
                    "assertion_id": "body-zero-x-impossible",
                    "kind": "body_scalar_in_range",
                    "body_id": 0,
                    "metric": "position",
                    "axis": "x",
                    "min_milli": 2000,
                    "max_milli": 3000
                }]
            }),
        ),
    )
    .unwrap();
    assert_eq!(bad.result["assertion_verdict"]["verdict"], "failed");
    assert_eq!(
        bad.result["assertion_verdict"]["checks"][0]["code"],
        "LITD_HARNESS_ASSERT_BODY_SCALAR_OUT_OF_RANGE"
    );

    let empty_before = state_summary(&state_path);
    let empty_error = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "empty-ledger",
            json!({
                "verdict_id": "issue1171-empty",
                "run_id": "issue1171-run",
                "assertions": []
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let empty_after = state_summary(&state_path);
    assert_eq!(empty_before, empty_after);
    println!("issue1171 empty before={empty_before} after={empty_after} error={empty_error}");

    let missing = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "missing-ledger",
            json!({
                "verdict_id": "issue1171-missing-body",
                "run_id": "issue1171-run",
                "assertions": [{
                    "assertion_id": "missing-body-position",
                    "kind": "body_scalar_in_range",
                    "body_id": 999,
                    "metric": "position",
                    "axis": "x",
                    "min_milli": 0,
                    "max_milli": 1
                }]
            }),
        ),
    )
    .unwrap();
    assert_eq!(missing.result["assertion_verdict"]["verdict"], "failed");
    assert_eq!(
        missing.result["assertion_verdict"]["checks"][0]["code"],
        "LITD_HARNESS_ASSERT_BODY_NOT_FOUND"
    );

    let malformed_before = state_summary(&state_path);
    let malformed_error = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "malformed-ledger",
            json!({
                "verdict_id": "issue1171-malformed",
                "run_id": "issue1171-run",
                "assertions": [{
                    "assertion_id": "bad-range-declaration",
                    "kind": "body_scalar_in_range",
                    "body_id": 0,
                    "metric": "position",
                    "axis": "x",
                    "min_milli": 3,
                    "max_milli": 2
                }]
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let malformed_after = state_summary(&state_path);
    assert_eq!(malformed_before, malformed_after);
    println!(
        "issue1171 malformed before={} after={} error={}",
        malformed_before, malformed_after, malformed_error
    );

    let state = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(state.state.schema, MCP_HARNESS_STATE_SCHEMA);
    assert_eq!(state.state.assertions.len(), 3);
    assert_eq!(state.state.assertions[0].passed_count, 2);
    assert_eq!(state.state.assertions[1].failed_count, 1);
    assert_eq!(
        state.state.assertions[2].checks[0].location,
        "final_snapshot.bodies[id=999].position.x"
    );
    assert_eq!(
        state.state.runs[0].final_snapshot.bodies[0]
            .position
            .x
            .raw(),
        4_294_967_296
    );
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1171-missing-ledger")
        .unwrap()
        .expect("missing-body assertion ledger must be persisted to Aster");
    assert_eq!(aster.sha256, missing.ledger_readback.unwrap().sha256);
    let read_harness =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read_harness.state_readback.unwrap().assertion_count, 3);
    println!("issue1171 final {}", state_summary(&state_path));
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
        json!("litd-editor-issue1171-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1171-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1171-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1171-project-save",
            "requested_by": "litd-editor-issue1171-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1171-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1171-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1171",
            "title": "Harness Issue 1171",
            "version": "1.0.0",
            "tags": ["agent_harness", "assertions"],
            "project_path": project_path,
            "snapshot_label": "assertion-source",
            "document": fixture_document()
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1171-harness", "issue1171-rev-1").unwrap();
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

fn state_summary(state_path: &Path) -> String {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "assertion_count": state.as_ref().map(|read| read.assertion_count).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default()
    })
    .to_string()
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1171_assertion_api", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
