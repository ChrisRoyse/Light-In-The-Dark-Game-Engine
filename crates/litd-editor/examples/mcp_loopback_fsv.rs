use std::{fs, path::Path};

use litd_editor::{
    mcp_loopback::{TOOLS_CALL_METHOD, TOOLS_LIST_METHOD, handle_mcp_loopback_json},
    mcp_world_tools::{
        PAINT_TILES_TOOL, read_mcp_world_document, read_mcp_world_tool_ledger,
        sample_issue918_document,
    },
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue1453Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_LOOPBACK_FSV_ROOT",
        "issue1453_mcp_loopback",
        "manual",
    )?;
    reset_root(&root)?;
    let document_path = root.join("world-document.json");
    let ledger_path = root.join("world-ledger.json");
    let report_path = root.join("mcp-loopback-fsv-readback.json");
    let before = state(&document_path, &ledger_path);

    let list = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1453-list",
        "method": TOOLS_LIST_METHOD,
        "params": {}
    }))
    .unwrap();
    ensure_success("tools/list", &list)?;

    let paint_call = handle_mcp_loopback_json(call_request(
        "issue1453-paint",
        PAINT_TILES_TOOL,
        paint_args(&document_path, &ledger_path),
    ))
    .unwrap();
    ensure_success("paint tiles tools/call", &paint_call)?;

    let physical_readback = read_mcp_world_document(&document_path)?;
    let after = state(&document_path, &ledger_path);
    let edges = vec![
        edge_unknown_tool(&document_path, &ledger_path)?,
        edge_bad_arguments(&document_path, &ledger_path)?,
        edge_bad_version(&document_path, &ledger_path)?,
    ];
    let fsv = Issue1453Fsv {
        issue: 1453,
        source_of_truth: json!({
            "world_document": document_path,
            "world_ledger": ledger_path,
            "report": report_path
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "tools_list_count": list.result.as_ref().unwrap()["tool_count"],
            "paint_call": paint_call,
            "physical_world_document_readback": physical_readback,
            "painted_cell_tile": physical_readback
                .document
                .terrain_cells
                .get("terrain_cell_1_1")
                .map(|cell| cell.tile_id.clone())
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    if read_json::<Issue1453Fsv>(&report_path)? != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue1453 mcp_loopback_fsv report={} report_sha256={} world_sha256={} ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        read_mcp_world_document(&document_path)?.sha256,
        read_mcp_world_tool_ledger(&ledger_path)?.sha256,
    );
    Ok(())
}

fn edge_unknown_tool(
    document_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state(document_path, ledger_path);
    let response = handle_mcp_loopback_json(call_request(
        "issue1453-unknown-tool",
        "litd.editor.world.not_registered",
        json!({}),
    ))
    .unwrap();
    edge_report(
        "unknown_tool",
        before,
        state(document_path, ledger_path),
        response,
    )
}

fn edge_bad_arguments(
    document_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state(document_path, ledger_path);
    let response = handle_mcp_loopback_json(call_request(
        "issue1453-bad-arguments",
        PAINT_TILES_TOOL,
        json!({}),
    ))
    .unwrap();
    edge_report(
        "bad_arguments",
        before,
        state(document_path, ledger_path),
        response,
    )
}

fn edge_bad_version(
    document_path: &Path,
    ledger_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = state(document_path, ledger_path);
    let response = handle_mcp_loopback_json(json!({
        "jsonrpc": "1.0",
        "id": "issue1453-bad-version",
        "method": TOOLS_LIST_METHOD,
        "params": {}
    }))
    .unwrap();
    edge_report(
        "bad_jsonrpc_version",
        before,
        state(document_path, ledger_path),
        response,
    )
}

fn edge_report(
    name: &str,
    before: Value,
    after: Value,
    response: litd_editor::mcp_loopback::McpLoopbackJsonRpcResponse,
) -> Result<Value, Box<dyn std::error::Error>> {
    if response.error.is_none() {
        return Err(format!("edge {name} unexpectedly succeeded").into());
    }
    if before != after {
        return Err(format!("edge {name} changed world state").into());
    }
    Ok(json!({
        "name": name,
        "state_unchanged": true,
        "before": before,
        "after": after,
        "error": response.error
    }))
}

fn call_request(id: &str, tool_name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": TOOLS_CALL_METHOD,
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    })
}

fn paint_args(document_path: &Path, ledger_path: &Path) -> Value {
    json!({
        "document_path": document_path,
        "ledger_path": ledger_path,
        "ledger_id": "issue1453-world-ledger",
        "request_id": "issue1453-paint",
        "requested_by": "litd-editor-issue1453-fsv",
        "initial_document": sample_issue918_document(),
        "brush": paint_brush(),
        "pathing": {
            "cell_ids": ["terrain_cell_1_1", "terrain_cell_1_2"],
            "mode": "Blocked"
        }
    })
}

fn paint_brush() -> TerrainBrushSpec {
    TerrainBrushSpec {
        center_tile_x: 1,
        center_tile_z: 1,
        size_tiles: 2,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::PaintTile {
            tile_id: "tile.ashen_path".to_string(),
        },
    }
}

fn state(document_path: &Path, ledger_path: &Path) -> Value {
    let readback = if document_path.exists() {
        Some(read_mcp_world_document(document_path).unwrap())
    } else {
        None
    };
    json!({
        "document_exists": document_path.exists(),
        "document_bytes": fs::metadata(document_path).map(|metadata| metadata.len()).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0),
        "terrain_cell_count": readback.as_ref().map(|readback| readback.terrain_cell_count).unwrap_or(0),
        "pathing_override_count": readback.as_ref().map(|readback| readback.pathing_override_count).unwrap_or(0),
        "sha256": readback.as_ref().map(|readback| readback.sha256.clone()).unwrap_or_default()
    })
}

fn ensure_success(
    name: &str,
    response: &litd_editor::mcp_loopback::McpLoopbackJsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(error) = &response.error {
        return Err(format!("{name} failed: {error:?}").into());
    }
    Ok(())
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    let bytes = serde_json::to_vec_pretty(value)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}
