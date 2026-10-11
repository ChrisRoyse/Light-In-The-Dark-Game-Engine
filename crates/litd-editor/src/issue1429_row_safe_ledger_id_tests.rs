use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    agent_skills::{
        AgentSkillSearchQuery, build_agent_skill_discovery_ledger, default_agent_skill_registry,
        read_agent_skill_discovery_ledger, search_agent_skills, write_agent_skill_discovery_ledger,
    },
    mcp_agent_tools::mcp_agent_tools_store::{
        McpAgentToolRecord, read_mcp_agent_tool_ledger, write_mcp_agent_tool_ledger,
    },
};

#[test]
fn issue1429_row_safe_ledger_ids_fail_closed_with_real_aster_readback() {
    let root = unique_root();
    fs::create_dir_all(&root).unwrap();
    let agent_aster = root.join("agent-skill-ledger-aster");
    let agent_legacy = root.join("agent-skill-legacy.json");
    let mcp_agent_aster = root.join("mcp-agent-ledger-aster");
    let stale_path = root.join("stale-file-not-vault");

    let before = json!({
        "agent_aster": file_state(&agent_aster),
        "mcp_agent_aster": file_state(&mcp_agent_aster),
        "stale_path": file_state(&stale_path),
    });

    let registry = default_agent_skill_registry().unwrap();
    let matched = search_agent_skills(
        &registry,
        &AgentSkillSearchQuery {
            query_id: "issue1429-agent-query".to_string(),
            intent: "bind an in-game agent to a prompt model".to_string(),
            limit: 3,
            requested_by: "issue1429-fsv".to_string(),
        },
    )
    .unwrap();
    let agent_ledger = build_agent_skill_discovery_ledger(
        "issue1429-agent-skills-ledger",
        registry,
        vec![matched],
    )
    .unwrap();
    let agent_write =
        write_agent_skill_discovery_ledger(&agent_legacy, &agent_aster, &agent_ledger).unwrap();
    let agent_read = read_agent_skill_discovery_ledger(&agent_aster, &agent_ledger.ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(agent_read.sha256, agent_write.sha256);
    assert_eq!(agent_read.search_count, 1);
    assert!(!agent_legacy.exists());

    let mcp_record = mcp_agent_record(&agent_read.sha256, agent_read.bytes.len() as u64);
    let mcp_write = write_mcp_agent_tool_ledger(
        &mcp_agent_aster,
        "issue1429-mcp-agent-ledger",
        mcp_record.clone(),
    )
    .unwrap();
    let mcp_read = read_mcp_agent_tool_ledger(&mcp_agent_aster).unwrap();
    assert_eq!(mcp_read.sha256, mcp_write.sha256);
    assert_eq!(mcp_read.ledger.ledger_id, "issue1429-mcp-agent-ledger");
    assert_eq!(mcp_read.ledger.record_count, 1);

    let edges = json!({
        "empty": edge_invalid_ledger_ids(&root, "empty", "", &agent_ledger, &mcp_record),
        "newline": edge_invalid_ledger_ids(&root, "newline", "bad\nledger", &agent_ledger, &mcp_record),
        "tab": edge_invalid_ledger_ids(&root, "tab", "bad\tledger", &agent_ledger, &mcp_record),
        "nul": edge_invalid_ledger_ids(&root, "nul", "bad\u{0000}ledger", &agent_ledger, &mcp_record),
        "stale_file_path": stale_path_edge(&stale_path, &agent_aster, &agent_ledger.ledger_id),
    });

    let after = json!({
        "agent_aster": file_state(&agent_aster),
        "mcp_agent_aster": file_state(&mcp_agent_aster),
        "stale_path": file_state(&stale_path),
    });
    let source_audit = source_audit();
    assert!(source_audit["matches"].as_array().unwrap().is_empty());

    let report = json!({
        "schema_version": 1,
        "issue": 1429,
        "source_of_truth": {
            "agent_skill_aster": agent_aster,
            "mcp_agent_aster": mcp_agent_aster,
            "report_path": root.join("issue1429-row-safe-ledger-id-report.json"),
        },
        "before": before,
        "happy_path": {
            "agent_skill": {
                "ledger_id": agent_ledger.ledger_id,
                "sha256": agent_read.sha256,
                "bytes": agent_read.bytes.len(),
                "graph_rows": agent_read.graph_rows,
                "search_count": agent_read.search_count,
            },
            "mcp_agent": {
                "ledger_id": mcp_read.ledger.ledger_id,
                "sha256": mcp_read.sha256,
                "bytes": mcp_read.bytes,
                "record_count": mcp_read.ledger.record_count,
            },
        },
        "edges": edges,
        "after": after,
        "source_audit": source_audit,
        "physical_files": physical_summary(&[root.join("agent-skill-ledger-aster"), root.join("mcp-agent-ledger-aster")]),
    });
    let report_path = root.join("issue1429-row-safe-ledger-id-report.json");
    let bytes = serde_json::to_vec_pretty(&report).unwrap();
    fs::write(&report_path, &bytes).unwrap();
    let readback = fs::read(&report_path).unwrap();
    assert_eq!(readback, bytes);
    println!(
        "ISSUE1429_FSV_REPORT path={} bytes={} sha256={} agent_sha={} mcp_sha={} edge_count={} source_forbidden_matches={}",
        report_path.display(),
        readback.len(),
        sha256_hex(&readback),
        agent_read.sha256,
        mcp_read.sha256,
        report["edges"].as_object().unwrap().len(),
        report["source_audit"]["matches"].as_array().unwrap().len(),
    );
}

fn edge_invalid_ledger_ids(
    root: &Path,
    label: &str,
    invalid_id: &str,
    agent_ledger: &crate::agent_skills::AgentSkillDiscoveryLedger,
    mcp_record: &McpAgentToolRecord,
) -> Value {
    let agent_aster = root.join(format!("edge-{label}-agent-aster"));
    let mcp_aster = root.join(format!("edge-{label}-mcp-agent-aster"));
    let before = json!({
        "agent_aster": file_state(&agent_aster),
        "mcp_aster": file_state(&mcp_aster),
    });

    let mut bad_agent = agent_ledger.clone();
    bad_agent.ledger_id = invalid_id.to_string();
    let agent_error = write_agent_skill_discovery_ledger(
        &root.join(format!("edge-{label}-legacy.json")),
        &agent_aster,
        &bad_agent,
    )
    .unwrap_err()
    .report();
    let mcp_error = write_mcp_agent_tool_ledger(&mcp_aster, invalid_id, mcp_record.clone())
        .unwrap_err()
        .report();

    let after = json!({
        "agent_aster": file_state(&agent_aster),
        "mcp_aster": file_state(&mcp_aster),
    });
    assert_eq!(before, after);
    let value = json!({
        "input_debug": invalid_id.escape_debug().to_string(),
        "before": before,
        "after": after,
        "agent_error": error_json(agent_error),
        "mcp_agent_error": error_json(mcp_error),
    });
    println!("ISSUE1429_EDGE {label} {value}");
    value
}

fn stale_path_edge(stale_path: &Path, agent_aster: &Path, valid_ledger_id: &str) -> Value {
    fs::write(stale_path, b"{\"schema\":\"old-side-store\"}").unwrap();
    let before = file_state(stale_path);
    let before_hash = sha256_hex(&fs::read(stale_path).unwrap());
    let error = read_agent_skill_discovery_ledger(stale_path, valid_ledger_id)
        .unwrap_err()
        .report();
    let after_hash = sha256_hex(&fs::read(stale_path).unwrap());
    let after = file_state(stale_path);
    let valid_read_after = read_agent_skill_discovery_ledger(agent_aster, valid_ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(before_hash, after_hash);
    assert_eq!(valid_read_after.search_count, 1);
    let value = json!({
        "before": before,
        "after": after,
        "hash_unchanged": before_hash == after_hash,
        "valid_ledger_still_readable_sha256": valid_read_after.sha256,
        "error": error_json(error),
    });
    println!("ISSUE1429_EDGE stale_file_path {value}");
    value
}

fn mcp_agent_record(state_sha256: &str, state_bytes: u64) -> McpAgentToolRecord {
    McpAgentToolRecord {
        request_id: "issue1429-mcp-agent-request".to_string(),
        requested_by: "issue1429-fsv".to_string(),
        tool_name: "litd.editor.agents.bind_agent".to_string(),
        operation_id: "editor.agents.bind_agent".to_string(),
        state_path: "Calyx Aster row issue1429-agent-skills-ledger".to_string(),
        state_sha256: state_sha256.to_string(),
        state_bytes,
        action_summary: "verified row-safe ledger id persistence".to_string(),
        affected_ids: vec!["agent.issue1429".to_string()],
        binding_count: 1,
        prompt_model_count: 1,
        ward_envelope_count: 1,
    }
}

fn source_audit() -> Value {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        concat!("ledger.ledger_id", ".trim().is_empty()"),
        concat!(
            "ledger_id",
            ".trim().is_empty()",
            " || ledger_id",
            ".contains",
            "(['\\n', '\\r', '\\t'])"
        ),
        concat!("ledger.ledger_id.contains", "(['\\n', '\\r', '\\t'])"),
    ];
    let mut matches = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let text = fs::read_to_string(&path).unwrap();
                for pattern in forbidden {
                    if text.contains(pattern) {
                        matches.push(json!({
                            "path": path.strip_prefix(&src).unwrap().display().to_string(),
                            "pattern": pattern,
                        }));
                    }
                }
            }
        }
    }
    json!({
        "src": src,
        "forbidden_patterns": forbidden,
        "matches": matches,
    })
}

