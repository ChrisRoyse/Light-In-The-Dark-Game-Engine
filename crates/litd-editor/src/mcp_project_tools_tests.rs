use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::HarnessOracleKind,
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, PACKAGE_LITDGAME_TOOL, ProjectLifecycleAction, READ_PROJECT_TOOL,
        RUN_HEADLESS_PLAYTEST_TOOL, call_mcp_project_tool, read_mcp_project_state,
    },
    play::PlayCameraRig,
};
use litd_engine::math::{Transform, Vec3};

#[path = "mcp_project_tools_tests_part2.rs"]
mod part2;

#[test]
fn issue926_project_mcp_tools_save_playtest_package_and_read_real_state() {
    let root = test_root("unit");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("ashen-reach.litdgame.json");
    let document = fixture_document();

    let before = state_summary(&state_path, &project_path, &package_path);
    println!(
        "issue926 before {}",
        serde_json::to_string(&before).unwrap()
    );

    let saved = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, document.clone()),
    )
    .unwrap();
    assert_eq!(saved.state_readback.project_count, 1);
    assert!(project_path.exists());

    let playtest = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-playtest",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("playtest-ledger.json"),
            "ledger_id": "issue926-playtest-ledger",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 128
        }),
    )
    .unwrap();
    assert_eq!(playtest.state_readback.playtest_count, 1);

    let package = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue926-package",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("package-ledger.json"),
            "ledger_id": "issue926-package-ledger",
            "project_path": project_path,
            "package_path": package_path,
            "share_id": "issue926-share",
            "created_by": "agent.project.builder",
            "host_port": 19260,
            "max_package_bytes": 262144
        }),
    )
    .unwrap();
    assert_eq!(package.state_readback.package_count, 1);
    assert!(package_path.exists());

    let read =
        call_mcp_project_tool(READ_PROJECT_TOOL, json!({ "state_path": state_path })).unwrap();
    let state = read.state_readback.state;
    let project = &state.projects[0];
    let play = &state.playtests[0];
    let package = &state.packages[0];

    assert_eq!(project.project_id, "project.ashen_reach_issue926");
    assert_eq!(project.title, "Ashen Reach Issue 926");
    assert_eq!(project.unit_count, 1);
    assert_eq!(play.camera_rig, PlayCameraRig::TopDown);
    assert_eq!(play.start_tick, 0);
    assert_eq!(play.final_tick, 128);
    assert_eq!(play.steps, 128);
    assert_eq!(play.spawned_bodies, 1);
    assert_eq!(play.verdict, "passed");
    assert_eq!(play.oracle_verdict.failed_count, 0);
    assert_eq!(play.oracle_verdict.check_count, 9);
    assert!(play.verdict_reason.contains("all 9"));
    assert_eq!(package.project_id, "project.ashen_reach_issue926");
    assert!(package.world_hash_matches);
    assert!(package.runtime_lock_verified);
    assert_eq!(package.asset_count, 1);

    let after = state_summary(&state_path, &project_path, &package_path);
    println!("issue926 after {}", serde_json::to_string(&after).unwrap());
}

#[test]
fn issue926_project_mcp_tools_edges_fail_closed_without_writing() {
    let root = test_root("edges");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("tiny.litdgame.json");
    let document = fixture_document();

    let invalid_before = state_summary(&state_path, &project_path, &package_path);
    let invalid_title = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue926-bad-title",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("bad-title-ledger.json"),
            "ledger_id": "issue926-bad-title-ledger",
            "action": "save_snapshot",
            "project_id": "project.ashen_reach_issue926",
            "title": "",
            "version": "1.0.0",
            "tags": ["dark_fantasy"],
            "project_path": project_path,
            "snapshot_label": "invalid",
            "document": document
        }),
    )
    .unwrap_err()
    .to_string();
    let invalid_after = state_summary(&state_path, &project_path, &package_path);
    assert_eq!(invalid_before, invalid_after);
    println!(
        "issue926 invalid_title before={} after={} error={}",
        invalid_before, invalid_after, invalid_title
    );

    let missing_before = state_summary(&state_path, &project_path, &package_path);
    let missing_project = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-missing-project",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("missing-ledger.json"),
            "ledger_id": "issue926-missing-ledger",
            "project_path": root.join("missing.litdworld.json"),
            "camera_rig": "FirstPerson",
            "ticks": 64
        }),
    )
    .unwrap_err()
    .to_string();
    let missing_after = state_summary(&state_path, &project_path, &package_path);
    assert_eq!(missing_before, missing_after);
    println!(
        "issue926 missing_project before={} after={} error={}",
        missing_before, missing_after, missing_project
    );

    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, fixture_document()),
    )
    .unwrap();

    let zero_before = state_summary(&state_path, &project_path, &package_path);
    let zero_ticks = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-zero-ticks",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("zero-ledger.json"),
            "ledger_id": "issue926-zero-ledger",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 0
        }),
    )
    .unwrap_err()
    .to_string();
    let zero_after = state_summary(&state_path, &project_path, &package_path);
    assert_eq!(zero_before, zero_after);
    println!(
        "issue926 zero_ticks before={} after={} error={}",
        zero_before, zero_after, zero_ticks
    );

    let tiny_before = state_summary(&state_path, &project_path, &package_path);
    let tiny_package = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue926-tiny-package",
            "requested_by": "litd-editor-issue926-test",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("tiny-ledger.json"),
            "ledger_id": "issue926-tiny-ledger",
            "project_path": project_path,
            "package_path": package_path,
            "share_id": "issue926-too-large",
            "created_by": "agent.project.builder",
            "host_port": 19261,
            "max_package_bytes": 256
        }),
    )
    .unwrap_err()
    .to_string();
    let tiny_after = state_summary(&state_path, &project_path, &package_path);
    assert_eq!(tiny_before, tiny_after);
    println!(
        "issue926 tiny_package before={} after={} error={}",
        tiny_before, tiny_after, tiny_package
    );
}

