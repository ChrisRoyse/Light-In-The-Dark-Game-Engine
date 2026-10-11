use std::{fs, path::Path};

use serde_json::{Value, json};

use super::*;

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn issue925_asset_mcp_tools_generate_prepare_budget_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue925_mcp_asset_tools", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("asset-state.json");
    let asset_ledger = root.join("asset-ledger.json");
    let prep_ledger = root.join("prep-ledger.json");
    let budget_ledger = root.join("budget-ledger.json");
    let before = state(&state_path, &asset_ledger);
    println!("issue925 before {before}");

    let asset = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": asset_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-asset-ledger",
            "request_id": "issue925-asset",
            "requested_by": "litd-editor-issue925-test",
            "asset": sample_asset()
        }),
    )
    .unwrap();
    assert_eq!(asset.tool_name, GENERATE_IMPORT_ASSET_TOOL);
    let prep = call_mcp_asset_tool(
        RIG_TEXTURE_ANIMATE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": prep_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-prep-ledger",
            "request_id": "issue925-prep",
            "requested_by": "litd-editor-issue925-test",
            "preparation": sample_preparation()
        }),
    )
    .unwrap();
    assert_eq!(prep.tool_name, RIG_TEXTURE_ANIMATE_TOOL);
    let budget = call_mcp_asset_tool(
        CONFORM_INGEST_BUDGET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": budget_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-budget-ledger",
            "request_id": "issue925-budget",
            "requested_by": "litd-editor-issue925-test",
            "budget": sample_budget()
        }),
    )
    .unwrap();
    assert_eq!(budget.tool_name, CONFORM_INGEST_BUDGET_TOOL);
    let read = call_mcp_asset_tool(READ_ASSETS_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["state_readback"]["asset_count"], 1);
    assert_eq!(read.result["state_readback"]["preparation_count"], 1);
    assert_eq!(read.result["state_readback"]["budget_count"], 1);
    let after = state(&state_path, &asset_ledger);
    println!("issue925 after {after}");
    assert_eq!(after["asset_count"], 1);
    assert_eq!(after["preparation_count"], 1);
    assert_eq!(after["budget_count"], 1);
}

#[test]
fn issue1518_asset_tool_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the asset family: a poisoned state_path outside the
    // project root (absolute-escape) and a ..-traversal ledger_path are both rejected at
    // confine_tool_paths BEFORE load_or_create_mcp_asset_state (which would otherwise create
    // state_path) or any ledger write, leaving the filesystem untouched.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1518_mcp_asset_tools", "confine").unwrap();
    reset_root(&root);
    let ledger_path = root.join("issue1518-asset-ledger.json");

    // --- EDGE 1: absolute state_path OUTSIDE the workspace root ---
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-asset");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).unwrap();
    }
    let escape_state = escape_dir.join("asset-state.json");
    let escape_error = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": escape_state,
            "ledger_path": ledger_path,
            "state_id": "issue1518-asset-state",
            "ledger_id": "issue1518-asset-ledger",
            "request_id": "issue1518-escape",
            "requested_by": "litd-editor-issue1518-test",
            "asset": sample_asset()
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 asset absolute-escape rejected: {escape_error}");
    assert!(
        escape_error.contains("outside the project root"),
        "expected outside-root rejection, got: {escape_error}"
    );
    assert!(
        !escape_dir.exists(),
        "confinement breach: asset tool created {} outside the project root",
        escape_dir.display()
    );

    // --- EDGE 2: ..-traversal ledger_path ---
    let traversal_error = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": root.join("asset-state.json"),
            "ledger_path": "issue1518-led/../../litd-issue1518-asset-traversal",
            "state_id": "issue1518-asset-state",
            "ledger_id": "issue1518-asset-ledger",
            "request_id": "issue1518-traversal",
            "requested_by": "litd-editor-issue1518-test",
            "asset": sample_asset()
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 asset traversal rejected: {traversal_error}");
    assert!(
        traversal_error.contains("parent-directory"),
        "expected traversal rejection, got: {traversal_error}"
    );
    assert!(
        !Path::new("../litd-issue1518-asset-traversal").exists(),
        "confinement breach: traversal created a directory outside the workspace"
    );
    // The traversal was rejected before state_path was touched: no asset-state.json written.
    assert!(
        !root.join("asset-state.json").exists(),
        "confinement breach: state written despite a rejected ledger_path"
    );
}

