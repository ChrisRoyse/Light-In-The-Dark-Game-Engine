use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::{
    mcp_world_tools::{PAINT_TILES_TOOL, read_mcp_world_document, sample_issue918_document},
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
    },
};

use super::*;

#[test]
fn loopback_lists_callable_tools_and_executes_real_world_mutation() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1453_mcp_loopback", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("world-document.json");
    let ledger_path = root.join("world-ledger.json");
    let before = world_state(&document_path, &ledger_path);
    println!("issue1453 loopback before {before}");

    let list = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1453-list",
        "method": TOOLS_LIST_METHOD,
        "params": {}
    }))
    .unwrap();
    assert!(list.error.is_none(), "{list:?}");
    let list_result = list.result.as_ref().unwrap();
    let registry = default_mcp_tool_registry().unwrap();
    let expected_names = registry
        .entries
        .iter()
        .map(|entry| entry.tool.tool_name.as_str())
        .collect::<Vec<_>>();
    let listed_names = list_result["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        list_result["tool_count"].as_u64(),
        Some(registry.entries.len() as u64)
    );
    assert_eq!(listed_names, expected_names);
    assert!(listed_names.contains(&PAINT_TILES_TOOL));

    let call = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1453-paint",
        "method": TOOLS_CALL_METHOD,
        "params": {
            "name": PAINT_TILES_TOOL,
            "arguments": paint_args(&document_path, &ledger_path)
        }
    }))
    .unwrap();
    assert!(call.error.is_none(), "{call:?}");
    assert_eq!(call.result.as_ref().unwrap()["isError"], false);
    assert_eq!(
        call.result.as_ref().unwrap()["structuredContent"]["tool_name"],
        PAINT_TILES_TOOL
    );

    let readback = read_mcp_world_document(&document_path).unwrap();
    assert_eq!(
        readback.document.terrain_cells["terrain_cell_1_1"].tile_id,
        "tile.ashen_path"
    );
    assert_eq!(readback.pathing_override_count, 2);
    let after = world_state(&document_path, &ledger_path);
    println!("issue1453 loopback after {after}");
    assert_eq!(after["document_exists"], true);
    assert_eq!(after["ledger_exists"], true);
    assert_ne!(before, after);
}

#[test]
fn loopback_edges_fail_closed_without_domain_writes() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1453_mcp_loopback", "edges").unwrap();
    reset_root(&root);
    let document_path = root.join("edge-world-document.json");
    let ledger_path = root.join("edge-world-ledger.json");

    let before_unknown = world_state(&document_path, &ledger_path);
    let unknown = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1453-unknown",
        "method": TOOLS_CALL_METHOD,
        "params": {
            "name": "litd.editor.world.not_registered",
            "arguments": {}
        }
    }))
    .unwrap();
    let after_unknown = world_state(&document_path, &ledger_path);
    println!(
        "issue1453 unknown before={before_unknown} after={after_unknown} response={unknown:?}"
    );
    assert_eq!(before_unknown, after_unknown);
    assert_eq!(unknown.error.unwrap().code, -32602);

    let before_bad_args = world_state(&document_path, &ledger_path);
    let bad_args = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1453-bad-args",
        "method": TOOLS_CALL_METHOD,
        "params": {
            "name": PAINT_TILES_TOOL,
            "arguments": {}
        }
    }))
    .unwrap();
    let after_bad_args = world_state(&document_path, &ledger_path);
    println!(
        "issue1453 bad_args before={before_bad_args} after={after_bad_args} response={bad_args:?}"
    );
    assert_eq!(before_bad_args, after_bad_args);
    assert_eq!(bad_args.error.unwrap().code, -32000);

    let bad_version = handle_mcp_loopback_json(json!({
        "jsonrpc": "1.0",
        "id": "issue1453-bad-version",
        "method": TOOLS_LIST_METHOD,
        "params": {}
    }))
    .unwrap();
    assert_eq!(bad_version.error.unwrap().code, -32600);
}

fn paint_args(document_path: &Path, ledger_path: &Path) -> Value {
    json!({
        "document_path": document_path,
        "ledger_path": ledger_path,
        "ledger_id": "issue1453-world-ledger",
        "request_id": "issue1453-paint",
        "requested_by": "litd-editor-issue1453-test",
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

fn world_state(document_path: &Path, ledger_path: &Path) -> Value {
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
        "sha256": readback.as_ref().map(|readback| readback.sha256.clone()).unwrap_or_default(),
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}

#[test]
fn issue1490_run_tool_dispatch_isolates_panics() {
    // A panicking tool must become a JSON-RPC internal error, not a process abort. The
    // fact that this test runs to completion after the panic is caught proves the loop
    // (process) stays alive — the whole point of the isolation (#1490).
    let response = run_tool_dispatch(json!("issue1490-panic"), "probe.panic", || {
        panic!("simulated tool panic boom")
    });
    let error = response.error.expect("panic must map to a JSON-RPC error");
    assert_eq!(error.code, -32603);
    assert_eq!(error.message, "Internal error");
    assert_eq!(error.data["tool_name"], "probe.panic");
    assert!(
        error.data["reason"]
            .as_str()
            .unwrap()
            .contains("simulated tool panic boom"),
        "reason must carry the panic payload: {error:?}"
    );
    assert!(response.result.is_none());
    assert_eq!(response.id, json!("issue1490-panic"));

    // String panic payload is also extracted (not just &str literals).
    let string_panic = run_tool_dispatch(json!(1), "probe.panic_string", || {
        panic!("{}", String::from("string-payload-panic"))
    });
    assert!(
        string_panic.error.unwrap().data["reason"]
            .as_str()
            .unwrap()
            .contains("string-payload-panic")
    );
}

#[test]
fn issue1490_run_tool_dispatch_maps_tool_error_and_success() {
    let ok = run_tool_dispatch(json!(1), "probe.ok", || Ok(json!({ "did": "work" })));
    assert!(ok.error.is_none());
    assert_eq!(
        ok.result.as_ref().unwrap()["structuredContent"],
        json!({ "did": "work" })
    );

    let err = run_tool_dispatch(json!(2), "probe.err", || {
        Err(EngineError::UiRenderInvalidConfig {
            field: "probe".into(),
            reason: "bad".into(),
        })
    });
    let error = err.error.unwrap();
    assert_eq!(error.code, -32000);
    assert_eq!(error.message, "Tool execution failed");
}

#[test]
fn issue1490_initialize_handshake_returns_protocol_and_server_info() {
    let response = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1490-init",
        "method": INITIALIZE_METHOD,
        "params": {}
    }))
    .expect("initialize must produce a response");
    assert!(response.error.is_none(), "{response:?}");
    let result = response.result.unwrap();
    assert_eq!(result["protocolVersion"], MCP_PROTOCOL_VERSION);
    assert_eq!(result["serverInfo"]["name"], "litd-editor-mcp");
    assert!(result["capabilities"].get("tools").is_some());
}

#[test]
fn issue1490_notification_without_id_gets_no_response() {
    // A JSON-RPC notification (no id) must not be answered.
    let notification = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    }));
    assert!(
        notification.is_none(),
        "notification must not be answered: {notification:?}"
    );

    // A normal request (with id) still gets a response.
    let answered = handle_mcp_loopback_json(json!({
        "jsonrpc": "2.0",
        "id": "issue1490-tools-list",
        "method": TOOLS_LIST_METHOD,
        "params": {}
    }));
    assert!(answered.is_some());
}