#[test]
fn issue1315_headless_playtest_verdict_is_oracle_backed_in_persisted_state() {
    let root = test_root("issue1315-happy");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("unused.litdgame.json");

    let before = state_summary(&state_path, &project_path, &package_path);
    println!(
        "issue1315 happy before {}",
        serde_json::to_string(&before).unwrap()
    );
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, fixture_document()),
    )
    .unwrap();
    call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        playtest_args(
            &state_path,
            &root,
            &project_path,
            "issue1315-happy-playtest",
        ),
    )
    .unwrap();

    let readback = read_mcp_project_state(&state_path).unwrap();
    let playtest = &readback.state.playtests[0];
    assert_eq!(playtest.verdict, "passed");
    assert_eq!(playtest.oracle_verdict.verdict, "passed");
    assert_eq!(playtest.oracle_verdict.check_count, 9);
    assert_eq!(playtest.oracle_verdict.failed_count, 0);
    assert_eq!(playtest.spawned_bodies, 1);
    assert!(playtest.bodies_updated > 0);
    assert_ne!(playtest.initial_state_hash, playtest.final_state_hash);
    assert!(playtest.verdict_reason.contains("all 9"));

    let after = state_summary(&state_path, &project_path, &package_path);
    println!(
        "issue1315 happy after {} playtest={}",
        serde_json::to_string(&after).unwrap(),
        serde_json::to_string(playtest).unwrap()
    );
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue926-save",
        "requested_by": "litd-editor-issue926-test",
        "state_path": state_path,
        "state_id": "issue926-project-state",
        "ledger_path": root.join("project-ledger.json"),
        "ledger_id": "issue926-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue926",
        "title": "Ashen Reach Issue 926",
        "version": "1.0.0",
        "tags": ["dark_fantasy", "playtest", "local_first"],
        "project_path": project_path,
        "snapshot_label": "known-good-save",
        "document": document
    })
}

fn playtest_args(state_path: &Path, root: &Path, project_path: &Path, request_id: &str) -> Value {
    json!({
        "request_id": request_id,
        "requested_by": "litd-editor-issue1315-test",
        "state_path": state_path,
        "state_id": "issue926-project-state",
        "ledger_path": root.join(format!("{request_id}-ledger.json")),
        "ledger_id": format!("{request_id}-ledger"),
        "project_path": project_path,
        "camera_rig": "TopDown",
        "ticks": 128
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue926-project", "issue926-rev-1").unwrap();
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
                Transform::from_translation(Vec3::new(2.0, 0.0, -4.0)),
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

fn with_world_property(mut document: EditorDocument, key: &str, value: &str) -> EditorDocument {
    let mut object = document
        .objects
        .remove("world")
        .expect("fixture document has world object");
    object.properties.insert(key.to_string(), value.to_string());
    document.upsert_object(object).unwrap();
    document
}

fn state_summary(state_path: &Path, project_path: &Path, package_path: &Path) -> Value {
    let state = read_mcp_project_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_file_exists": project_path.exists(),
        "project_file_bytes": project_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "package_file_exists": package_path.exists(),
        "package_file_bytes": package_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_count": state.as_ref().map(|readback| readback.project_count).unwrap_or(0),
        "playtest_count": state.as_ref().map(|readback| readback.playtest_count).unwrap_or(0),
        "package_count": state.as_ref().map(|readback| readback.package_count).unwrap_or(0)
    })
}

fn test_root(label: &str) -> PathBuf {
    // #1518: the core project handlers (new/open/save, playtest, package, read) now
    // confine every tool-supplied path to LITD_EDITOR_PROJECT_ROOT. Wire the trusted
    // root (the workspace) into each test's own setup so it passes in isolation rather
    // than only when another parallel test set the process-global root first.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue926_mcp_project_tools", label)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
