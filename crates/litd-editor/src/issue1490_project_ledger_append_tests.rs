// FSV for #1490: the project tool ledger must APPEND, not overwrite.
//
// Before the fix, `write_mcp_project_tool_ledger` always wrote `record_count: 1,
// records: vec![record]`, so every project mutation destroyed the prior audit
// record — the `record_count`/`records: Vec` schema implied an accumulating audit
// trail that never accumulated. This test proves, against the real Calyx Aster
// store, that two sequential writes to the same ledger row leave BOTH records in
// order, and that the fail-closed guards (ledger_id mismatch, invalid record,
// invalid ledger_id) never mutate the source of truth.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::fsv_support::artifact_dir;
use crate::mcp_project_tools::mcp_project_tools_store::{
    McpProjectToolRecord, read_mcp_project_tool_ledger, write_mcp_project_tool_ledger,
};

const LEDGER_ID: &str = "issue1490-project-tool-ledger";

#[test]
fn issue1490_project_tool_ledger_appends_across_writes_fsv() {
    let (run, root) = artifact_dir("issue1490_project_ledger_append", "stores").unwrap();
    let ledger_path = root.join("mcp-project-tool-ledger-aster");

    // Source of truth is the Aster row; empty before the first write.
    let before = file_state(&ledger_path);
    assert!(!ledger_path.exists());

    // --- Happy path: first write -> depth 1 ---
    let record_a = project_record(
        "issue1490-request-a",
        "new_open_save",
        "project.ashen_reach",
    );
    let write_a = write_mcp_project_tool_ledger(&ledger_path, LEDGER_ID, record_a.clone()).unwrap();
    let read_a = read_mcp_project_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_a.sha256, write_a.sha256);
    assert_eq!(read_a.ledger.record_count, 1);
    assert_eq!(read_a.ledger.records.len(), 1);
    assert_eq!(read_a.ledger.records[0], record_a);
    assert_eq!(read_a.ledger.ledger_id, LEDGER_ID);

    // --- Happy path: second write to the SAME row -> depth 2, appended in order ---
    let record_b = project_record(
        "issue1490-request-b",
        "package_litdgame",
        "package.ashen_reach",
    );
    let write_b = write_mcp_project_tool_ledger(&ledger_path, LEDGER_ID, record_b.clone()).unwrap();
    let read_b = read_mcp_project_tool_ledger(&ledger_path).unwrap();
    assert_eq!(read_b.sha256, write_b.sha256);
    // The append is the whole point: overwrite would leave this at 1.
    assert_eq!(read_b.ledger.record_count, 2);
    assert_eq!(read_b.ledger.records.len(), 2);
    assert_eq!(
        read_b.ledger.records[0], record_a,
        "first record must survive the second write"
    );
    assert_eq!(
        read_b.ledger.records[1], record_b,
        "second record must be appended after the first"
    );
    // Content actually changed on disk between the two writes.
    assert_ne!(read_a.sha256, read_b.sha256);

    // --- Edge 1: ledger_id mismatch is rejected, store unchanged ---
    let sha_before_edges = read_b.sha256.clone();
    let mismatch = write_mcp_project_tool_ledger(
        &ledger_path,
        "issue1490-different-ledger",
        project_record(
            "issue1490-request-c",
            "publish_litdgame",
            "publish.ashen_reach",
        ),
    );
    let mismatch_err = mismatch.unwrap_err().report();
    let after_mismatch = read_mcp_project_tool_ledger(&ledger_path).unwrap();
    assert_eq!(
        after_mismatch.ledger.record_count, 2,
        "mismatch must not append"
    );
    assert_eq!(
        after_mismatch.sha256, sha_before_edges,
        "mismatch must not mutate the row"
    );

    // --- Edge 2: invalid record (empty affected_ids) is rejected, store unchanged ---
    let mut bad_record = project_record(
        "issue1490-request-d",
        "new_open_save",
        "project.ashen_reach",
    );
    bad_record.affected_ids.clear();
    let invalid_record = write_mcp_project_tool_ledger(&ledger_path, LEDGER_ID, bad_record);
    let invalid_record_err = invalid_record.unwrap_err().report();
    let after_invalid_record = read_mcp_project_tool_ledger(&ledger_path).unwrap();
    assert_eq!(
        after_invalid_record.ledger.record_count, 2,
        "invalid record must not append"
    );
    assert_eq!(after_invalid_record.sha256, sha_before_edges);

    // --- Edge 3: invalid ledger_id (newline) is rejected on a fresh row ---
    let fresh_path = root.join("edge-invalid-ledger-id-aster");
    let invalid_id = write_mcp_project_tool_ledger(
        &fresh_path,
        "bad\nledger",
        project_record(
            "issue1490-request-e",
            "new_open_save",
            "project.ashen_reach",
        ),
    );
    let invalid_id_err = invalid_id.unwrap_err().report();
    assert!(
        !fresh_path.exists(),
        "invalid ledger_id must not create a row"
    );

    let after = file_state(&ledger_path);
    let report = json!({
        "schema_version": 1,
        "issue": 1490,
        "finding": "project tool ledger must append, not overwrite (audit depth of one)",
        "source_of_truth": ledger_path.display().to_string(),
        "before": before,
        "after": after,
        "happy_path": {
            "first_write": { "record_count": read_a.ledger.record_count, "sha256": read_a.sha256 },
            "second_write": {
                "record_count": read_b.ledger.record_count,
                "sha256": read_b.sha256,
                "record_ids": read_b.ledger.records.iter()
                    .map(|record| record.request_id.clone()).collect::<Vec<_>>(),
            },
        },
        "edges": {
            "ledger_id_mismatch": {
                "record_count_after": after_mismatch.ledger.record_count,
                "sha256_unchanged": after_mismatch.sha256 == sha_before_edges,
                "error": error_json(mismatch_err),
            },
            "invalid_record": {
                "record_count_after": after_invalid_record.ledger.record_count,
                "sha256_unchanged": after_invalid_record.sha256 == sha_before_edges,
                "error": error_json(invalid_record_err),
            },
            "invalid_ledger_id": {
                "row_created": fresh_path.exists(),
                "error": error_json(invalid_id_err),
            },
        },
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "ISSUE1490_APPEND_FSV_REPORT path={} bytes={} sha256={} depth1_sha={} depth2_sha={} appended_ids={:?}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        read_a.sha256,
        read_b.sha256,
        read_b
            .ledger
            .records
            .iter()
            .map(|r| r.request_id.clone())
            .collect::<Vec<_>>(),
    );
}

fn project_record(request_id: &str, tool: &str, affected_id: &str) -> McpProjectToolRecord {
    // A 64-hex state_sha256 and non-zero state_bytes are required by the validator;
    // deriving the hash from the request_id keeps each record distinct.
    let state_sha256 = sha256_hex(request_id.as_bytes());
    McpProjectToolRecord {
        request_id: request_id.to_string(),
        requested_by: "issue1490-fsv".to_string(),
        tool_name: format!("litd.editor.project.{tool}"),
        operation_id: format!("editor.project.{tool}"),
        state_path: "Calyx Aster row issue1490-project-state".to_string(),
        state_sha256,
        state_bytes: 4096,
        action_summary: format!("verified append via {tool}"),
        affected_ids: vec![affected_id.to_string()],
        project_count: 1,
        playtest_count: 0,
        package_count: 0,
        published_count: 0,
    }
}

fn file_state(path: &Path) -> Value {
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": path.is_dir(),
        "is_file": path.is_file(),
        "sha256": if path.is_file() {
            Some(sha256_hex(&fs::read(path).unwrap()))
        } else {
            None
        },
    })
}

fn error_json(report: litd_engine::diagnostics::ErrorReport) -> Value {
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
