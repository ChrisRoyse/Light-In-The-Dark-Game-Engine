//! FSV harness for #1777: prove the procgen MCP `generate_procedural_mesh`
//! ledger's single source of truth is the Calyx Aster vault (doctrine #1348 /
//! #1363), NOT a JSON side-store.
//!
//! It drives the REAL tool (`call_mcp_procgen_tool`), then reads the ledger back
//! three INDEPENDENT ways and requires all three to agree:
//!   1. the tool's own returned `ledger_readback.sha256`;
//!   2. a separate Aster row-read (`read_procgen_ledger`); and
//!   3. the physical on-disk WAL segment bytes replayed from the vault's `wal/`
//!      directory (`read_procgen_ledger_wal`) -- the deepest proof the vault is
//!      the store.
//!
//! Three edge cases each capture the source-of-truth state read independently of
//! the tool return value, before AND after the action, and emit an on-disk
//! artifact that is read straight back and compared byte-for-byte.
//!
//! Run: `cargo run --features fsv --example mcp_procgen_aster_fsv`

use std::{fs, path::Path};

use litd_editor::mcp_procgen_tools::{
    GENERATE_MESH_TOOL, call_mcp_procgen_tool, read_procgen_ledger, read_procgen_ledger_wal,
};
use litd_engine::calyx_json_store::CalyxJsonWalReadback;
use litd_engine::test_temp::TestTempDir;
use serde_json::{Value, json};

const SOURCE_OF_TRUTH: &str = "Calyx Aster procgen MCP ledger row (Aster vault + on-disk WAL segment); content-addressed mesh blobs are permitted opaque assets";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (run, artifact_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_PROCGEN_ASTER_FSV",
        "issue1777_procgen_aster",
        "procgen-aster-fsv.json",
    )?;

    let happy = happy_path()?;
    let edge_cases = vec![
        edge_missing_row()?,
        edge_append_advances_wal()?,
        edge_rejected_call_leaves_source_untouched()?,
    ];

    let fsv = json!({
        "issue": 1777,
        "source_of_truth": SOURCE_OF_TRUTH,
        "happy_path": happy,
        "edge_cases": edge_cases,
    });

    write_json(&artifact_path, &fsv)?;
    let readback = read_json(&artifact_path)?;
    if readback != fsv {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let recorded = run.record_json_file(&artifact_path)?;

    println!(
        "{}",
        json!({
            "issue": 1777,
            "artifact_path": artifact_path.display().to_string(),
            "artifact_sha256": recorded.sha256,
            "tool_reported_ledger_sha256": fsv["happy_path"]["tool_reported_ledger_sha256"],
            "aster_row_sha256": fsv["happy_path"]["aster_row_readback"]["sha256"],
            "physical_wal_sha256": fsv["happy_path"]["physical_wal_readback"]["sha256"],
            "all_three_readbacks_agree": true,
            "edge_case_count": fsv["edge_cases"].as_array().map(Vec::len).unwrap_or(0),
            "artifact_readback_matches": true,
        })
    );
    Ok(())
}