#[test]
fn issue925_asset_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue925_mcp_asset_tools", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("asset-state.json");
    let edge_ledger = root.join("edge-ledger.json");

    let before_bad_sha = state(&state_path, &edge_ledger);
    let bad_sha = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-bad-sha",
            "request_id": "issue925-bad-sha",
            "requested_by": "litd-editor-issue925-test",
            "asset": bad_sha_asset()
        }),
    )
    .unwrap_err();
    let after_bad_sha = state(&state_path, &edge_ledger);
    println!("issue925 bad_sha before={before_bad_sha} after={after_bad_sha} error={bad_sha}");
    assert_eq!(before_bad_sha, after_bad_sha);
    assert!(bad_sha.to_string().contains("SHA-256"));

    let before_missing_asset = state(&state_path, &edge_ledger);
    let missing_asset = call_mcp_asset_tool(
        RIG_TEXTURE_ANIMATE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-missing-asset",
            "request_id": "issue925-missing-asset",
            "requested_by": "litd-editor-issue925-test",
            "preparation": sample_preparation()
        }),
    )
    .unwrap_err();
    let after_missing_asset = state(&state_path, &edge_ledger);
    println!(
        "issue925 missing_asset before={before_missing_asset} after={after_missing_asset} error={missing_asset}"
    );
    assert_eq!(before_missing_asset, after_missing_asset);
    assert!(missing_asset.to_string().contains("missing asset"));

    seed_asset_preparation(&root, &state_path);
    let before_over_budget = state(&state_path, &edge_ledger);
    let over_budget = call_mcp_asset_tool(
        CONFORM_INGEST_BUDGET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-over-budget",
            "request_id": "issue925-over-budget",
            "requested_by": "litd-editor-issue925-test",
            "budget": over_budget()
        }),
    )
    .unwrap_err();
    let after_over_budget = state(&state_path, &edge_ledger);
    println!(
        "issue925 over_budget before={before_over_budget} after={after_over_budget} error={over_budget}"
    );
    assert_eq!(before_over_budget, after_over_budget);
    assert!(over_budget.to_string().contains("triangle"));
}

fn seed_asset_preparation(root: &Path, state_path: &Path) {
    call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-asset-ledger.json"),
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-seed-asset",
            "request_id": "issue925-seed-asset",
            "requested_by": "litd-editor-issue925-test",
            "asset": sample_asset()
        }),
    )
    .unwrap();
    call_mcp_asset_tool(
        RIG_TEXTURE_ANIMATE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-prep-ledger.json"),
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-seed-prep",
            "request_id": "issue925-seed-prep",
            "requested_by": "litd-editor-issue925-test",
            "preparation": sample_preparation()
        }),
    )
    .unwrap();
}

fn sample_asset() -> Value {
    json!({
        "asset_id": "asset.horned_forest_beast",
        "display_name": "Horned Forest Beast",
        "asset_type": "character",
        "source_kind": "generated",
        "prompt": "Horned forest beast with mossy fur, game-ready topology, and blue rune accents.",
        "source_ref": "prompt.horned_forest_beast.v1",
        "content_sha256": SHA_A,
        "license": "project_internal",
        "provenance_refs": ["prompt.horned_forest_beast.v1", "concept.board.forest_beast"]
    })
}

fn sample_preparation() -> Value {
    json!({
        "asset_id": "asset.horned_forest_beast",
        "mesh_ref": "mesh.horned_forest_beast.lod0",
        "rig_ref": "rig.quadruped_beast.v1",
        "texture_refs": ["texture.beast.albedo", "texture.beast.normal"],
        "animation_refs": ["anim.beast.idle", "anim.beast.run"],
        "ready_for_runtime": true
    })
}

fn sample_budget() -> Value {
    json!({
        "asset_id": "asset.horned_forest_beast",
        "triangle_count": 18000,
        "material_count": 3,
        "texture_bytes": 4194304,
        "draw_call_estimate": 4,
        "max_triangle_count": 24000,
        "max_material_count": 4,
        "max_texture_bytes": 8388608,
        "max_draw_calls": 6,
        "calyx_ingest_ref": "calyx.ingest.asset.horned_forest_beast"
    })
}

fn bad_sha_asset() -> Value {
    let mut value = sample_asset();
    value["content_sha256"] = json!("not-a-sha");
    value
}

fn over_budget() -> Value {
    let mut value = sample_budget();
    value["triangle_count"] = json!(48000);
    value
}

fn state(state_path: &Path, ledger_path: &Path) -> Value {
    let readback = if state_path.exists() {
        Some(read_mcp_asset_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "asset_count": readback.as_ref().map(|readback| readback.asset_count).unwrap_or(0),
        "preparation_count": readback.as_ref().map(|readback| readback.preparation_count).unwrap_or(0),
        "budget_count": readback.as_ref().map(|readback| readback.budget_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
