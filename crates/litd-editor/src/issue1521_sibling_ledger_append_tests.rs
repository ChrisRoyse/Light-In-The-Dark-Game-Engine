// FSV for #1521: the sibling MCP tool ledgers must APPEND, not overwrite — the same
// audit-depth-of-one bug fixed for the project ledger in #1490. Proven here against the
// two distinct Aster store mechanisms these ledgers use:
//   - agent tool ledger  -> calyx_json_store (write/read_calyx_json_bytes), shared by
//     the asset/trigger/world/vfx/harness ledgers too;
//   - object tool ledger -> McpObjectAsterStore (put_ledger_bytes + the new
//     read_latest_ledger_bytes_optional).
// For each: two writes to the same row/ledger_id leave record_count == 2 with both
// records in order and physically present in the durable WAL frame; a ledger_id
// mismatch and an invalid record both fail closed without mutating the row.

use std::fs;

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::fsv_support::artifact_dir;
use crate::mcp_agent_tools::mcp_agent_tools_store::{
    McpAgentToolRecord, read_mcp_agent_tool_ledger, write_mcp_agent_tool_ledger,
};
use crate::mcp_object_tools::mcp_object_tools_store::{
    McpObjectToolRecord, read_mcp_object_tool_ledger, write_mcp_object_tool_ledger,
};

#[test]
fn issue1521_agent_ledger_appends_across_writes_fsv() {
    let (run, root) = artifact_dir("issue1521_agent_ledger_append", "stores").unwrap();
    let ledger_path = root.join("mcp-agent-tool-ledger-aster");
    const LEDGER_ID: &str = "issue1521-agent-ledger";

    let before = file_state(&ledger_path);
    assert!(!ledger_path.exists());

    let record_a = agent_record("issue1521-agent-a", "bind_agent", "agent.alpha");
    let write_a = write_mcp_agent_tool_ledger(&ledger_path, LEDGER_ID, record_a.clone()).unwrap();
    let read_a = read_mcp_agent_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_a.sha256, write_a.sha256);
    assert_eq!(read_a.ledger.record_count, 1);
    assert_eq!(read_a.ledger.records[0], record_a);

    let record_b = agent_record("issue1521-agent-b", "bind_prompt_model", "agent.beta");
    let write_b = write_mcp_agent_tool_ledger(&ledger_path, LEDGER_ID, record_b.clone()).unwrap();
    let read_b = read_mcp_agent_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_b.sha256, write_b.sha256);
    assert_eq!(
        read_b.ledger.record_count, 2,
        "second write must APPEND, not overwrite"
    );
    assert_eq!(
        read_b.ledger.records[0], record_a,
        "first record must survive the second write"
    );
    assert_eq!(read_b.ledger.records[1], record_b);
    assert_ne!(read_a.sha256, read_b.sha256);

    // Edge: ledger_id mismatch is rejected, row unchanged.
    let sha_before_edges = read_b.sha256.clone();
    let mismatch = write_mcp_agent_tool_ledger(
        &ledger_path,
        "issue1521-different-agent-ledger",
        agent_record("issue1521-agent-c", "bind_agent", "agent.gamma"),
    )
    .unwrap_err()
    .report();
    let after_mismatch = read_mcp_agent_tool_ledger(&ledger_path).unwrap();
    assert_eq!(after_mismatch.ledger.record_count, 2);
    assert_eq!(after_mismatch.sha256, sha_before_edges);

    // Edge: invalid record (empty affected_ids) is rejected, row unchanged.
    let mut bad = agent_record("issue1521-agent-d", "bind_agent", "agent.delta");
    bad.affected_ids.clear();
    let invalid = write_mcp_agent_tool_ledger(&ledger_path, LEDGER_ID, bad)
        .unwrap_err()
        .report();
    let after_invalid = read_mcp_agent_tool_ledger(&ledger_path).unwrap();
    assert_eq!(after_invalid.ledger.record_count, 2);
    assert_eq!(after_invalid.sha256, sha_before_edges);

    let wal_ids = wal_request_ids(&ledger_path, &["issue1521-agent-a", "issue1521-agent-b"]);
    assert!(wal_ids.contains_key("issue1521-agent-a"));
    assert!(wal_ids.contains_key("issue1521-agent-b"));

    let report = json!({
        "schema_version": 1, "issue": 1521, "ledger": "agent", "mechanism": "calyx_json_store",
        "source_of_truth": ledger_path.display().to_string(),
        "before_exists": before["exists"],
        "depth1": { "record_count": read_a.ledger.record_count, "sha256": read_a.sha256 },
        "depth2": {
            "record_count": read_b.ledger.record_count, "sha256": read_b.sha256,
            "request_ids": read_b.ledger.records.iter().map(|r| r.request_id.clone()).collect::<Vec<_>>(),
        },
        "wal_request_id_counts": wal_ids,
        "edges": {
            "ledger_id_mismatch": { "record_count_after": after_mismatch.ledger.record_count, "unchanged": after_mismatch.sha256 == sha_before_edges, "error": error_json(mismatch) },
            "invalid_record": { "record_count_after": after_invalid.ledger.record_count, "unchanged": after_invalid.sha256 == sha_before_edges, "error": error_json(invalid) },
        },
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "ISSUE1521_AGENT_APPEND_FSV path={} depth1_sha={} depth2_sha={} count={} wal_a={} wal_b={}",
        readback.path.display(),
        read_a.sha256,
        read_b.sha256,
        read_b.ledger.record_count,
        wal_ids.get("issue1521-agent-a").copied().unwrap_or(0),
        wal_ids.get("issue1521-agent-b").copied().unwrap_or(0),
    );
}