/// Drive the tool once, then read the ledger back three independent ways.
fn happy_path() -> Result<Value, Box<dyn std::error::Error>> {
    let dir = TestTempDir::new("procgen_aster_happy");
    fs::create_dir_all(dir.path())?;
    let vault = dir.path().join("procgen-ledger.json");

    // Before: the source of truth must be empty AND fail loud (no silent empty).
    let before_state = capture_ledger_state(&vault);
    if before_state["row_present"] != json!(false) {
        return Err("ledger row unexpectedly present before any tool call".into());
    }

    let output = call_mcp_procgen_tool(
        GENERATE_MESH_TOOL,
        box_args(dir.path(), "mcp.box.2x2x2", "req-happy", [2.0, 2.0, 2.0]),
    )?;
    let tool_content_hash = output.result["content_hash"]
        .as_str()
        .ok_or("tool result missing content_hash")?
        .to_string();
    let tool_ledger_sha = output.ledger_readback.sha256.clone();

    // Independent readback #1: a SEPARATE Aster row-read from the vault.
    let row = read_procgen_ledger(&vault)?;
    if row.sha256 != tool_ledger_sha {
        return Err(format!(
            "aster row sha256 {} != tool sha256 {}",
            row.sha256, tool_ledger_sha
        )
        .into());
    }
    if row.last_content_hash.as_deref() != Some(tool_content_hash.as_str()) {
        return Err("aster row last_content_hash did not match the produced mesh hash".into());
    }

    // Independent readback #2: physical on-disk WAL segment bytes.
    let wal = read_procgen_ledger_wal(&vault)?;
    if wal.sha256 != tool_ledger_sha {
        return Err(format!(
            "physical WAL sha256 {} != tool sha256 {}",
            wal.sha256, tool_ledger_sha
        )
        .into());
    }
    // The ledger path is a real Aster vault DIRECTORY (has wal/), not a JSON file.
    if !vault.join("wal").is_dir() {
        return Err("ledger path is not a Calyx Aster vault (no wal/ dir)".into());
    }

    let after_state = capture_ledger_state(&vault);
    Ok(json!({
        "tool_name": output.tool_name,
        "tool_source_of_truth": output.source_of_truth,
        "mesh_content_hash": tool_content_hash,
        "before_state": before_state,
        "tool_reported_ledger_sha256": tool_ledger_sha,
        "aster_row_readback": {
            "path": row.path,
            "record_count": row.record_count,
            "sha256": row.sha256,
            "last_content_hash": row.last_content_hash,
        },
        "physical_wal_readback": wal_summary(&wal),
        "after_state": after_state,
    }))
}

/// Edge 1: no tool call ran, so the vault holds no row. Both the readback and
/// its state stay empty and the readback fails loud -- never a silent empty.
fn edge_missing_row() -> Result<Value, Box<dyn std::error::Error>> {
    let dir = TestTempDir::new("procgen_aster_missing");
    fs::create_dir_all(dir.path())?;
    let vault = dir.path().join("procgen-ledger.json");

    let before_state = capture_ledger_state(&vault);
    let error = read_procgen_ledger(&vault)
        .err()
        .ok_or("missing-row readback unexpectedly succeeded")?;
    let message = error.to_string();
    if !message.contains("missing Calyx Aster procgen ledger row") {
        return Err(format!("missing-row readback did not fail loud: {message}").into());
    }
    let after_state = capture_ledger_state(&vault);
    Ok(json!({
        "name": "missing_ledger_row_fails_loud",
        "before_state": before_state,
        "observed_error": message,
        "after_state": after_state,
    }))
}

/// Edge 2: a second call appends a record; the source of truth moves from 1 to 2
/// records, the physical WAL seq advances, and the new physical WAL bytes hash to
/// the freshly tool-reported digest.
fn edge_append_advances_wal() -> Result<Value, Box<dyn std::error::Error>> {
    let dir = TestTempDir::new("procgen_aster_append");
    fs::create_dir_all(dir.path())?;
    let vault = dir.path().join("procgen-ledger.json");

    call_mcp_procgen_tool(
        GENERATE_MESH_TOOL,
        box_args(dir.path(), "mcp.box.2x2x2", "req-a", [2.0, 2.0, 2.0]),
    )?;
    let before_state = capture_ledger_state(&vault);
    let before_wal = read_procgen_ledger_wal(&vault)?;

    let second = call_mcp_procgen_tool(
        GENERATE_MESH_TOOL,
        box_args(dir.path(), "mcp.box.1x1x1", "req-b", [1.0, 1.0, 1.0]),
    )?;
    let after_state = capture_ledger_state(&vault);
    let after_wal = read_procgen_ledger_wal(&vault)?;

    if before_state["record_count"] != json!(1) || after_state["record_count"] != json!(2) {
        return Err("append did not move the ledger from 1 to 2 records".into());
    }
    if after_wal.seq <= before_wal.seq {
        return Err(format!(
            "physical WAL seq did not advance: before={} after={}",
            before_wal.seq, after_wal.seq
        )
        .into());
    }
    if after_wal.sha256 != second.ledger_readback.sha256 {
        return Err("post-append physical WAL sha256 != tool-reported sha256".into());
    }
    Ok(json!({
        "name": "append_advances_wal_and_source_of_truth",
        "before_state": before_state,
        "before_wal_seq": before_wal.seq,
        "after_state": after_state,
        "after_wal_seq": after_wal.seq,
        "after_wal_sha256": after_wal.sha256,
        "tool_reported_sha256": second.ledger_readback.sha256,
    }))
}

