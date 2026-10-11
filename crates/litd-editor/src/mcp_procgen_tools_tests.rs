use litd_engine::test_temp::TestTempDir;
use serde_json::json;

use super::*;

/// A fresh, auto-cleaning scratch dir (#1535: no raw `env::temp_dir()`). The
/// returned guard removes the directory on `Drop`, so callers keep it in scope.
fn temp_dir(tag: &str) -> TestTempDir {
    let guard = TestTempDir::new(&format!("mcp_procgen_{tag}"));
    std::fs::create_dir_all(guard.path()).unwrap();
    guard
}

fn box_args(dir: &std::path::Path) -> serde_json::Value {
    json!({
        "store_root": dir.join("store"),
        "ledger_path": dir.join("procgen-ledger.json"),
        "request_id": "req-1",
        "requested_by": "fsv-test",
        "mesh_id": "mcp.box.2x2x2",
        "request": {
            "target": "primitive",
            "spec": { "kind": "box", "size": [2.0, 2.0, 2.0] }
        }
    })
}

#[test]
fn generate_mesh_tool_stores_and_reads_back_a_box() {
    let dir = temp_dir("box");
    let output = call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir)).unwrap();
    assert_eq!(output.tool_name, GENERATE_MESH_TOOL);
    let result = &output.result;
    assert_eq!(result["vertex_count"], 24);
    assert_eq!(result["index_count"], 36);
    assert_eq!(result["triangle_count"], 12);
    assert_eq!(result["loaded_vertex_count"], 24);
    // Known first vertex position of a 2x2x2 box (+X face corner 0).
    assert_eq!(result["sample_positions"][0], json!([1.0, -1.0, -1.0]));
    // The manifest really exists on disk at the returned path.
    let manifest = result["store_manifest_path"].as_str().unwrap();
    assert!(std::path::Path::new(manifest).exists());
    // Ledger recorded exactly one call and reads back.
    assert_eq!(output.ledger_readback.record_count, 1);
    assert_eq!(
        output.ledger_readback.last_content_hash.as_deref(),
        result["content_hash"].as_str()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn generate_mesh_tool_is_deterministic() {
    let dir_a = temp_dir("det_a");
    let dir_b = temp_dir("det_b");
    let a = call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir_a)).unwrap();
    let b = call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir_b)).unwrap();
    assert_eq!(a.result["content_hash"], b.result["content_hash"]);
    assert_eq!(a.result["vertex_sha256"], b.result["vertex_sha256"]);
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn generate_mesh_tool_appends_ledger_across_calls() {
    let dir = temp_dir("ledger");
    let mut heightfield = box_args(&dir);
    heightfield["mesh_id"] = json!("mcp.tile");
    heightfield["request"] = json!({
        "target": "heightfield",
        "spec": {
            "tile_id": "mcp.tile",
            "width": 3, "depth": 3, "cell_size": 1.0,
            "heights": [0.0, 0.5, 1.0, 0.5, 1.0, 1.5, 1.0, 1.5, 2.0],
            "lod": 0, "skirt": 0.0, "splat_bands": []
        }
    });
    call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir)).unwrap();
    let second = call_mcp_procgen_tool(GENERATE_MESH_TOOL, heightfield).unwrap();
    assert_eq!(second.ledger_readback.record_count, 2);
    let readback = read_procgen_ledger(&dir.join("procgen-ledger.json")).unwrap();
    assert_eq!(readback.record_count, 2);
    assert_eq!(readback.sha256, second.ledger_readback.sha256);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn generate_mesh_tool_rejects_zero_size_loudly() {
    let dir = temp_dir("bad");
    let mut args = box_args(&dir);
    args["request"]["spec"]["size"] = json!([0.0, 2.0, 2.0]);
    let error = call_mcp_procgen_tool(GENERATE_MESH_TOOL, args).unwrap_err();
    assert!(error.to_string().contains("zero-size"));
    // No ledger should have been written for a rejected request.
    assert!(!dir.join("procgen-ledger.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_tool_is_rejected() {
    let error = call_mcp_procgen_tool("litd.engine.render.bogus", json!({})).unwrap_err();
    assert!(error.to_string().contains("not a registered"));
}

#[test]
fn ledger_row_is_a_calyx_aster_vault_not_a_json_side_store() {
    // Independent readback: after the tool runs, the ledger digest it reported
    // must match a SEPARATE read straight out of the Calyx Aster vault, and the
    // ledger path must be an Aster vault DIRECTORY (not a bespoke JSON file).
    let dir = temp_dir("aster");
    let output = call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir)).unwrap();
    let vault = dir.join("procgen-ledger.json");
    assert!(
        vault.is_dir(),
        "ledger path must be an Aster vault directory"
    );
    // The Aster vault physically has a WAL directory (real Calyx substrate).
    assert!(
        vault.join("wal").is_dir(),
        "Aster vault must have a WAL dir"
    );
    let independent = read_procgen_ledger(&vault).unwrap();
    assert_eq!(independent.sha256, output.ledger_readback.sha256);
    assert_eq!(
        independent.last_content_hash.as_deref(),
        output.result["content_hash"].as_str()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ledger_physical_wal_bytes_match_the_tool_reported_digest() {
    // Deepest source-of-truth proof: replay the ledger straight out of the
    // Aster vault's on-disk WAL segment (independent of both the tool return
    // value AND the row-read API) and confirm those physical bytes hash to the
    // same sha256 the tool reported, and that the write is a well-formed
    // plain-column write (two Graph rows + one TimeIndex row).
    let dir = temp_dir("wal");
    let output = call_mcp_procgen_tool(GENERATE_MESH_TOOL, box_args(&dir)).unwrap();
    let vault = dir.join("procgen-ledger.json");
    let wal = read_procgen_ledger_wal(&vault).unwrap();
    assert_eq!(wal.sha256, output.ledger_readback.sha256);
    assert_eq!(wal.wal_row_count, 3);
    assert_eq!(wal.graph_row_count, 2);
    assert_eq!(wal.time_index_row_count, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reading_a_missing_ledger_wal_fails_loud() {
    // Edge: no tool call has run, so the vault holds no procgen ledger row.
    // The physical-WAL readback must fail loud, never invent an empty frame.
    let dir = temp_dir("wal_missing");
    let error = read_procgen_ledger_wal(&dir.join("procgen-ledger.json")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing Calyx Aster procgen ledger row"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reading_a_missing_ledger_row_fails_loud() {
    // Edge: no tool call has run, so the vault holds no procgen ledger row.
    // The readback must fail loud, never silently invent an empty ledger.
    let dir = temp_dir("missing");
    let error = read_procgen_ledger(&dir.join("procgen-ledger.json")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing Calyx Aster procgen ledger row"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