#[test]
fn issue1521_object_ledger_appends_across_writes_fsv() {
    let (run, root) = artifact_dir("issue1521_object_ledger_append", "stores").unwrap();
    let ledger_path = root.join("mcp-object-tool-ledger-aster");
    const LEDGER_ID: &str = "issue1521-object-ledger";

    assert!(!ledger_path.exists());

    let record_a = object_record("issue1521-object-a", "object.alpha");
    let write_a = write_mcp_object_tool_ledger(&ledger_path, LEDGER_ID, record_a.clone()).unwrap();
    let read_a = read_mcp_object_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_a.sha256, write_a.sha256);
    assert_eq!(read_a.ledger.record_count, 1);
    assert_eq!(read_a.ledger.records[0], record_a);

    let record_b = object_record("issue1521-object-b", "object.beta");
    let write_b = write_mcp_object_tool_ledger(&ledger_path, LEDGER_ID, record_b.clone()).unwrap();
    let read_b = read_mcp_object_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_b.sha256, write_b.sha256);
    assert_eq!(
        read_b.ledger.record_count, 2,
        "second write must APPEND via read_latest_ledger_bytes_optional"
    );
    assert_eq!(read_b.ledger.records[0], record_a);
    assert_eq!(read_b.ledger.records[1], record_b);

    // Edge: ledger_id mismatch rejected, row unchanged.
    let sha_before_edges = read_b.sha256.clone();
    let mismatch = write_mcp_object_tool_ledger(
        &ledger_path,
        "issue1521-different-object-ledger",
        object_record("issue1521-object-c", "object.gamma"),
    )
    .unwrap_err()
    .report();
    let after_mismatch = read_mcp_object_tool_ledger(&ledger_path).unwrap();
    assert_eq!(after_mismatch.ledger.record_count, 2);
    assert_eq!(after_mismatch.sha256, sha_before_edges);

    // Edge: invalid record (empty affected_object_ids) rejected, row unchanged.
    let mut bad = object_record("issue1521-object-d", "object.delta");
    bad.affected_object_ids.clear();
    let invalid = write_mcp_object_tool_ledger(&ledger_path, LEDGER_ID, bad)
        .unwrap_err()
        .report();
    let after_invalid = read_mcp_object_tool_ledger(&ledger_path).unwrap();
    assert_eq!(after_invalid.ledger.record_count, 2);
    assert_eq!(after_invalid.sha256, sha_before_edges);

    let wal_ids = wal_request_ids(&ledger_path, &["issue1521-object-a", "issue1521-object-b"]);
    assert!(wal_ids.contains_key("issue1521-object-a"));
    assert!(wal_ids.contains_key("issue1521-object-b"));

    let report = json!({
        "schema_version": 1, "issue": 1521, "ledger": "object", "mechanism": "McpObjectAsterStore",
        "source_of_truth": ledger_path.display().to_string(),
        "depth1": { "record_count": read_a.ledger.record_count, "sha256": read_a.sha256 },
        "depth2": {
            "record_count": read_b.ledger.record_count, "sha256": read_b.sha256,
            "request_ids": read_b.ledger.records.iter().map(|r| r.request_id.clone()).collect::<Vec<_>>(),
        },
        "wal_request_id_counts": wal_ids,
        "edges": {
            "ledger_id_mismatch": { "record_count_after": after_mismatch.ledger.record_count, "unchanged": after_mismatch.sha256 == sha_before_edges, "error": error_json(mismatch) },
            "invalid_record": { "record_count_after": after_invalid.ledger.record_count, "unchanged": after_invalid.sha256 == sha_before_edges, "error": error_json(invalid) },
        },
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "ISSUE1521_OBJECT_APPEND_FSV path={} depth1_sha={} depth2_sha={} count={} wal_a={} wal_b={}",
        readback.path.display(),
        read_a.sha256,
        read_b.sha256,
        read_b.ledger.record_count,
        wal_ids.get("issue1521-object-a").copied().unwrap_or(0),
        wal_ids.get("issue1521-object-b").copied().unwrap_or(0),
    );
}