fn unique_root() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .join("target")
        .join("fsv")
        .join("issue1429_row_safe_ledger_id")
        .join(format!("run-{}-{nanos}", std::process::id()))
}

fn file_state(path: &Path) -> Value {
    json!({
        "path": path,
        "exists": path.exists(),
        "is_file": path.is_file(),
        "is_dir": path.is_dir(),
        "file_count": count_files(path),
        "sha256": file_sha256(path),
    })
}

fn physical_summary(paths: &[PathBuf]) -> Value {
    let stores = paths
        .iter()
        .map(|path| {
            json!({
                "path": path,
                "exists": path.exists(),
                "is_dir": path.is_dir(),
                "file_count": count_files(path),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "total_files": paths.iter().map(|path| count_files(path)).sum::<usize>(),
        "stores": stores,
    })
}

fn count_files(path: &Path) -> usize {
    if path.is_file() {
        return 1;
    }
    if !path.is_dir() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}

fn file_sha256(path: &Path) -> Option<String> {
    if path.is_file() {
        Some(sha256_hex(&fs::read(path).unwrap()))
    } else {
        None
    }
}

fn error_json(error: litd_engine::diagnostics::ErrorReport) -> Value {
    json!({
        "code": error.code,
        "message": error.message,
        "remediation": error.remediation,
        "source_error": error.source_error,
    })
}