/// Edge 3: an invalid (zero-size) request is rejected loudly and must NOT touch
/// the ledger. The independently-read source-of-truth state is byte-identical
/// before and after the rejected call.
fn edge_rejected_call_leaves_source_untouched() -> Result<Value, Box<dyn std::error::Error>> {
    let dir = TestTempDir::new("procgen_aster_rejected");
    fs::create_dir_all(dir.path())?;
    let vault = dir.path().join("procgen-ledger.json");

    call_mcp_procgen_tool(
        GENERATE_MESH_TOOL,
        box_args(dir.path(), "mcp.box.2x2x2", "req-ok", [2.0, 2.0, 2.0]),
    )?;
    let before_state = capture_ledger_state(&vault);

    let error = call_mcp_procgen_tool(
        GENERATE_MESH_TOOL,
        box_args(dir.path(), "mcp.box.bad", "req-bad", [0.0, 2.0, 2.0]),
    )
    .err()
    .ok_or("zero-size request unexpectedly succeeded")?;
    let message = error.to_string();
    if !message.contains("zero-size") {
        return Err(format!("zero-size request not rejected as expected: {message}").into());
    }

    let after_state = capture_ledger_state(&vault);
    if before_state != after_state {
        return Err("rejected call mutated the ledger source of truth".into());
    }
    Ok(json!({
        "name": "rejected_call_leaves_source_of_truth_byte_identical",
        "before_state": before_state,
        "observed_error": message,
        "after_state": after_state,
    }))
}

/// Read the ledger source of truth INDEPENDENTLY of any tool return value.
fn capture_ledger_state(vault: &Path) -> Value {
    match read_procgen_ledger(vault) {
        Ok(row) => json!({
            "row_present": true,
            "record_count": row.record_count,
            "sha256": row.sha256,
            "last_content_hash": row.last_content_hash,
        }),
        Err(error) => json!({
            "row_present": false,
            "error": error.to_string(),
        }),
    }
}

fn wal_summary(wal: &CalyxJsonWalReadback) -> Value {
    json!({
        "wal_dir": wal.wal_dir.display().to_string(),
        "seq": wal.seq,
        "byte_len": wal.byte_len,
        "sha256": wal.sha256,
        "wal_row_count": wal.wal_row_count,
        "graph_row_count": wal.graph_row_count,
        "time_index_row_count": wal.time_index_row_count,
        "segment_path": wal.segment_path.display().to_string(),
        "segment_sha256": wal.segment_sha256,
        "frame_sha256": wal.frame_sha256,
    })
}

fn box_args(dir: &Path, mesh_id: &str, request_id: &str, size: [f64; 3]) -> Value {
    json!({
        "store_root": dir.join("store"),
        "ledger_path": dir.join("procgen-ledger.json"),
        "request_id": request_id,
        "requested_by": "issue1777-fsv",
        "mesh_id": mesh_id,
        "request": {
            "target": "primitive",
            "spec": { "kind": "box", "size": size }
        }
    })
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