fn agent_record(request_id: &str, op: &str, affected_id: &str) -> McpAgentToolRecord {
    McpAgentToolRecord {
        request_id: request_id.to_string(),
        requested_by: "issue1521-fsv".to_string(),
        tool_name: format!("litd.editor.agents.{op}"),
        operation_id: format!("editor.agents.{op}"),
        state_path: "Calyx Aster row issue1521-agent-state".to_string(),
        state_sha256: sha256_hex(request_id.as_bytes()),
        state_bytes: 2048,
        action_summary: format!("verified append via {op}"),
        affected_ids: vec![affected_id.to_string()],
        binding_count: 1,
        prompt_model_count: 1,
        ward_envelope_count: 1,
    }
}

fn object_record(request_id: &str, affected_id: &str) -> McpObjectToolRecord {
    McpObjectToolRecord {
        request_id: request_id.to_string(),
        requested_by: "issue1521-fsv".to_string(),
        tool_name: "litd.editor.objects.apply_object_delta".to_string(),
        operation_id: "editor.objects.apply_object_delta".to_string(),
        document_path: "Calyx Aster row issue1521-object-doc".to_string(),
        document_sha256: sha256_hex(request_id.as_bytes()),
        document_bytes: 4096,
        document_fingerprint: format!("fingerprint-{request_id}"),
        action_summary: "verified append".to_string(),
        affected_object_ids: vec![affected_id.to_string()],
        object_count: 1,
        unit_count: 1,
        hero_count: 0,
        ability_count: 0,
        item_count: 0,
        buff_count: 0,
        upgrade_count: 0,
        tech_count: 0,
        gameplay_table_count: 0,
    }
}

/// Count how many times each expected request id physically appears in the durable WAL
/// files under the Aster vault directory (source-of-truth check, not just the readback).
fn wal_request_ids(
    vault_dir: &std::path::Path,
    ids: &[&str],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    let mut stack = vec![vault_dir.to_path_buf()];
    let mut blob = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("wal") {
                blob.extend(fs::read(&path).unwrap());
            }
        }
    }
    let text = String::from_utf8_lossy(&blob);
    for id in ids {
        let n = text.matches(id).count();
        if n > 0 {
            counts.insert((*id).to_string(), n);
        }
    }
    counts
}

fn file_state(path: &std::path::Path) -> Value {
    json!({ "path": path.display().to_string(), "exists": path.exists() })
}

fn error_json(report: litd_engine::diagnostics::ErrorReport) -> Value {
    json!({ "code": report.code, "message": report.message })
}
