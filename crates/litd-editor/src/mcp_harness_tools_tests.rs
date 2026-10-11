use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        ASSERT_STATE_TOOL, CALYX_SIGNALS_TOOL, DRIVE_BOT_TOOL, READ_EVENTS_TOOL, READ_HARNESS_TOOL,
        RUN_HEADLESS_TOOL, RUN_ORACLES_TOOL, RUN_PEV_TOOL, SCREENSHOT_DIFF_TOOL,
        SNAPSHOT_BRANCH_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
        read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[path = "mcp_harness_tools_tests_part2.rs"]
mod part2;

#[test]
fn issue1178_agent_harness_edges_fail_closed_without_partial_writes() {
    let root = test_root("edges");
    clean_dir(&root);
    let project_path = root.join("harness-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let empty_before = state_summary(&state_path, &aster_dir);
    let empty_assertions = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "empty-assert-ledger",
            json!({
                "verdict_id": "empty",
                "run_id": "missing",
                "assertions": []
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let empty_after = state_summary(&state_path, &aster_dir);
    assert_eq!(empty_before, empty_after);
    println!(
        "issue1178 empty_assertions before={} after={} error={}",
        empty_before, empty_after, empty_assertions
    );

    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "run-ledger",
            json!({
                "run_id": "issue1178-edge-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 4
            }),
        ),
    )
    .unwrap();

    let pev_before = state_summary(&state_path, &aster_dir);
    let failed_pev = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "failed-pev-ledger",
            json!({
                "pev_id": "issue1178-failed-pev",
                "run_id": "issue1178-failed-pev-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 5,
                "assertions": [
                    {"assertion_id": "wrong-final-tick", "kind": "final_tick_equals", "expected_u64": 999}
                ],
                "oracles": ["deterministic_replay"]
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let pev_after = state_summary(&state_path, &aster_dir);
    assert_eq!(pev_before, pev_after);
    println!(
        "issue1178 failed_pev before={} after={} error={}",
        pev_before, pev_after, failed_pev
    );

    let screenshot_before = state_summary(&state_path, &aster_dir);
    let missing_screenshot = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "missing-screenshot-ledger",
            json!({
                "diff_id": "missing-png",
                "baseline_path": real_png_in(&root),
                "candidate_path": root.join("missing.png"),
                "max_changed_bytes": 0
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let screenshot_after = state_summary(&state_path, &aster_dir);
    assert_eq!(screenshot_before, screenshot_after);
    println!(
        "issue1178 missing_screenshot before={} after={} error={}",
        screenshot_before, screenshot_after, missing_screenshot
    );

    let read_limit = call_mcp_harness_tool(
        READ_EVENTS_TOOL,
        json!({
            "state_path": state_path,
            "after_event_index": 0,
            "limit": 0
        }),
    )
    .unwrap_err()
    .to_string();
    let read_after = state_summary(&state_path, &aster_dir);
    assert_eq!(screenshot_after, read_after);
    println!("issue1178 read_limit_error={read_limit}");
}

#[test]
fn issue1518_harness_tool_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the harness family: poisoned paths outside the
    // project root are rejected at the read-handler confinement AND at load_state (the
    // shared choke point for every mutation handler's context paths), before any fs effect.
    let root = test_root("issue1518-confine");
    let project_path = root.join("harness-project.litdworld.json");
    create_project(&root, &project_path);
    let aster_dir = root.join("aster");
    let good_state = root.join("harness-state.json");

    // --- EDGE 1: READ_HARNESS_TOOL with an absolute state_path OUTSIDE the workspace root ---
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-harness");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let escape_state = escape_dir.join("harness-state.json");
    let e1 = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": escape_state }))
        .unwrap_err()
        .to_string();
    println!("issue1518 harness read absolute-escape rejected: {e1}");
    assert!(
        e1.contains("outside the project root"),
        "expected outside-root rejection, got: {e1}"
    );
    assert!(!escape_dir.exists());

    // --- EDGE 2: READ_HARNESS_TOOL with a ..-traversal state_path ---
    let e2 = call_mcp_harness_tool(
        READ_HARNESS_TOOL,
        json!({ "state_path": "hs/../../litd-issue1518-harness-traversal" }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 harness read traversal rejected: {e2}");
    assert!(
        e2.contains("parent-directory"),
        "expected traversal rejection, got: {e2}"
    );

    // --- EDGE 3: a mutation (RUN_HEADLESS) whose context state_path escapes the root is
    // rejected at load_state before any state is written ---
    let run_args = json_with_context(
        &root,
        &escape_state,
        &aster_dir,
        "run-ledger",
        json!({
            "run_id": "issue1518-run",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 4
        }),
    );
    let e3 = call_mcp_harness_tool(RUN_HEADLESS_TOOL, run_args)
        .unwrap_err()
        .to_string();
    println!("issue1518 harness run write-sink escape rejected: {e3}");
    assert!(
        e3.contains("outside the project root"),
        "expected outside-root rejection at load_state, got: {e3}"
    );
    assert!(
        !escape_dir.exists(),
        "confinement breach: harness wrote state outside the project root"
    );
    assert!(
        !good_state.exists(),
        "no in-root state should be written by a rejected mutation"
    );
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
        json!("litd-editor-issue1178-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1178-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1178-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    let document = fixture_document();
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1178-project-save",
            "requested_by": "litd-editor-issue1178-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1178-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1178-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1178",
            "title": "Harness Issue 1178",
            "version": "1.0.0",
            "tags": ["agent_harness", "fsv"],
            "project_path": project_path,
            "snapshot_label": "harness-source",
            "document": document
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1178-harness", "issue1178-rev-1").unwrap();
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
    let aster_exists = aster_dir.exists();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "state_fingerprint": state.as_ref().map(|read| read.state_fingerprint.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_exists
    })
    .to_string()
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1178_mcp_harness_tools", name)
}

/// A real, decodable PNG staged INSIDE the test's confinement root (`root`, the in-tree scratch
/// dir the harness already writes state/ledger/aster into).
///
/// #1765: the screenshot-diff harness tools confine every path argument to the project root
/// (#1518). The old fixture pointed `baseline_path`/`candidate_path` at the out-of-git-tree
/// `local-asset-vault` PNG, which lives beside the PRIMARY checkout -- never inside a linked
/// worktree's confinement root, so the confined tool rejected it from every worktree, and a box
/// that has not materialized the vault could not read it at all. Writing a real PNG into the same
/// in-root scratch dir keeps the path inside the confinement root on ANY checkout while still
/// exercising real PNG decode + pixel diff (the behavior under test); the specific vault asset was
/// incidental to what the screenshot-diff tool proves.
fn real_png_in(root: &Path) -> PathBuf {
    let path = root.join("harness-baseline.png");
    let (width, height) = (8_u32, 8_u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[(x * 24) as u8, (y * 24) as u8, 96, 255]);
        }
    }
    image::save_buffer_with_format(
        &path,
        &rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .expect("stage a real PNG inside the harness test confinement root");
    path
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
