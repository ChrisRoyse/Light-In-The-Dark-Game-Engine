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
        ASSERT_STATE_TOOL, EMIT_FEEDBACK_TOOL, MCP_HARNESS_STATE_SCHEMA, READ_EVENTS_TOOL,
        RUN_HEADLESS_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
        read_mcp_harness_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1172_feedback_stream_persists_actionable_records_and_edges() {
    let root = test_root("feedback");
    clean_dir(&root);
    let project_path = root.join("harness-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    println!(
        "issue1172 before {}",
        state_summary(&state_path, &aster_dir)
    );
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "run-ledger",
            json!({
                "run_id": "issue1172-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 8
            }),
        ),
    )
    .unwrap();

    let assertion = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "assert-ledger",
            json!({
                "verdict_id": "issue1172-assert",
                "run_id": "issue1172-run",
                "assertions": [
                    {"assertion_id": "bad-final-tick", "kind": "final_tick_equals", "expected_u64": 999},
                    {"assertion_id": "missing-body", "kind": "body_exists", "body_id": 999},
                    {
                        "assertion_id": "bad-body-x",
                        "kind": "body_scalar_in_range",
                        "body_id": 0,
                        "metric": "position",
                        "axis": "x",
                        "min_milli": 999000,
                        "max_milli": 1000000
                    }
                ]
            }),
        ),
    )
    .unwrap();
    let assert_event = assertion
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .events
        .last()
        .unwrap();
    let assert_codes = assert_event
        .feedback
        .iter()
        .map(|record| record.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        assert_codes,
        vec![
            "LITD_HARNESS_ASSERT_VALUE_MISMATCH",
            "LITD_HARNESS_ASSERT_BODY_NOT_FOUND",
            "LITD_HARNESS_ASSERT_BODY_SCALAR_OUT_OF_RANGE"
        ]
    );
    assert_actionable_feedback(&assert_event.feedback);

    let explicit = call_mcp_harness_tool(
        EMIT_FEEDBACK_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "feedback-ledger",
            json!({
                "feedback_id": "issue1172-explicit",
                "feedback": [
                    {
                        "code": "LITD_HARNESS_EDGE_DUPLICATE",
                        "message": "first duplicate edge",
                        "remediation": "keep the first unique feedback event and drop repeated identical payloads",
                        "actual": "duplicate_count=2",
                        "expected": "duplicate_count=1",
                        "location": "feedback.edge.duplicate"
                    },
                    {
                        "code": "LITD_HARNESS_EDGE_DUPLICATE",
                        "message": "first duplicate edge",
                        "remediation": "keep the first unique feedback event and drop repeated identical payloads",
                        "actual": "duplicate_count=2",
                        "expected": "duplicate_count=1",
                        "location": "feedback.edge.duplicate"
                    },
                    {
                        "code": "",
                        "message": "",
                        "remediation": "",
                        "actual": "",
                        "expected": "",
                        "location": ""
                    },
                    {
                        "code": "LITD_HARNESS_EDGE_ORDER",
                        "message": "ordering edge",
                        "remediation": "preserve source order after dedupe so agents can apply fixes deterministically",
                        "actual": "order=third",
                        "expected": "order=third",
                        "location": "feedback.edge.order"
                    }
                ]
            }),
        ),
    )
    .unwrap();
    let explicit_event = explicit
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .events
        .last()
        .unwrap();
    assert_eq!(explicit_event.feedback.len(), 3);
    assert_eq!(
        explicit_event
            .feedback
            .iter()
            .map(|record| (record.stream_index, record.code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "LITD_HARNESS_EDGE_DUPLICATE"),
            (2, "LITD_HARNESS_UNKNOWN_FAILURE"),
            (3, "LITD_HARNESS_EDGE_ORDER")
        ]
    );
    assert_actionable_feedback(&explicit_event.feedback);

    let events = call_mcp_harness_tool(
        READ_EVENTS_TOOL,
        json!({
            "state_path": state_path,
            "after_event_index": 0,
            "limit": 10
        }),
    )
    .unwrap();
    assert_eq!(events.result["events"].as_array().unwrap().len(), 3);

    let state = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(state.state.schema, MCP_HARNESS_STATE_SCHEMA);
    assert_eq!(state.event_count, 3);
    assert_eq!(state.state.events[1].feedback.len(), 3);
    assert_eq!(state.state.events[2].feedback.len(), 3);

    let ledger = read_mcp_harness_ledger(root.join("feedback-ledger.json")).unwrap();
    assert_eq!(ledger.ledger.records[0].event.feedback.len(), 3);
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1172-feedback-ledger")
        .unwrap()
        .expect("feedback ledger should be present in Aster");
    assert_eq!(aster.sha256, ledger.sha256);
    assert!(aster.graph_rows >= 2);
    println!(
        "issue1172 after {} assert_codes={:?} explicit_sha={} aster_rows={}",
        state_summary(&state_path, &aster_dir),
        assert_codes,
        ledger.sha256,
        aster.graph_rows
    );
}

fn assert_actionable_feedback(records: &[crate::mcp_harness_tools::HarnessFeedbackRecord]) {
    for record in records {
        assert!(!record.code.trim().is_empty());
        assert!(!record.message.trim().is_empty());
        assert!(!record.remediation.trim().is_empty());
        assert!(!record.actual.trim().is_empty());
        assert!(!record.expected.trim().is_empty());
        assert!(!record.location.trim().is_empty());
        assert!(record.source_of_truth.contains("Aster"));
    }
}

fn json_with_context(
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
        json!("litd-editor-issue1172-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1172-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1172-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1172-project-save",
            "requested_by": "litd-editor-issue1172-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1172-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1172-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1172",
            "title": "Harness Issue 1172",
            "version": "1.0.0",
            "tags": ["agent_harness", "feedback"],
            "project_path": project_path,
            "snapshot_label": "feedback-source",
            "document": fixture_document()
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1172-harness", "issue1172-rev-1").unwrap();
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

fn state_summary(state_path: &Path, aster_dir: &Path) -> String {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
    .to_string()
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1172_feedback_stream", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
