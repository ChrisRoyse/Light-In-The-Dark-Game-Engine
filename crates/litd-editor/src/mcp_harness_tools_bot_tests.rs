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
        DRIVE_BOT_TOOL, HarnessBotOutcomeStatus, RUN_HEADLESS_TOOL, call_mcp_harness_tool,
        read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1174_virtual_player_bots_record_metrics_trajectory_and_replay() {
    let root = test_root("outcomes");
    clean_dir(&root);
    let project_path = root.join("bot-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    println!(
        "issue1174 outcomes before {}",
        state_summary(&state_path, &aster_dir)
    );

    run_headless(&root, &state_path, &aster_dir, &project_path);
    let clear = drive_bot(
        &root,
        &state_path,
        &aster_dir,
        "clear-ledger",
        json!({
            "bot_run_id": "bot-clear",
            "run_id": "issue1174-source-run",
            "bot_id": "lowered-agent-clear",
            "driver_kind": "heuristic",
            "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 1800},
            "expected_outcome": "completed",
            "ticks": 8,
            "sample_interval_ticks": 1,
            "velocity_x_milli": 60000,
            "velocity_z_milli": 0
        }),
    );
    assert_eq!(clear.result["bot_run"]["status"], "completed");
    assert_eq!(clear.result["bot_run"]["verdict"], "passed");

    let replay = drive_bot(
        &root,
        &state_path,
        &aster_dir,
        "replay-ledger",
        json!({
            "bot_run_id": "bot-replay",
            "run_id": "issue1174-source-run",
            "bot_id": "lowered-agent-replay",
            "driver_kind": "calyx_agent_lowered",
            "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 1800},
            "expected_outcome": "completed",
            "ticks": 8,
            "sample_interval_ticks": 1,
            "velocity_x_milli": 60000,
            "velocity_z_milli": 0
        }),
    );
    assert_eq!(
        replay.result["bot_run"]["trajectory_hash"],
        clear.result["bot_run"]["trajectory_hash"]
    );
    assert_eq!(
        replay.result["bot_run"]["final_state_hash"],
        clear.result["bot_run"]["final_state_hash"]
    );

    drive_bot(
        &root,
        &state_path,
        &aster_dir,
        "unwinnable-ledger",
        json!({
            "bot_run_id": "bot-unwinnable",
            "run_id": "issue1174-source-run",
            "bot_id": "lowered-agent-unwinnable",
            "driver_kind": "behavior_tree",
            "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 2000},
            "expected_outcome": "unwinnable",
            "ticks": 4,
            "sample_interval_ticks": 1,
            "velocity_x_milli": -1000,
            "velocity_z_milli": 0
        }),
    );
    drive_bot(
        &root,
        &state_path,
        &aster_dir,
        "timeout-ledger",
        json!({
            "bot_run_id": "bot-timeout",
            "run_id": "issue1174-source-run",
            "bot_id": "lowered-agent-timeout",
            "driver_kind": "finite_state_machine",
            "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 5000},
            "expected_outcome": "timeout",
            "ticks": 1,
            "sample_interval_ticks": 1,
            "velocity_x_milli": 1000,
            "velocity_z_milli": 0
        }),
    );

    let readback = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(readback.state.bot_runs.len(), 4);
    let clear_record = bot(&readback.state.bot_runs, "bot-clear");
    assert_eq!(clear_record.status, HarnessBotOutcomeStatus::Completed);
    assert_eq!(clear_record.verdict, "passed");
    assert!(clear_record.replay_verified);
    assert!(clear_record.metrics.final_x_milli >= 1800);
    assert!(clear_record.metrics.objective_progress_milli > 0);
    assert!(clear_record.trajectory_sample_count >= 2);
    assert_eq!(
        clear_record.trajectory.last().unwrap().state_hash,
        clear_record.final_state_hash
    );
    assert_eq!(
        bot(&readback.state.bot_runs, "bot-replay").trajectory_hash,
        clear_record.trajectory_hash
    );
    assert_eq!(
        bot(&readback.state.bot_runs, "bot-unwinnable").status,
        HarnessBotOutcomeStatus::Unwinnable
    );
    assert_eq!(
        bot(&readback.state.bot_runs, "bot-timeout").status,
        HarnessBotOutcomeStatus::Timeout
    );
    assert_feedback_code(
        &readback.state.events,
        "bot-unwinnable",
        "LITD_HARNESS_BOT_UNWINNABLE",
    );
    assert_feedback_code(
        &readback.state.events,
        "bot-timeout",
        "LITD_HARNESS_BOT_TIMEOUT",
    );
    for ledger_id in [
        "issue1174-clear-ledger",
        "issue1174-replay-ledger",
        "issue1174-unwinnable-ledger",
        "issue1174-timeout-ledger",
    ] {
        let aster = read_mcp_harness_aster_ledger(&aster_dir, ledger_id)
            .unwrap()
            .unwrap();
        assert!(aster.bytes > 0);
        assert_eq!(aster.sha256.len(), 64);
    }
    println!(
        "issue1174 outcomes after {}",
        state_summary(&state_path, &aster_dir)
    );
}

