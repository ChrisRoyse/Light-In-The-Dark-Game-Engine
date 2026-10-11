use std::fs;

use litd_editor::mcp_asset_tools::{
    CONFORM_INGEST_BUDGET_TOOL, GENERATE_IMPORT_ASSET_TOOL, READ_ASSETS_TOOL,
    RIG_TEXTURE_ANIMATE_TOOL, call_mcp_asset_tool, read_mcp_asset_state,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue925Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_ASSET_TOOLS_FSV_ROOT",
        "issue925_mcp_asset_tools",
        "manual",
    )?;

    let state_path = root.join("asset-state.json");
    let asset_ledger = root.join("asset-ledger.json");
    let prep_ledger = root.join("prep-ledger.json");
    let budget_ledger = root.join("budget-ledger.json");
    let report_path = root.join("mcp-asset-tools-fsv-readback.json");
    let before = state(&state_path, &asset_ledger);

    let asset = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": asset_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-asset-ledger",
            "request_id": "issue925-asset",
            "requested_by": "litd-editor-issue925-fsv",
            "asset": sample_asset()
        }),
    )?;
    let prep = call_mcp_asset_tool(
        RIG_TEXTURE_ANIMATE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": prep_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-prep-ledger",
            "request_id": "issue925-prep",
            "requested_by": "litd-editor-issue925-fsv",
            "preparation": sample_preparation()
        }),
    )?;
    let budget = call_mcp_asset_tool(
        CONFORM_INGEST_BUDGET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": budget_ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-budget-ledger",
            "request_id": "issue925-budget",
            "requested_by": "litd-editor-issue925-fsv",
            "budget": sample_budget()
        }),
    )?;
    let read = call_mcp_asset_tool(READ_ASSETS_TOOL, json!({ "state_path": state_path }))?;
    let after = state(&state_path, &asset_ledger);
    let edges = vec![
        edge_bad_sha(&root, &state_path)?,
        edge_missing_preparation_asset(&root)?,
        edge_over_budget(&root, &state_path)?,
    ];
    let fsv = Issue925Fsv {
        issue: 925,
        source_of_truth: json!({
            "asset_state": state_path,
            "asset_ledger": asset_ledger,
            "prep_ledger": prep_ledger,
            "budget_ledger": budget_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "generate_import_asset": asset,
            "rig_texture_animate": prep,
            "conform_ingest_budget": budget,
            "read_assets": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    if read_json::<Issue925Fsv>(&report_path)? != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue925 mcp_asset_tools_fsv report={} report_sha256={} state_sha256={} asset_ledger_sha256={} prep_ledger_sha256={} budget_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&state_path)?,
        sha256_file(&asset_ledger)?,
        sha256_file(&prep_ledger)?,
        sha256_file(&budget_ledger)?,
    );
    Ok(())
}

fn edge_bad_sha(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-bad-sha-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-bad-sha",
            "request_id": "issue925-bad-sha",
            "requested_by": "litd-editor-issue925-fsv",
            "asset": bad_sha_asset()
        }),
    )
    .unwrap_err();
    edge_report("bad_content_sha", before, state(state_path, &ledger), error)
}

fn edge_missing_preparation_asset(
    root: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let state_path = root.join("edge-missing-prep-state.json");
    let ledger = root.join("edge-missing-prep-ledger.json");
    let before = state(&state_path, &ledger);
    let error = call_mcp_asset_tool(
        RIG_TEXTURE_ANIMATE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue925-edge-state",
            "ledger_id": "issue925-missing-prep",
            "request_id": "issue925-missing-prep",
            "requested_by": "litd-editor-issue925-fsv",
            "preparation": sample_preparation()
        }),
    )
    .unwrap_err();
    edge_report(
        "missing_asset_for_preparation",
        before,
        state(&state_path, &ledger),
        error,
    )
}

fn edge_over_budget(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-over-budget-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_asset_tool(
        CONFORM_INGEST_BUDGET_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue925-asset-state",
            "ledger_id": "issue925-over-budget",
            "request_id": "issue925-over-budget",
            "requested_by": "litd-editor-issue925-fsv",
            "budget": over_budget()
        }),
    )
    .unwrap_err();
    edge_report(
        "over_render_budget",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_report(
    name: &str,
    before: Value,
    after: Value,
    error: litd_engine::EngineError,
) -> Result<Value, Box<dyn std::error::Error>> {
    if before != after {
        return Err(format!("edge {name} changed source-of-truth state").into());
    }
    Ok(json!({
        "name": name,
        "state_unchanged": true,
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
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

fn state(state_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
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

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    let bytes = serde_json::to_vec_pretty(value)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