#[test]
fn issue1174_bot_driver_invalid_target_fails_without_partial_write() {
    let root = test_root("invalid-target");
    clean_dir(&root);
    let project_path = root.join("bot-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    run_headless(&root, &state_path, &aster_dir, &project_path);
    let before = read_mcp_harness_state(&state_path).unwrap();
    println!(
        "issue1174 invalid-target before {}",
        state_summary(&state_path, &aster_dir)
    );

    let error = call_mcp_harness_tool(
        DRIVE_BOT_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "invalid-target-ledger",
            json!({
                "bot_run_id": "bot-invalid-target",
                "run_id": "issue1174-source-run",
                "bot_id": "lowered-agent-invalid",
                "driver_kind": "heuristic",
                "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 1800},
                "expected_outcome": "completed",
                "target_body_id": 999,
                "ticks": 8,
                "sample_interval_ticks": 1,
                "velocity_x_milli": 60000,
                "velocity_z_milli": 0
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("editor.mcp_harness_tools.bot.target_body_id"));
    let after = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(after.state.bot_runs.len(), before.state.bot_runs.len());
    assert_eq!(after.state.update_count, before.state.update_count);
    assert!(!root.join("invalid-target-ledger.json").exists());
    println!(
        "issue1174 invalid-target after {} error={}",
        state_summary(&state_path, &aster_dir),
        error
    );
}

fn run_headless(root: &Path, state_path: &Path, aster_dir: &Path, project_path: &Path) {
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        json_with_context(
            root,
            state_path,
            aster_dir,
            "source-run-ledger",
            json!({
                "run_id": "issue1174-source-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 0
            }),
        ),
    )
    .unwrap();
}

fn drive_bot(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> crate::mcp_harness_tools::McpHarnessToolCallReadback {
    call_mcp_harness_tool(
        DRIVE_BOT_TOOL,
        json_with_context(root, state_path, aster_dir, ledger_id, payload),
    )
    .unwrap()
}

fn json_with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> Value {
    let mut value = payload;
    let object = value.as_object_mut().unwrap();
    object.insert(
        "request_id".to_string(),
        json!(format!("issue1174-{ledger_id}")),
    );
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1174-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1174-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1174-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1174-project-save",
            "requested_by": "litd-editor-issue1174-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1174-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1174-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1174",
            "title": "Harness Issue 1174",
            "version": "1.0.0",
            "tags": ["agent_harness", "virtual_player", "fsv"],
            "project_path": project_path,
            "snapshot_label": "bot-source",
            "document": fixture_document()
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1174-bots", "issue1174-rev-1").unwrap();
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
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
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
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
}

fn bot<'a>(
    records: &'a [crate::mcp_harness_tools::HarnessBotDriveRecord],
    id: &str,
) -> &'a crate::mcp_harness_tools::HarnessBotDriveRecord {
    records
        .iter()
        .find(|record| record.bot_run_id == id)
        .unwrap()
}

fn assert_feedback_code(
    events: &[crate::mcp_harness_tools::HarnessEventRecord],
    affected_id: &str,
    code: &str,
) {
    assert!(events.iter().any(|event| {
        event.affected_ids.iter().any(|id| id == affected_id)
            && event.feedback.iter().any(|feedback| feedback.code == code)
    }));
}

fn state_summary(state_path: &Path, aster_dir: &Path) -> String {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "bot_run_count": state.as_ref().map(|read| read.state.bot_runs.len()).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
    .to_string()
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1174_virtual_player_bots", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
